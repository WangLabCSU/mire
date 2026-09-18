use std::convert::Infallible;

use bytes::Bytes;
use rustc_hash::FxHashSet as HashSet;

use crate::domain::{
    actions::{SubseqActions, SubseqPairedActions},
    fragment::ReadFragment,
};
use mire_streaming::{RecordExecutor, RecordSink, RecordSource, Result, WorkflowError};

/// Refinement policy is chosen once per request, before records are processed.
pub(crate) enum RefineReads {
    Single(SubseqActions),
    Paired(SubseqPairedActions),
}

impl RefineReads {
    pub(crate) fn new(
        paired: bool,
        first: Option<SubseqActions>,
        second: Option<SubseqActions>,
    ) -> Result<Self> {
        if first.is_none() && (!paired || second.is_none()) {
            return Err(WorkflowError::InvalidRequest(
                "No sequence actions were specified.".into(),
            ));
        }
        if paired {
            Ok(Self::Paired(SubseqPairedActions::new(first, second)))
        } else {
            first.map(Self::Single).ok_or_else(|| {
                WorkflowError::InvalidRequest("No sequence actions were specified.".into())
            })
        }
    }

    pub(crate) fn execute<S, W, X>(&self, source: S, sink: &mut W, executor: &X) -> Result<()>
    where
        S: RecordSource<Record = ReadFragment> + Send,
        W: RecordSink<ReadFragment>,
        X: RecordExecutor,
    {
        executor.execute(source, sink, |mut fragment| {
            match self {
                Self::Single(actions) => actions.transform_fastq(&mut fragment.read1)?,
                Self::Paired(actions) => {
                    if let Some(second) = &mut fragment.read2 {
                        actions.transform_fastq(&mut fragment.read1, second)?;
                    }
                }
            }
            Ok::<_, crate::domain::actions::SeqActionError>(Some(fragment))
        })
    }
}

pub(crate) struct ExtractReads {
    ids: HashSet<Bytes>,
}

impl ExtractReads {
    pub(crate) fn new(ids: HashSet<Bytes>) -> Self {
        Self { ids }
    }

    pub(crate) fn execute<S, W, X>(&self, source: S, sink: &mut W, executor: &X) -> Result<()>
    where
        S: RecordSource<Record = ReadFragment> + Send,
        W: RecordSink<ReadFragment>,
        X: RecordExecutor,
    {
        executor.execute(source, sink, |fragment| {
            // Compatibility: the original single-end workflow emitted every read.
            // Paired extraction applies the ID selection. Keep these policies distinct.
            let selected = fragment.read2.is_none() || self.ids.contains(fragment.id());
            Ok::<_, Infallible>(selected.then_some(fragment))
        })
    }
}

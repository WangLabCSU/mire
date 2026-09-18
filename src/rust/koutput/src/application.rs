use bytes::Bytes;
use mire_sequence::ReadFragment;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

use crate::domain::{
    joined::{ClassifiedRead, ReadJoin},
    output::{Classification, ClassificationFilter},
};
use mire_streaming::{RecordExecutor, RecordSink, RecordSource, Result};

pub(crate) type Classifications = HashMap<Bytes, Classification>;

/// Filter original Kraken lines without changing their serialized representation.
pub(crate) fn extract_classifications<S, W, X>(
    source: S,
    sink: &mut W,
    executor: &X,
    filter: &ClassificationFilter,
) -> Result<()>
where
    S: RecordSource<Record = Bytes> + Send,
    W: RecordSink<Bytes>,
    X: RecordExecutor,
{
    executor.execute(source, sink, |line| {
        Ok::<_, std::convert::Infallible>(filter.matches_line(&line).then_some(line))
    })
}

struct ClassificationCollector(Classifications);

impl RecordSink<Classification> for ClassificationCollector {
    fn write_record(&mut self, record: Classification) -> Result<()> {
        self.0.insert(record.read_id.clone(), record);
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}

pub(crate) fn load_classifications<S, X>(
    source: S,
    executor: &X,
    filter: &ClassificationFilter,
) -> Result<Classifications>
where
    S: RecordSource<Record = Bytes> + Send,
    X: RecordExecutor,
{
    let mut sink = ClassificationCollector(HashMap::default());
    executor.execute(source, &mut sink, |line| {
        Ok::<_, crate::domain::output::KrakenOutputError>(
            if filter.accepts_classified_line(&line) {
                Classification::parse(&line)?
            } else {
                None
            },
        )
    })?;
    Ok(sink.0)
}

pub(crate) fn read_ids(source: &mut impl RecordSource<Record = Bytes>) -> Result<HashSet<Bytes>> {
    let mut ids = HashSet::default();
    while let Some(line) = source.next_record()? {
        // ID extraction only uses column two; it does not validate classifications.
        if let Ok(line) = std::str::from_utf8(&line) {
            if let Some(id) = line.split('\t').nth(1).filter(|id| !id.is_empty()) {
                ids.insert(Bytes::copy_from_slice(id.as_bytes()));
            }
        }
    }
    Ok(ids)
}

pub(crate) fn join_reads<S, W, X>(
    source: S,
    sink: &mut W,
    executor: &X,
    classifications: &Classifications,
    join: &ReadJoin,
) -> Result<()>
where
    S: RecordSource<Record = ReadFragment> + Send,
    W: RecordSink<ClassifiedRead>,
    X: RecordExecutor,
{
    executor.execute(source, sink, |fragment| {
        classifications
            .get(fragment.id())
            .map(|classification| join.join(classification, fragment))
            .transpose()
    })
}

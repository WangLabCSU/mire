use bytes::{Bytes, BytesMut};
use rustc_hash::FxHashMap as HashMap;
use thiserror::Error;

use crate::{
    domain::range::{SeqRangeError, SeqRanges},
    domain::record::FastqRecord,
    domain::tag::{make_description, TagRangeError, TagRanges},
};

#[derive(Debug, Error)]
pub enum SeqActionError {
    #[error(transparent)]
    Range(#[from] SeqRangeError),
    #[error(transparent)]
    Tag(#[from] TagRangeError),
    #[error("Read '{read_id}' has mismatched sequence and quality lengths ({sequence_len} and {quality_len})")]
    UnequalLength {
        read_id: String,
        sequence_len: usize,
        quality_len: usize,
    },
}

type Result<T> = std::result::Result<T, SeqActionError>;

fn validate_read(record: &FastqRecord<Bytes>) -> Result<()> {
    if record.seq.len() != record.qual.len() {
        return Err(SeqActionError::UnequalLength {
            read_id: String::from_utf8_lossy(&record.id).into_owned(),
            sequence_len: record.seq.len(),
            quality_len: record.qual.len(),
        });
    }
    Ok(())
}

pub(crate) struct SubseqEmbedActions {
    tags: TagRanges,
}

impl SubseqEmbedActions {
    fn new(tags: TagRanges) -> Self {
        Self { tags }
    }

    fn has_action(&self) -> bool {
        !self.tags.is_empty()
    }

    fn embed(&self, record: &mut FastqRecord<Bytes>) -> Result<()> {
        if self.has_action() {
            let tag_map = self.tags.map_sequences(&record.seq)?;
            record.desc = Some(make_description(&tag_map, record.desc.as_deref()));
        }
        Ok(())
    }
}

struct SubseqTrimActions {
    ranges: SeqRanges,
}

impl SubseqTrimActions {
    fn new(ranges: SeqRanges) -> Self {
        Self { ranges }
    }

    fn has_action(&self) -> bool {
        !self.ranges.is_empty()
    }

    /// Trim the sequence and quality string.
    /// If any range is out of bounds, it will return an error.
    fn trim(&self, record: &mut FastqRecord<Bytes>) -> Result<()> {
        if !self.has_action() {
            return Ok(());
        }
        let keep = self.ranges.retained_segments(record.seq.len())?;
        let total_seq_len = keep.iter().map(|range| range.len()).sum();
        let mut trimmed_seq = BytesMut::with_capacity(total_seq_len);
        let mut trimmed_qual = BytesMut::with_capacity(total_seq_len);

        for range in keep {
            trimmed_seq.extend_from_slice(&record.seq[range.clone()]);
            trimmed_qual.extend_from_slice(&record.qual[range]);
        }
        record.seq = trimmed_seq.freeze();
        record.qual = trimmed_qual.freeze();
        Ok(())
    }
}

pub struct SubseqActions {
    embed: SubseqEmbedActions,
    trim: SubseqTrimActions,
}

impl SubseqActions {
    pub fn builder() -> SubseqActionsBuilder {
        SubseqActionsBuilder::default()
    }

    pub(crate) fn transform_fastq(&self, record: &mut FastqRecord<Bytes>) -> Result<()> {
        validate_read(record)?;
        self.embed.embed(record)?;
        self.trim.trim(record)?;

        Ok(())
    }
}

/// Paired-end FASTQ transformation logic using optional tag embedding and trimming.
/// Each read end (read1 or read2) can have its own independent action configuration.
pub(crate) struct SubseqPairedActions {
    actions1: Option<SubseqActions>,
    actions2: Option<SubseqActions>,
}

impl SubseqPairedActions {
    pub fn new(actions1: Option<SubseqActions>, actions2: Option<SubseqActions>) -> Self {
        Self { actions1, actions2 }
    }

    pub(crate) fn transform_fastq(
        &self,
        record1: &mut FastqRecord<Bytes>,
        record2: &mut FastqRecord<Bytes>,
    ) -> Result<()> {
        validate_read(record1)?;
        validate_read(record2)?;

        // Apply tag embedding into `desc` fields
        self.embedded_labels(record1, record2)?;

        // Apply trimming logic for read1
        if let Some(actions) = &self.actions1 {
            actions.trim.trim(record1)?;
        }

        // Apply trimming logic for read2
        if let Some(actions) = &self.actions2 {
            actions.trim.trim(record2)?;
        }
        Ok(())
    }

    /// Embeds tag labels extracted from read1, read2, or both into FASTQ description fields.
    ///
    /// Behavior:
    /// - If only one side has embedding, only that side contributes tags.
    /// - If both sides have embedding, tags are merged by key. If the same tag exists in both,
    ///   the sequences are concatenated in read1-first, read2-second order.
    ///
    /// Both reads receive the combined tags in their FASTQ descriptions.
    fn embedded_labels(
        &self,
        record1: &mut FastqRecord<Bytes>,
        record2: &mut FastqRecord<Bytes>,
    ) -> Result<()> {
        let tag_map = match (&self.actions1, &self.actions2) {
            (Some(actions), None) => actions.embed.tags.map_sequences(&record1.seq)?,
            (None, Some(actions)) => actions.embed.tags.map_sequences(&record2.seq)?,
            (Some(actions1), Some(actions2)) => {
                let mut tag_map = actions1.embed.tags.map_sequences(&record1.seq)?;
                let tag_map2 = actions2.embed.tags.map_sequences(&record2.seq)?;

                // Merge tag→sequence entries
                for (tag, sequences) in tag_map2 {
                    if let Some(v) = tag_map.get_mut(&tag) {
                        v.extend(sequences); // read1 first, read2 second
                    } else {
                        tag_map.insert(tag, sequences);
                    }
                }
                tag_map
            }
            (None, None) => return Ok(()),
        };

        // Only write to description fields if any tag was collected
        if !tag_map.is_empty() {
            record1.desc = Some(make_description(&tag_map, record1.desc.as_deref()));
            record2.desc = Some(make_description(&tag_map, record2.desc.as_deref()));
        }
        Ok(())
    }
}

/// Builder pattern for constructing `SubseqActions` step-by-step.
/// Allows the user to accumulate multiple `embed` and `trim` operations,
/// validating them before finalizing into a concrete `SubseqActions` instance.
#[derive(Default)]
pub struct SubseqActionsBuilder {
    embed_list: Vec<(Bytes, SeqRanges)>,
    trim_list: Vec<SeqRanges>,
}

impl SubseqActionsBuilder {
    /// Adds a new embedded tag action with range validation.
    /// Ensures that ranges are sorted and non-overlapping before storing.
    fn add_embed(&mut self, tag: Bytes, ranges: SeqRanges) -> Result<()> {
        ranges.validate_extraction()?;
        self.embed_list.push((tag, ranges));
        Ok(())
    }

    /// Adds validated trim ranges. Bounds against individual reads are checked
    /// during transformation; overlapping trim ranges are allowed.
    fn add_trim(&mut self, ranges: SeqRanges) {
        self.trim_list.push(ranges);
    }

    /// Adds a compound action to the builder.
    pub fn add_action(&mut self, action: SeqAction, ranges: SeqRanges) -> Result<()> {
        match action {
            SeqAction::Embed(tag) => {
                self.add_embed(tag, ranges)?;
            }
            SeqAction::Trim => {
                self.add_trim(ranges);
            }
            SeqAction::EmbedTrim(tag) => {
                self.add_embed(tag, ranges.clone())?;
                self.add_trim(ranges);
            }
        }
        Ok(())
    }

    /// Builds actions, sorting trim ranges across all accumulated operations.
    /// Tag extraction rejects overlaps; trimming removes their union.
    pub fn build(self) -> Result<SubseqActions> {
        let tag_ranges = self
            .embed_list
            .into_iter()
            .collect::<HashMap<Bytes, SeqRanges>>();
        let tags = TagRanges::new(tag_ranges)?;
        let embed_actions = SubseqEmbedActions::new(tags);

        // Flatten all trim ranges into a single sorted list
        let full_ranges: SeqRanges = self.trim_list.into_iter().flatten().collect();

        // Construct the final SubseqActions struct
        Ok(SubseqActions {
            embed: embed_actions,
            trim: SubseqTrimActions::new(full_ranges),
        })
    }
}

pub enum SeqAction {
    Embed(Bytes),
    Trim,
    EmbedTrim(Bytes),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::range::SeqRange;

    fn record(seq: &'static [u8], qual: &'static [u8]) -> FastqRecord<Bytes> {
        FastqRecord::new(
            Bytes::from_static(b"read1"),
            Some(Bytes::from_static(b"original description")),
            Bytes::from_static(seq),
            Bytes::from_static(b"+"),
            Bytes::from_static(qual),
        )
    }

    fn actions(action: SeqAction, bounds: &[(Option<usize>, Option<usize>)]) -> SubseqActions {
        let ranges = bounds
            .iter()
            .map(|&(start, end)| SeqRange::build(start, end).unwrap())
            .collect();
        let mut builder = SubseqActions::builder();
        builder.add_action(action, ranges).unwrap();
        builder.build().unwrap()
    }

    #[test]
    fn embeds_original_bases_before_trimming_sequence_and_quality() {
        let actions = actions(
            SeqAction::EmbedTrim(Bytes::from_static(b"UMI")),
            &[(Some(1), Some(3))],
        );
        let mut read = record(b"ACGT", b"1234");
        actions.transform_fastq(&mut read).unwrap();
        assert_eq!(read.seq.as_ref(), b"AT");
        assert_eq!(read.qual.as_ref(), b"14");
        assert_eq!(
            read.desc.unwrap().as_ref(),
            b"original description MIRE{UMI:CG}"
        );
    }

    #[test]
    fn trimming_removes_the_union_of_overlapping_ranges() {
        let actions = actions(SeqAction::Trim, &[(Some(2), None), (Some(5), Some(7))]);
        let mut read = record(b"ACGTACGT", b"12345678");
        actions.transform_fastq(&mut read).unwrap();
        assert_eq!(read.seq.as_ref(), b"AC");
        assert_eq!(read.qual.as_ref(), b"12");
    }

    #[test]
    fn mismatched_sequence_and_quality_returns_an_error() {
        let actions = actions(SeqAction::Trim, &[(None, Some(1))]);
        let mut read = record(b"ACGT", b"12");
        assert!(actions.transform_fastq(&mut read).is_err());
    }

    #[test]
    fn paired_tags_are_merged_in_read_order_before_trimming() {
        let paired = SubseqPairedActions::new(
            Some(actions(
                SeqAction::EmbedTrim(Bytes::from_static(b"UMI")),
                &[(None, Some(2))],
            )),
            Some(actions(
                SeqAction::EmbedTrim(Bytes::from_static(b"UMI")),
                &[(Some(2), None)],
            )),
        );
        let mut read1 = record(b"ACGT", b"1234");
        let mut read2 = record(b"TGCA", b"5678");
        paired.transform_fastq(&mut read1, &mut read2).unwrap();
        assert_eq!(read1.seq.as_ref(), b"GT");
        assert_eq!(read1.qual.as_ref(), b"34");
        assert_eq!(read2.seq.as_ref(), b"TG");
        assert_eq!(read2.qual.as_ref(), b"56");
        assert_eq!(read1.desc, read2.desc);
        assert_eq!(
            read1.desc.unwrap().as_ref(),
            b"original description MIRE{UMI:ACCA}"
        );
    }

    #[test]
    fn empty_actions_preserve_empty_reads() {
        let actions = SubseqActions::builder().build().unwrap();
        let mut read = record(b"", b"");
        actions.transform_fastq(&mut read).unwrap();
        assert!(read.seq.is_empty());
        assert!(read.qual.is_empty());
        assert_eq!(read.desc.unwrap().as_ref(), b"original description");
    }

    #[test]
    fn trim_can_remove_the_entire_read() {
        let actions = actions(SeqAction::Trim, &[(None, Some(4))]);
        let mut read = record(b"ACGT", b"1234");
        actions.transform_fastq(&mut read).unwrap();
        assert!(read.seq.is_empty());
        assert!(read.qual.is_empty());
    }
}

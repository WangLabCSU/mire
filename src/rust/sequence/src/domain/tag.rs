use bytes::{BufMut, Bytes, BytesMut};
use rustc_hash::FxHashMap as HashMap;
use thiserror::Error;

use crate::domain::range::{SeqRangeError, SeqRanges};

/// Start of a MIRE tag annotation in a FASTQ description.
pub const TAG_PREFIX: &[u8] = b"MIRE{";
/// End of a MIRE tag annotation in a FASTQ description.
pub const TAG_SUFFIX: u8 = b'}';

/// Borrowed sequence fragments keyed by their tag name.
pub type TaggedSequences<'seq> = HashMap<Bytes, Vec<&'seq [u8]>>;

#[derive(Debug, Error)]
#[error("Invalid ranges for tag '{tag}': {source}")]
pub struct TagRangeError {
    tag: String,
    #[source]
    source: SeqRangeError,
}

impl TagRangeError {
    fn new(tag: &[u8], source: SeqRangeError) -> Self {
        Self {
            tag: String::from_utf8_lossy(tag).into_owned(),
            source,
        }
    }
}

/// Tag ranges are always sorted and non-overlapping within each tag.
#[derive(Debug)]
pub struct TagRanges {
    map: HashMap<Bytes, SeqRanges>,
}

impl TagRanges {
    /// Validate that extraction ranges for each tag do not overlap.
    ///
    /// # Errors
    /// Returns the tag name and conflicting ranges when a tag contains overlaps.
    pub fn new(map: HashMap<Bytes, SeqRanges>) -> Result<Self, TagRangeError> {
        for (tag, ranges) in &map {
            ranges
                .validate_extraction()
                .map_err(|error| TagRangeError::new(tag, error))?;
        }
        Ok(Self { map })
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Borrows subsequences in sequence order for each tag.
    ///
    /// # Errors
    /// Returns the tag name and bounds error if a range exceeds the sequence.
    pub fn map_sequences<'seq>(
        &self,
        seq: &'seq [u8],
    ) -> Result<TaggedSequences<'seq>, TagRangeError> {
        self.map
            .iter()
            .map(|(tag, ranges)| {
                let sequences = ranges
                    .iter()
                    .map(|range| range.try_extract(seq))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| TagRangeError::new(tag, error))?;
                Ok((tag.clone(), sequences))
            })
            .collect()
    }
}

/// Appends MIRE tags to a read description, preserving fragment order per tag.
pub(crate) fn make_description(tag_map: &TaggedSequences<'_>, desc: Option<&[u8]>) -> Bytes {
    let mut out = BytesMut::with_capacity(
        // original description length
        desc.map_or(0, |d| d.len() + 1)
            // prefix
            + TAG_PREFIX.len()
            // all tag
            + tag_map.iter().map(|(tag, sequence)| tag.len() + 1 + sequence.iter().map(|s| s.len()).sum::<usize>()).sum::<usize>()
            // separators between tags
            + tag_map.len().saturating_sub(1)
            // suffix
            + 1,
    );
    if let Some(v) = desc {
        out.extend_from_slice(v);
        out.put_u8(b' ');
    }
    out.extend_from_slice(TAG_PREFIX);
    for (i, (tag, sequences)) in tag_map.iter().enumerate() {
        if i > 0 {
            out.put_u8(b':');
        }
        out.extend_from_slice(tag);
        out.put_u8(b':');
        for seq in sequences {
            out.extend_from_slice(seq);
        }
    }
    out.put_u8(TAG_SUFFIX);
    out.freeze()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::range::SeqRange;

    fn tag_ranges(bounds: &[(Option<usize>, Option<usize>)]) -> HashMap<Bytes, SeqRanges> {
        std::iter::once((
            Bytes::from_static(b"UMI"),
            bounds
                .iter()
                .map(|&(start, end)| SeqRange::build(start, end).unwrap())
                .collect(),
        ))
        .collect()
    }

    #[test]
    fn constructor_sorts_ranges_for_every_caller() {
        let tags = TagRanges::new(tag_ranges(&[(Some(3), None), (None, Some(2))])).unwrap();
        let extracted = tags.map_sequences(b"ACGT").unwrap();
        assert_eq!(
            extracted[b"UMI".as_slice()],
            vec![b"AC".as_slice(), b"T".as_slice()]
        );
    }

    #[test]
    fn constructor_rejects_overlaps_with_tag_context() {
        let error = TagRanges::new(tag_ranges(&[(None, Some(3)), (Some(2), None)])).unwrap_err();
        assert_eq!(error.tag, "UMI");
        assert!(matches!(error.source, SeqRangeError::Overlap { .. }));
    }

    #[test]
    fn extraction_keeps_typed_bounds_error_and_tag_name() {
        let tags = TagRanges::new(tag_ranges(&[(None, Some(5))])).unwrap();
        let error = tags.map_sequences(b"ACGT").unwrap_err();
        assert_eq!(error.tag, "UMI");
        assert_eq!(
            error.source,
            SeqRangeError::PrefixOutOfBounds { end: 5, len: 4 }
        );
    }
}

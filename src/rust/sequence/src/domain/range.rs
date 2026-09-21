use std::ops::Range;

use thiserror::Error;

/// Invalid sequence bounds or conflicting extraction ranges.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SeqRangeError {
    #[error("at least one of 'start' or 'end' must be provided.")]
    MissingBounds,
    #[error("start {start} is greater than or equal to end {end}")]
    InvalidSpan { start: usize, end: usize },
    #[error("RangeTo end {end} out of bounds (len = {len})")]
    PrefixOutOfBounds { end: usize, len: usize },
    #[error("Range end {end} out of bounds (len = {len})")]
    EndOutOfBounds { end: usize, len: usize },
    #[error("RangeFrom start {start} out of bounds (len = {len})")]
    StartOutOfBounds { start: usize, len: usize },
    #[error("Overlapping ranges: {left:?} overlaps with {right:?}")]
    Overlap { left: SeqRange, right: SeqRange },
}

/// Validated, zero-based sequence bounds with an exclusive end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqRange {
    start: Option<usize>,
    end: Option<usize>,
}

impl SeqRange {
    /// Constructs a range, for example `build(Some(0), Some(4))` for `0..4`.
    ///
    /// # Errors
    /// Requires at least one bound and `start < end` when both are supplied.
    pub fn build(start: Option<usize>, end: Option<usize>) -> Result<Self, SeqRangeError> {
        match (start, end) {
            (None, None) => Err(SeqRangeError::MissingBounds),
            (Some(start), Some(end)) if start >= end => {
                Err(SeqRangeError::InvalidSpan { start, end })
            }
            _ => Ok(Self { start, end }),
        }
    }

    /// Resolves open bounds and checks that the range fits a sequence.
    ///
    /// # Errors
    /// Returns a bounds error if an end exceeds `len`, or an open-ended
    /// range starts at or beyond `len`.
    pub fn resolve(&self, len: usize) -> Result<Range<usize>, SeqRangeError> {
        match (self.start, self.end) {
            (None, Some(end)) if end > len => Err(SeqRangeError::PrefixOutOfBounds { end, len }),
            (Some(_), Some(end)) if end > len => Err(SeqRangeError::EndOutOfBounds { end, len }),
            (Some(start), None) if start >= len => {
                Err(SeqRangeError::StartOutOfBounds { start, len })
            }
            _ => Ok(self.start.unwrap_or(0)..self.end.unwrap_or(len)),
        }
    }

    /// Borrows the selected bases, returning an error for out-of-bounds ranges.
    pub(crate) fn try_extract<'seq>(
        &self,
        sequence: &'seq [u8],
    ) -> Result<&'seq [u8], SeqRangeError> {
        Ok(&sequence[self.resolve(sequence.len())?])
    }
}

/// Sequence ranges kept in ascending coordinate order at construction time.
#[derive(Debug, Clone, Default)]
pub struct SeqRanges(Vec<SeqRange>);

impl SeqRanges {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, SeqRange> {
        self.0.iter()
    }

    /// Rejects overlapping extraction ranges.
    pub fn validate_extraction(&self) -> Result<(), SeqRangeError> {
        for pair in self.0.windows(2) {
            if pair[0]
                .end
                .is_none_or(|end| end > pair[1].start.unwrap_or(0))
            {
                return Err(SeqRangeError::Overlap {
                    left: pair[0].clone(),
                    right: pair[1].clone(),
                });
            }
        }
        Ok(())
    }

    /// Returns the complement of sorted trim ranges, treating overlaps as a union.
    ///
    /// # Errors
    /// Checks every range against `len`, including ranges covered by another one.
    pub fn retained_segments(&self, len: usize) -> Result<Vec<Range<usize>>, SeqRangeError> {
        let mut keep = Vec::new();
        let mut cursor = 0;
        for range in &self.0 {
            let range = range.resolve(len)?;
            if cursor < range.start {
                keep.push(cursor..range.start);
            }
            cursor = cursor.max(range.end);
        }
        if cursor < len {
            keep.push(cursor..len);
        }
        Ok(keep)
    }
}

impl FromIterator<SeqRange> for SeqRanges {
    fn from_iter<T: IntoIterator<Item = SeqRange>>(iter: T) -> Self {
        let mut ranges: Vec<_> = iter.into_iter().collect();
        ranges.sort_by_key(|range| (range.start.unwrap_or(0), range.end.unwrap_or(usize::MAX)));
        Self(ranges)
    }
}

impl IntoIterator for SeqRanges {
    type IntoIter = std::vec::IntoIter<SeqRange>;
    type Item = SeqRange;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_or_reversed_bounds() {
        assert_eq!(
            SeqRange::build(None, None),
            Err(SeqRangeError::MissingBounds)
        );
        for (start, end) in [(2, 2), (3, 2)] {
            assert_eq!(
                SeqRange::build(Some(start), Some(end)),
                Err(SeqRangeError::InvalidSpan { start, end })
            );
        }
    }

    #[test]
    fn extracts_bounded_prefix_and_suffix_ranges() {
        let sequence = b"ACGT";
        for (start, end, expected) in [
            (Some(1), Some(3), b"CG".as_slice()),
            (None, Some(2), b"AC".as_slice()),
            (Some(2), None, b"GT".as_slice()),
            (Some(3), Some(4), b"T".as_slice()),
        ] {
            assert_eq!(
                SeqRange::build(start, end)
                    .unwrap()
                    .try_extract(sequence)
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn rejects_out_of_bounds_without_panicking() {
        for (start, end) in [
            (None, Some(5)),
            (Some(2), Some(5)),
            (Some(4), None),
            (Some(usize::MAX), None),
        ] {
            assert!(SeqRange::build(start, end)
                .unwrap()
                .try_extract(b"ACGT")
                .is_err());
        }
        assert!(SeqRange::build(Some(0), None)
            .unwrap()
            .try_extract(b"")
            .is_err());
        assert_eq!(
            SeqRange::build(None, Some(0))
                .unwrap()
                .try_extract(b"")
                .unwrap(),
            b""
        );
    }

    #[test]
    fn extraction_accepts_adjacent_ranges_and_rejects_overlaps() {
        let prefix = SeqRange::build(None, Some(2)).unwrap();
        let suffix = SeqRange::build(Some(2), None).unwrap();
        let adjacent: SeqRanges = [suffix.clone(), prefix.clone()].into_iter().collect();
        adjacent.validate_extraction().unwrap();
        assert_eq!(adjacent.iter().next(), Some(&prefix));
        let overlapping: SeqRanges = [suffix, SeqRange::build(Some(1), Some(3)).unwrap()]
            .into_iter()
            .collect();
        assert!(matches!(
            overlapping.validate_extraction(),
            Err(SeqRangeError::Overlap { .. })
        ));
    }

    #[test]
    fn validates_even_trim_ranges_covered_by_a_suffix() {
        let ranges: SeqRanges = [
            SeqRange::build(Some(0), None).unwrap(),
            SeqRange::build(Some(2), Some(10)).unwrap(),
        ]
        .into_iter()
        .collect();
        assert!(ranges.retained_segments(4).is_err());
    }

    #[test]
    fn trim_union_matches_a_per_base_mask_for_all_small_range_pairs() {
        for len in 1..=6 {
            let mut candidates = Vec::new();
            for start in 0..len {
                candidates.push(SeqRange::build(Some(start), None).unwrap());
                for end in start + 1..=len {
                    candidates.push(SeqRange::build(Some(start), Some(end)).unwrap());
                }
            }
            for end in 0..=len {
                candidates.push(SeqRange::build(None, Some(end)).unwrap());
            }
            for left in &candidates {
                for right in &candidates {
                    let ranges: SeqRanges = [left.clone(), right.clone()].into_iter().collect();
                    let expected: Vec<_> = (0..len)
                        .filter(|base| {
                            !left.resolve(len).unwrap().contains(base)
                                && !right.resolve(len).unwrap().contains(base)
                        })
                        .collect();
                    let retained: Vec<_> = ranges
                        .retained_segments(len)
                        .unwrap()
                        .into_iter()
                        .flatten()
                        .collect();
                    assert_eq!(retained, expected, "{ranges:?}, length {len}");
                }
            }
        }
    }
}

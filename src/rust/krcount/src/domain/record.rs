use thiserror::Error;

use super::kmer::KmerError;

#[derive(Debug, Error)]
pub(crate) enum CountError {
    #[error("Koutreads record must have 5 fields, got {0}")]
    FieldCount(usize),
    #[error("Tag '{0}' not found in input")]
    MissingTag(String),
    #[error("Tag '{0}' found, but no value follows")]
    EmptyTag(String),
    #[error(transparent)]
    Kmer(#[from] KmerError),
}

/// Borrowed, validated fields of the five-column classified-read format.
pub(crate) struct CountRecord<'a> {
    pub(crate) taxid: &'a [u8],
    pub(crate) tags: &'a [u8],
    pub(crate) lca: &'a [u8],
    pub(crate) sequences: Vec<&'a [u8]>,
    quality: &'a [u8],
    sequence: &'a [u8],
}

impl<'a> CountRecord<'a> {
    pub(crate) fn parse(line: &'a [u8]) -> Result<Option<Self>, CountError> {
        if line.trim_ascii().is_empty() {
            return Ok(None);
        }
        let fields: Vec<_> = line.split(|byte| *byte == b'\t').collect();
        let [taxid, tags, lca, sequences, qualities] = fields.as_slice() else {
            return Err(CountError::FieldCount(fields.len()));
        };
        let sequence = *sequences;
        let quality = *qualities;
        let sequences: Vec<_> = sequences.split(|byte| *byte == b' ').collect();
        Ok(Some(Self {
            taxid,
            tags,
            lca,
            sequences,
            quality,
            sequence,
        }))
    }

    pub(crate) fn passes_filters(&self) -> bool {
        // Apply the existing quality rule to the complete serialized field.
        // A paired separator is therefore filtered just as in the original implementation.
        self.quality.iter().all(|base| *base >= 53) && passes_complexity(self.sequence, 20)
    }

    pub(crate) fn tag(&self, label: Option<&str>) -> Result<Option<&'a [u8]>, CountError> {
        let Some(label) = label else {
            return Ok(None);
        };
        // Preserve substring lookup for existing label conventions.
        let start = memchr::memmem::find(self.tags, label.as_bytes())
            .ok_or_else(|| CountError::MissingTag(label.to_owned()))?
            + label.len()
            + 1;
        if start >= self.tags.len() {
            return Err(CountError::EmptyTag(label.to_owned()));
        }
        let value = &self.tags[start..];
        let end = value
            .iter()
            .position(|byte| *byte == b' ')
            .unwrap_or(value.len());
        Ok(Some(&value[..end]))
    }
}

fn passes_complexity(sequence: &[u8], minimum_non_repeated: usize) -> bool {
    // Preserve release-build threshold arithmetic for short reads.
    // Explicit wrapping also makes debug and release behavior agree.
    let limit = sequence.len().wrapping_sub(minimum_non_repeated);
    let mut counts = [0usize; 256];
    for &base in sequence {
        counts[base as usize] += 1;
        if counts[base as usize] > limit {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_sequences_preserve_release_threshold_arithmetic() {
        assert!(passes_complexity(b"ACGT", 20));
        assert!(passes_complexity(b"", 20));
    }

    #[test]
    fn tags_preserve_substring_lookup() {
        let record = CountRecord::parse(b"2\tNOTUMI:bad UMI:good\t2:1\tAC\tII")
            .unwrap()
            .unwrap();
        assert_eq!(record.tag(Some("UMI")).unwrap(), Some(b"bad".as_slice()));
        assert!(record.tag(Some("BARCODE")).is_err());
    }

    #[test]
    fn paired_quality_preserves_serialized_field_filter() {
        let sequence = "ACGT".repeat(10);
        let quality = "I".repeat(40);
        let line = format!("2\t\t2:10 |:| 2:10\t{sequence} {sequence}\t{quality} {quality}");
        let record = CountRecord::parse(line.as_bytes()).unwrap().unwrap();
        assert!(!record.passes_filters());
    }
}

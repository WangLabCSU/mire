use bytes::{Bytes, BytesMut};
use rustc_hash::FxHashMap as HashMap;
use thiserror::Error;

use super::output::{Classification, KrakenOutputError, ReadLengths};
use mire_sequence::{Read, ReadFragment, TagRangeError, TagRanges, TAG_PREFIX, TAG_SUFFIX};

#[derive(Debug, Error)]
pub(crate) enum ReadJoinError {
    #[error(transparent)]
    Classification(#[from] KrakenOutputError),
    #[error(
        "Sequence length mismatch for read '{read_id}': expected {expected:?}, got {actual:?}"
    )]
    Length {
        read_id: String,
        expected: ReadLengths,
        actual: ReadLengths,
    },
    #[error("Invalid MIRE tag annotation '{0}'")]
    TagAnnotation(String),
    #[error(transparent)]
    TagRanges(#[from] TagRangeError),
}

pub(crate) struct ReadEnd {
    pub(crate) sequence: Bytes,
    pub(crate) quality: Bytes,
}

/// One classified read (or read pair), including its extracted tags.
pub(crate) struct ClassifiedRead {
    pub(crate) taxid: Bytes,
    pub(crate) lca: Bytes,
    pub(crate) tags: HashMap<Bytes, Bytes>,
    pub(crate) first: Option<ReadEnd>,
    pub(crate) last: ReadEnd,
}

pub struct ReadJoin {
    pub first_tags: Option<TagRanges>,
    pub second_tags: Option<TagRanges>,
}

impl ReadJoin {
    pub(crate) fn join(
        &self,
        classification: &Classification,
        fragment: ReadFragment,
    ) -> Result<ClassifiedRead, ReadJoinError> {
        let lengths = ReadLengths::parse(&classification.lengths)?;
        validate_lengths(&lengths, &fragment)?;
        let mut tags = description_tags(fragment.first().description())?;
        if let Some(second) = fragment.second() {
            tags.extend(description_tags(second.description())?);
        }
        tags.extend(self.sequence_tags(&fragment)?);
        let end = |read: Read| {
            let (sequence, quality) = read.into_sequence_and_quality();
            ReadEnd { sequence, quality }
        };
        let (read1, read2) = fragment.into_reads();
        let (first, last) = match (read2, &lengths) {
            (Some(second), ReadLengths::Paired(_, _)) => (Some(end(read1)), end(second)),
            (Some(second), ReadLengths::Single(_)) => (None, end(second)),
            (None, _) => (None, end(read1)),
        };
        Ok(ClassifiedRead {
            taxid: classification.taxid.clone(),
            lca: classification.lca.clone(),
            tags,
            first,
            last,
        })
    }

    fn sequence_tags(
        &self,
        fragment: &ReadFragment,
    ) -> Result<HashMap<Bytes, Bytes>, TagRangeError> {
        let mut tags: HashMap<Bytes, BytesMut> = HashMap::default();
        for (ranges, read) in [
            (self.first_tags.as_ref(), Some(fragment.first())),
            (self.second_tags.as_ref(), fragment.second()),
        ] {
            if let (Some(ranges), Some(read)) = (ranges, read) {
                for (tag, sequences) in ranges.map_sequences(read.sequence())? {
                    let value = tags.entry(tag).or_default();
                    for sequence in sequences {
                        value.extend_from_slice(sequence);
                    }
                }
            }
        }
        Ok(tags
            .into_iter()
            .map(|(tag, value)| (tag, value.freeze()))
            .collect())
    }
}

fn validate_lengths(expected: &ReadLengths, fragment: &ReadFragment) -> Result<(), ReadJoinError> {
    let actual = match (expected, fragment.second()) {
        (ReadLengths::Paired(_, _), Some(second)) => {
            ReadLengths::Paired(fragment.first().sequence().len(), second.sequence().len())
        }
        (ReadLengths::Single(_), Some(second)) => ReadLengths::Single(second.sequence().len()),
        (_, None) => ReadLengths::Single(fragment.first().sequence().len()),
    };
    if *expected != actual {
        return Err(ReadJoinError::Length {
            read_id: String::from_utf8_lossy(fragment.id()).into_owned(),
            expected: expected.clone(),
            actual,
        });
    }
    Ok(())
}

fn description_tags(desc: Option<&Bytes>) -> Result<HashMap<Bytes, Bytes>, ReadJoinError> {
    let mut tags = HashMap::default();
    let Some(desc) = desc else {
        return Ok(tags);
    };
    let Some(start) = desc
        .windows(TAG_PREFIX.len())
        .position(|part| part == TAG_PREFIX)
    else {
        return Ok(tags);
    };
    let tail = &desc[start + TAG_PREFIX.len()..];
    let Some(end) = tail.iter().position(|byte| *byte == TAG_SUFFIX) else {
        return Ok(tags);
    };
    let annotation = &tail[..end];
    if annotation.is_empty() {
        return Ok(tags);
    }
    let mut fields = annotation.split(|byte| *byte == b':');
    while let Some(tag) = fields.next() {
        let value = fields
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ReadJoinError::TagAnnotation(String::from_utf8_lossy(annotation).into_owned())
            })?;
        if tag.is_empty() {
            return Err(ReadJoinError::TagAnnotation(
                String::from_utf8_lossy(annotation).into_owned(),
            ));
        }
        tags.insert(Bytes::copy_from_slice(tag), Bytes::copy_from_slice(value));
    }
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_tags_return_an_error_instead_of_looping() {
        for desc in [
            b"MIRE{broken}".as_slice(),
            b"MIRE{UMI:AA:broken}",
            b"MIRE{UMI:}",
        ] {
            assert!(description_tags(Some(&Bytes::copy_from_slice(desc))).is_err());
        }
    }

    #[test]
    fn description_tags_preserve_values() {
        let tags = description_tags(Some(&Bytes::from_static(
            b"original MIRE{UMI:AC:BARCODE:GT}",
        )))
        .unwrap();
        assert_eq!(tags[b"UMI".as_slice()].as_ref(), b"AC");
        assert_eq!(tags[b"BARCODE".as_slice()].as_ref(), b"GT");
    }
}

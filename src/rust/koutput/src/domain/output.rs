use aho_corasick::{AhoCorasick, AhoCorasickKind};
use bytes::Bytes;
use rustc_hash::FxHashSet as HashSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum KrakenOutputError {
    #[error("Kraken output must have 5 tab-separated fields, got {0}")]
    FieldCount(usize),
    #[error("Invalid Kraken classification status '{0}' (expected C or U)")]
    Status(String),
    #[error("Invalid taxid field '{0}'")]
    Taxid(String),
    #[error("Invalid sequence length '{0}'")]
    Length(String),
    #[error("Missing sequence ID in Kraken output")]
    MissingReadId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReadLengths {
    Single(usize),
    Paired(usize, usize),
}

impl ReadLengths {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, KrakenOutputError> {
        let parse = |value: &[u8]| {
            std::str::from_utf8(value.trim_ascii())
                .ok()
                .and_then(|value| value.parse().ok())
                .ok_or_else(|| {
                    KrakenOutputError::Length(String::from_utf8_lossy(bytes).into_owned())
                })
        };
        if let Some(separator) = bytes.iter().position(|byte| *byte == b':') {
            Ok(Self::Paired(
                parse(&bytes[..separator])?,
                parse(&bytes[separator + 1..])?,
            ))
        } else {
            Ok(Self::Single(parse(bytes)?))
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Classification {
    pub(crate) read_id: Bytes,
    pub(crate) taxid: Bytes,
    pub(crate) lengths: Bytes,
    pub(crate) lca: Bytes,
}

impl Classification {
    pub(crate) fn parse(line: &[u8]) -> Result<Option<Self>, KrakenOutputError> {
        if line.trim_ascii().is_empty() {
            return Ok(None);
        }
        let fields: Vec<_> = line.split(|byte| *byte == b'\t').take(5).collect();
        let [status, id, taxon, length, lca] = fields.as_slice() else {
            return Err(KrakenOutputError::FieldCount(fields.len()));
        };
        match *status {
            b"C" | b"U" => {}
            _ => {
                return Err(KrakenOutputError::Status(
                    String::from_utf8_lossy(status).into_owned(),
                ))
            }
        };
        if id.is_empty() {
            return Err(KrakenOutputError::MissingReadId);
        }
        Ok(Some(Self {
            read_id: Bytes::copy_from_slice(id),
            taxid: Bytes::copy_from_slice(parse_taxid(taxon)?),
            lengths: Bytes::copy_from_slice(length),
            lca: Bytes::copy_from_slice(lca),
        }))
    }
}

fn parse_taxid(field: &[u8]) -> Result<&[u8], KrakenOutputError> {
    const PREFIX: &[u8] = b"(taxid ";
    let taxid = if let Some(start) = field.windows(PREFIX.len()).position(|part| part == PREFIX) {
        let tail = &field[start + PREFIX.len()..];
        let end = tail
            .iter()
            .position(|byte| *byte == b')')
            .ok_or_else(|| KrakenOutputError::Taxid(String::from_utf8_lossy(field).into_owned()))?;
        &tail[..end]
    } else {
        field
    };
    if taxid.is_empty() {
        return Err(KrakenOutputError::Taxid(
            String::from_utf8_lossy(field).into_owned(),
        ));
    }
    Ok(taxid)
}

/// Taxon inclusion and LCA exclusion are independent domain predicates.
pub(crate) struct ClassificationFilter {
    included: HashSet<Bytes>,
    excluded_lca: Option<AhoCorasick>,
}

impl ClassificationFilter {
    pub(crate) fn new(
        included: HashSet<Bytes>,
        excluded_lca: Option<HashSet<Bytes>>,
    ) -> Result<Self, aho_corasick::BuildError> {
        let excluded_lca = excluded_lca
            .map(|taxids| {
                let patterns = taxids
                    .into_iter()
                    .map(|taxid| [taxid.as_ref(), b":"].concat());
                AhoCorasick::builder()
                    .kind(Some(AhoCorasickKind::DFA))
                    .build(patterns)
            })
            .transpose()?;
        Ok(Self {
            included,
            excluded_lca,
        })
    }

    fn excludes(&self, lca: &[u8]) -> bool {
        // Compatibility: exclusions historically matched the byte pattern "taxid:"
        // anywhere in LCA text, including suffixes of longer taxids.
        self.excluded_lca
            .as_ref()
            .is_some_and(|matcher| matcher.is_match(lca))
    }

    #[cfg(test)]
    fn matches(&self, record: &Classification) -> bool {
        self.included.contains(&record.taxid) && !self.excludes(&record.lca)
    }

    pub(crate) fn accepts_classified_line(&self, line: &[u8]) -> bool {
        let fields: Vec<_> = line.split(|byte| *byte == b'\t').take(5).collect();
        let [b"C", _, taxon, _, lca] = fields.as_slice() else {
            return false;
        };
        parse_taxid(taxon).is_ok_and(|taxid| self.included.contains(taxid)) && !self.excludes(lca)
    }

    /// Match included taxids, applying LCA exclusions when supplied.
    /// Named taxids also match when exclusions are supplied and none are found.
    pub(crate) fn matches_line(&self, line: &[u8]) -> bool {
        // Preserve the extraction workflow's existing named-taxid selection policy.
        let mut fields = line.split(|byte| *byte == b'\t');
        let Some(taxon) = fields.nth(2) else {
            return false;
        };
        let Ok(taxid) = parse_taxid(taxon) else {
            return false;
        };
        if fields.next().is_none() {
            return false;
        }
        let included = self.included.contains(taxid);
        let named = taxon.windows(7).any(|part| part == b"(taxid ");
        if !included && !named {
            return false;
        }
        if self.excluded_lca.is_none() {
            return included;
        }
        let Some(lca) = fields.next() else {
            return false;
        };
        !self.excludes(lca)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_taxa_and_paired_lengths() {
        let record = Classification::parse(b"C\tread\tBacteria (taxid 2)\t4:6\t2:1 |:| 2:3")
            .unwrap()
            .unwrap();
        assert_eq!(record.taxid.as_ref(), b"2");
        assert_eq!(
            ReadLengths::parse(&record.lengths).unwrap(),
            ReadLengths::Paired(4, 6)
        );
    }

    #[test]
    fn exclusion_does_not_bypass_taxon_inclusion() {
        let filter = ClassificationFilter::new(
            [Bytes::from_static(b"2")].into_iter().collect(),
            Some([Bytes::from_static(b"9")].into_iter().collect()),
        )
        .unwrap();
        let record = Classification::parse(b"C\tread\tOther (taxid 3)\t4\t3:1")
            .unwrap()
            .unwrap();
        assert!(!filter.matches(&record));
    }

    #[test]
    fn exclusion_preserves_legacy_suffix_matching() {
        let filter = ClassificationFilter::new(
            [Bytes::from_static(b"2")].into_iter().collect(),
            Some([Bytes::from_static(b"2")].into_iter().collect()),
        )
        .unwrap();
        let record = Classification::parse(b"C\tread\t2\t4\t12:1")
            .unwrap()
            .unwrap();
        assert!(!filter.matches(&record));
    }

    #[test]
    fn extraction_preserves_named_taxid_and_explicit_exclusion_behavior() {
        let selected = [Bytes::from_static(b"2")].into_iter().collect();
        let filter = ClassificationFilter::new(selected, Some(HashSet::default())).unwrap();
        assert!(filter.matches_line(b"C\tread\tOther (taxid 3)\t4\t3:1"));
        assert!(!filter.matches_line(b"C\tread\t3\t4\t3:1"));
    }

    #[test]
    fn rejects_malformed_fields_and_accepts_empty_input() {
        assert!(Classification::parse(b" \t ").unwrap().is_none());
        for line in [b"C\tread".as_slice(), b"C\tread\t\t4\t2:1"] {
            assert!(Classification::parse(line).is_err());
        }
    }
}

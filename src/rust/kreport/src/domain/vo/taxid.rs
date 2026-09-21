use std::fmt;

use thiserror::Error;

/// A taxonomic identifier in canonical decimal notation.
///
/// Kraken reports use `0` for unclassified reads. Other identifiers use decimal
/// digits without leading zeros, such as `2` or `562`; `02` and `0002` are rejected.
/// There is no numeric upper bound; for example, `4294967296` is also accepted.
/// Read a taxon's identifier with `taxon.taxid()`.
///
/// ```
/// use kreport::KrakenReportReader;
/// let input = b"100\t2\t2\tS\t562\tEscherichia coli\n";
/// let mut reader = KrakenReportReader::new(input.as_slice());
/// let entry = reader.read_entry()?.unwrap();
/// let taxid = entry.taxon().taxid();
/// assert_eq!(taxid.as_str(), "562");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Taxid(String);

impl Taxid {
    pub(crate) fn new(value: String) -> Result<Self, TaxidParseError> {
        if value.is_empty() {
            return Err(TaxidParseError::Empty);
        }
        if !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(TaxidParseError::InvalidFormat);
        }
        if value.len() > 1 && value.starts_with('0') {
            return Err(TaxidParseError::LeadingZero);
        }
        Ok(Self(value))
    }

    /// The identifier in canonical decimal notation, such as `562`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Taxid {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Taxid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

// Failures when constructing a taxid.
#[derive(Debug, Error)]
pub(crate) enum TaxidParseError {
    #[error("Empty taxid")]
    Empty,

    #[error("Invalid taxid, expected ASCII decimal digits without a sign or whitespace")]
    InvalidFormat,

    #[error("Invalid taxid, leading zeros are not accepted")]
    LeadingZero,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{Taxid, TaxidParseError};

    fn assert_rejected(value: &str) {
        assert!(Taxid::new(value.to_owned()).is_err(), "{value:?}");
    }

    #[test]
    fn accepts_canonical_decimal_taxids_including_zero() {
        for value in ["0", "1", "2", "562"] {
            let taxid = Taxid::new(value.to_owned()).unwrap();
            assert_eq!(taxid.as_str(), value);
            assert_eq!(taxid.as_ref(), value);
            assert_eq!(taxid.to_string(), value);
        }
    }

    #[test]
    fn taxid_equality_and_hashing_use_identifier_identity() {
        let taxid = Taxid::new("2".into()).unwrap();
        assert_ne!(taxid, Taxid::new("3".into()).unwrap());
        let ids: HashSet<_> = [taxid, Taxid::new("2".to_owned()).unwrap()]
            .into_iter()
            .collect();
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn rejects_empty_taxids() {
        assert_rejected("");
        assert!(matches!(
            Taxid::new(String::new()),
            Err(TaxidParseError::Empty)
        ));
    }

    #[test]
    fn rejects_leading_zeros() {
        for value in ["00", "01", "02", "0002", "0562"] {
            assert_rejected(value);
            assert!(matches!(
                Taxid::new(value.to_owned()),
                Err(TaxidParseError::LeadingZero)
            ));
        }
    }

    #[test]
    fn rejects_non_ascii_digits_signs_and_whitespace() {
        for value in [
            "taxon-A", "2a", "+2", "-2", " 2", "2 ", "2\n", "2.0", "1_000", "２", "٢",
        ] {
            assert_rejected(value);
            assert!(matches!(
                Taxid::new(value.to_owned()),
                Err(TaxidParseError::InvalidFormat)
            ));
        }
    }

    #[test]
    fn preserves_long_decimal_taxids() {
        for value in ["4294967295", "4294967296", "999999999999999999999999999"] {
            let taxid = Taxid::new(value.to_owned()).unwrap();
            assert_eq!(taxid.as_str(), value);
        }
    }
}

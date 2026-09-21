use std::borrow::Cow;
use std::fmt;

use thiserror::Error;

use super::vo::{Rank, Taxid, TaxidParseError, Taxon, TaxonLevel};

/// A condition on a taxon's identifier, rank, taxonomic level or scientific name.
///
/// A requested level with no intermediate level matches any depth at the
/// same major rank: both `G` and `G0` match taxa at `G`, `G1` and `G2`.
/// An intermediate level must match the major rank and depth: `G2` matches only
/// `G2`. A level-and-name condition also requires the same scientific name.
///
/// ```
/// use kreport::{Rank, TaxonSpec};
/// let spec = TaxonSpec::parse("Genus".into())?;
/// assert_eq!(spec, TaxonSpec::from(Rank::Genus));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct TaxonSpec(TaxonSpecInner);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum TaxonSpecInner {
    // Match a taxonomic identifier exactly.
    Taxid(Taxid),

    // Match a major rank, including its intermediate levels.
    TaxonRank(Rank),

    // Match a taxonomic level; no intermediate level permits any depth at that rank.
    TaxonLevel(TaxonLevel),

    // Match a scientific name exactly, regardless of taxonomic level.
    TaxonTerm(String),

    // Match both a taxonomic level and a scientific name.
    LevelAndTerm { level: TaxonLevel, term: String },
}

impl From<Taxid> for TaxonSpec {
    fn from(value: Taxid) -> Self {
        Self(TaxonSpecInner::Taxid(value))
    }
}

impl From<Rank> for TaxonSpec {
    fn from(value: Rank) -> Self {
        Self(TaxonSpecInner::TaxonRank(value))
    }
}

impl From<TaxonLevel> for TaxonSpec {
    fn from(value: TaxonLevel) -> Self {
        Self(TaxonSpecInner::TaxonLevel(value))
    }
}

impl TryFrom<&str> for TaxonSpec {
    type Error = TaxonSpecParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(Cow::Borrowed(value))
    }
}

impl TryFrom<String> for TaxonSpec {
    type Error = TaxonSpecParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(Cow::Owned(value))
    }
}

impl TaxonSpec {
    /// Parse a taxonomy condition, preferring identifiers and ranks over scientific names.
    ///
    /// - `562` selects a taxid.
    /// - `Genus` selects a major rank by its full, case-sensitive name.
    /// - `G` or `G2` selects a taxonomic level.
    /// - `G2__Genus group` selects a level and scientific name; names may contain `__`.
    /// - Other text, such as `Bacteria`, selects a scientific name.
    ///
    /// Unrecognized levels and incomplete level-and-name conditions are treated
    /// as complete names, for example `G256`, `X__name` and `G__`.
    ///
    /// # Errors
    /// Rejects empty input and digit-only taxids with leading
    /// zeros, such as `02`. Invalid digit-only taxids do not become scientific names.
    pub fn parse(value: Cow<'_, str>) -> Result<Self, TaxonSpecParseError> {
        if value.is_empty() {
            return Err(TaxonSpecParseError(TaxonSpecParseErrorKind::Empty));
        }

        if value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Ok(Taxid::new(value.into_owned())?.into());
        }

        if let Ok(rank) = Rank::parse(&value) {
            return Ok(rank.into());
        }

        if let Ok(level) = TaxonLevel::parse(&value) {
            return Ok(level.into());
        }

        if let Some((level, term)) = value.split_once("__").filter(|(_, term)| !term.is_empty()) {
            if let Ok(level) = TaxonLevel::parse(level) {
                return Ok(Self(TaxonSpecInner::LevelAndTerm {
                    level,
                    term: term.to_owned(),
                }));
            }
        }

        Ok(Self(TaxonSpecInner::TaxonTerm(value.into_owned())))
    }

    /// Whether the given taxon satisfies this specification.
    ///
    /// ```
    /// use kreport::{KrakenReportReader, TaxonSpec};
    ///
    /// let input = b"100\t2\t2\tG2\t10\tGenus group\n";
    /// let mut reader = KrakenReportReader::new(input.as_slice());
    /// let entry = reader.read_entry()?.unwrap();
    /// let spec = TaxonSpec::parse("Genus".into())?;
    /// assert!(spec.is_satisfied_by(entry.taxon()));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn is_satisfied_by(&self, taxon: &Taxon) -> bool {
        match &self.0 {
            TaxonSpecInner::Taxid(taxid) => taxon.taxid() == taxid,
            TaxonSpecInner::TaxonRank(rank) => taxon.rank() == rank,
            TaxonSpecInner::TaxonLevel(level) => {
                taxon.rank() == level.rank()
                    // Depth 0 represents no intermediate level.
                    && (level.depth() == 0 || taxon.depth() == level.depth())
            }
            TaxonSpecInner::TaxonTerm(term) => taxon.term() == term,
            TaxonSpecInner::LevelAndTerm { level, term } => {
                taxon.rank() == level.rank()
                    && (level.depth() == 0 || taxon.depth() == level.depth())
                    && taxon.term() == term
            }
        }
    }
}

/// A taxonomy condition could not be parsed.
#[derive(Error)]
#[error(transparent)]
pub struct TaxonSpecParseError(TaxonSpecParseErrorKind);

impl fmt::Debug for TaxonSpecParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

impl From<TaxidParseError> for TaxonSpecParseError {
    fn from(error: TaxidParseError) -> Self {
        Self(TaxonSpecParseErrorKind::InvalidTaxid(error))
    }
}

#[derive(Debug, Error)]
enum TaxonSpecParseErrorKind {
    #[error("Empty taxon specification.")]
    Empty,

    #[error(transparent)]
    InvalidTaxid(TaxidParseError),
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::super::vo::{Rank, Taxid, TaxidParseError};
    use super::{
        Taxon, TaxonLevel, TaxonSpec, TaxonSpecInner, TaxonSpecParseError, TaxonSpecParseErrorKind,
    };

    #[test]
    fn borrowed_and_owned_conditions_are_equivalent() {
        for text in ["562", "Genus", "G2", "Bacteria", "G2__Genus group"] {
            let borrowed = TaxonSpec::parse(Cow::Borrowed(text)).unwrap();
            let owned = TaxonSpec::parse(Cow::Owned(text.to_owned())).unwrap();
            assert_eq!(borrowed, owned, "{text}");
            let converted_borrowed: TaxonSpec = text.try_into().unwrap();
            let converted_owned: TaxonSpec = text.to_owned().try_into().unwrap();
            assert_eq!(converted_borrowed, borrowed, "{text}");
            assert_eq!(converted_owned, owned, "{text}");
        }
    }

    #[test]
    fn parses_unrecognized_text_as_a_taxon_term() {
        for term in [
            "Bacteria",
            "Genus group",
            "genus",
            "G256",
            "G02",
            "２",
            "+2",
            " 2",
        ] {
            assert_eq!(
                TaxonSpec::parse(term.into()).unwrap().0,
                TaxonSpecInner::TaxonTerm(term.to_owned()),
                "{term:?}"
            );
        }
    }

    #[test]
    fn parses_full_rank_names_as_rank_specs() {
        for (name, rank) in [
            ("Unclassified", Rank::Unclassified),
            ("Root", Rank::Root),
            ("Domain", Rank::Domain),
            ("Kingdom", Rank::Kingdom),
            ("Phylum", Rank::Phylum),
            ("Class", Rank::Class),
            ("Order", Rank::Order),
            ("Family", Rank::Family),
            ("Genus", Rank::Genus),
            ("Species", Rank::Species),
        ] {
            assert_eq!(
                TaxonSpec::parse(name.into()).unwrap().0,
                TaxonSpecInner::TaxonRank(rank),
                "{name}"
            );
        }
    }

    #[test]
    fn parses_rank_abbreviations_and_depths_as_level_specs() {
        for (text, rank, depth) in [
            ("U", Rank::Unclassified, 0),
            ("R", Rank::Root, 0),
            ("D", Rank::Domain, 0),
            ("K", Rank::Kingdom, 0),
            ("P", Rank::Phylum, 0),
            ("C", Rank::Class, 0),
            ("O", Rank::Order, 0),
            ("F", Rank::Family, 0),
            ("G", Rank::Genus, 0),
            ("S", Rank::Species, 0),
            ("G0", Rank::Genus, 0),
            ("G2", Rank::Genus, 2),
            ("G255", Rank::Genus, 255),
        ] {
            let spec = TaxonSpec::parse(text.into()).unwrap();
            let TaxonSpecInner::TaxonLevel(level) = spec.0 else {
                panic!("expected a taxonomic level for {text}");
            };
            assert_eq!(level.rank(), &rank, "{text}");
            assert_eq!(level.depth(), depth, "{text}");
        }
    }

    #[test]
    fn parses_taxid_spec() {
        assert_eq!(
            TaxonSpec::parse("562".into()).unwrap().0,
            TaxonSpecInner::Taxid(Taxid::new("562".to_owned()).unwrap())
        );
    }

    #[test]
    fn rejects_taxid_specs_with_leading_zeros() {
        for key in ["00", "02", "0002"] {
            let error = TaxonSpec::parse(key.into()).unwrap_err();
            assert!(matches!(
                error,
                TaxonSpecParseError(TaxonSpecParseErrorKind::InvalidTaxid(
                    TaxidParseError::LeadingZero
                ))
            ));
            assert_eq!(
                error.to_string(),
                "Invalid taxid, leading zeros are not accepted"
            );
        }
    }

    #[test]
    fn preserves_long_decimal_taxid_specs() {
        for value in ["4294967296", "999999999999999999999999999"] {
            assert_eq!(
                TaxonSpec::parse(value.into()).unwrap().0,
                TaxonSpecInner::Taxid(Taxid::new(value.to_owned()).unwrap())
            );
        }
    }

    #[test]
    fn unrecognized_level_and_term_conditions_fall_back_to_complete_terms() {
        for term in [
            "__Bacteria",
            "D__",
            "__",
            "X__name",
            "G256__Group",
            "G02__Group",
            "Genus__group",
        ] {
            assert_eq!(
                TaxonSpec::parse(term.into()).unwrap().0,
                TaxonSpecInner::TaxonTerm(term.to_owned()),
                "{term}"
            );
        }
    }

    #[test]
    fn taxa_match_by_taxid_or_ranked_name() {
        let taxon = Taxon::new(
            TaxonLevel::parse("D").unwrap(),
            Taxid::new("2".into()).unwrap(),
            "Bacteria".to_owned(),
        );

        for (label, expected) in [
            ("2", true),
            ("D__Bacteria", true),
            ("S__Bacteria", false),
            ("D__Archaea", false),
            ("3", false),
        ] {
            let key = TaxonSpec::parse(label.into()).unwrap();
            assert_eq!(key.is_satisfied_by(&taxon), expected, "{label}");
        }
    }

    #[test]
    fn parses_level_and_term_spec() {
        assert_eq!(
            TaxonSpec::parse("D__Bacteria".into()).unwrap().0,
            TaxonSpecInner::LevelAndTerm {
                level: TaxonLevel::parse("D").unwrap(),
                term: "Bacteria".to_owned(),
            }
        );
    }

    #[test]
    fn rejects_empty_taxon_spec() {
        let error =
            TaxonSpec::parse(String::new().into()).expect_err("expected empty taxonomy to fail");

        assert_eq!(error.to_string(), "Empty taxon specification.");
    }

    #[test]
    fn level_and_term_specs_with_an_intermediate_level_require_the_same_level_and_term() {
        let key = TaxonSpec::parse("G1__Genus__group".into()).unwrap();
        for (rank, name, expected) in [
            ("G1", "Genus__group", true),
            ("G", "Genus__group", false),
            ("G2", "Genus__group", false),
            ("G1", "Genus", false),
        ] {
            let taxon = Taxon::new(
                TaxonLevel::parse(rank).unwrap(),
                Taxid::new("10".into()).unwrap(),
                name.to_owned(),
            );
            assert_eq!(key.is_satisfied_by(&taxon), expected, "{rank}__{name}");
        }
    }

    fn taxon(level: &str, term: &str) -> Taxon {
        Taxon::new(
            TaxonLevel::parse(level).unwrap(),
            Taxid::new("10".into()).unwrap(),
            term.into(),
        )
    }

    #[test]
    fn rank_specs_match_major_ranks_regardless_of_depth() {
        let spec = TaxonSpec::parse("Genus".into()).unwrap();
        for (level, expected) in [("G", true), ("G0", true), ("G2", true), ("S2", false)] {
            assert_eq!(
                spec.is_satisfied_by(&taxon(level, "Genus group")),
                expected,
                "{level}"
            );
        }
    }

    #[test]
    fn level_specs_without_an_intermediate_level_match_all_depths() {
        for requested in ["G", "G0"] {
            let spec = TaxonSpec::parse(requested.into()).unwrap();
            for (level, expected) in [("G", true), ("G2", true), ("G255", true), ("S", false)] {
                assert_eq!(
                    spec.is_satisfied_by(&taxon(level, "Genus group")),
                    expected,
                    "{requested}: {level}"
                );
            }
        }
    }

    #[test]
    fn level_specs_with_an_intermediate_level_require_the_same_rank_and_depth() {
        let spec = TaxonSpec::parse("G2".into()).unwrap();
        for (level, expected) in [("G", false), ("G1", false), ("G2", true), ("S2", false)] {
            assert_eq!(
                spec.is_satisfied_by(&taxon(level, "Genus group")),
                expected,
                "{level}"
            );
        }
    }

    #[test]
    fn term_specs_match_scientific_names_regardless_of_level() {
        let spec = TaxonSpec::parse("Genus group".into()).unwrap();
        for level in ["G", "G2", "S"] {
            assert!(spec.is_satisfied_by(&taxon(level, "Genus group")));
            assert!(!spec.is_satisfied_by(&taxon(level, "Other group")));
        }
    }

    #[test]
    fn level_and_term_specs_without_an_intermediate_level_match_all_depths() {
        for label in ["G__Genus group", "G0__Genus group"] {
            let spec = TaxonSpec::parse(label.into()).unwrap();
            for (level, term, expected) in [
                ("G", "Genus group", true),
                ("G2", "Genus group", true),
                ("S", "Genus group", false),
                ("G2", "Other group", false),
            ] {
                assert_eq!(
                    spec.is_satisfied_by(&taxon(level, term)),
                    expected,
                    "{label}: {level}__{term}"
                );
            }
        }
    }
}

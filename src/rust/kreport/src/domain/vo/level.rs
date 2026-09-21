use std::error::Error as StdError;
use std::fmt;
use std::num::ParseIntError;

use thiserror::Error;

/// Major taxonomic ranks, including the unclassified and root markers.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rank {
    /// Reads without a taxonomic classification.
    Unclassified,
    /// The root of the taxonomic hierarchy.
    Root,
    /// The domain rank.
    Domain,
    /// The kingdom rank.
    Kingdom,
    /// The phylum rank.
    Phylum,
    /// The class rank.
    Class,
    /// The order rank.
    Order,
    /// The family rank.
    Family,
    /// The genus rank.
    Genus,
    /// The species rank.
    Species,
}

impl Rank {
    // Parse a full, case-sensitive major-rank name, such as "Genus".
    pub(in crate::domain) fn parse(rank: &str) -> Result<Self, RankParseError> {
        let rank = match rank {
            "Unclassified" => Rank::Unclassified,
            "Root" => Rank::Root,
            "Domain" => Rank::Domain,
            "Kingdom" => Rank::Kingdom,
            "Phylum" => Rank::Phylum,
            "Class" => Rank::Class,
            "Order" => Rank::Order,
            "Family" => Rank::Family,
            "Genus" => Rank::Genus,
            "Species" => Rank::Species,
            _ => return Err(RankParseError),
        };
        Ok(rank)
    }

    /// The single-letter report representation.
    ///
    /// Report abbreviations are `U` (unclassified), `R` (root), `D` (domain),
    /// `K` (kingdom), `P` (phylum), `C` (class), `O` (order), `F` (family),
    /// `G` (genus) and `S` (species).
    pub fn abbre(&self) -> &'static str {
        match self {
            Self::Unclassified => "U",
            Self::Root => "R",
            Self::Domain => "D",
            Self::Kingdom => "K",
            Self::Phylum => "P",
            Self::Class => "C",
            Self::Order => "O",
            Self::Family => "F",
            Self::Genus => "G",
            Self::Species => "S",
        }
    }
}

// Failure to parse a taxonomic rank.
#[derive(Debug, Error)]
#[error("Invalid taxonomic rank name, expected Unclassified, Root, Domain, Kingdom, Phylum, Class, Order, Family, Genus or Species")]
pub(in crate::domain) struct RankParseError;

/// A taxon's level in the taxonomic hierarchy, at a major or intermediate rank.
///
/// Reports represent major ranks with a [`Rank`] abbreviation, such as `G` for genus.
/// Intermediate ranks append a depth indicating the distance from the nearest
/// ancestor at a major rank. For example, `G2` denotes a taxon two levels below
/// its genus ancestor.
///
/// A depth of zero denotes no intermediate level, so `G0` is equivalent to `G`.
/// Leading zeros such as `G00`, `G01` and `G02` are not accepted.
/// Read a taxon's level with `taxon.level()`.
///
/// ```
/// use kreport::KrakenReportReader;
/// let input = b"100\t2\t2\tG2\t10\tGenus group\n";
/// let mut reader = KrakenReportReader::new(input.as_slice());
/// let entry = reader.read_entry().unwrap()?;
/// let taxon_level = entry.taxon().level();
/// assert_eq!(taxon_level.rank().abbre(), "G");
/// assert_eq!(taxon_level.depth(), 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaxonLevel {
    rank: Rank,
    depth: u8,
}

impl TaxonLevel {
    fn new(rank: Rank) -> Self {
        Self { rank, depth: 0 }
    }

    fn with_depth(depth: u8, rank: Rank) -> Self {
        Self { rank, depth }
    }

    pub(crate) fn parse(level: &str) -> Result<Self, TaxonLevelParseError> {
        let mut chars = level.chars();
        let rank = match chars.next().ok_or(TaxonLevelParseError::Empty)? {
            'U' => Rank::Unclassified,
            'R' => Rank::Root,
            'D' => Rank::Domain,
            'K' => Rank::Kingdom,
            'P' => Rank::Phylum,
            'C' => Rank::Class,
            'O' => Rank::Order,
            'F' => Rank::Family,
            'G' => Rank::Genus,
            'S' => Rank::Species,
            _ => return Err(TaxonLevelParseError::InvalidRank),
        };
        let depth = chars.as_str();
        if depth.is_empty() {
            return Ok(Self::new(rank));
        }
        // Integer parsing accepts a leading '+' and leading zeros; rank depths do not.
        if depth.starts_with('+') || (depth.len() > 1 && depth.starts_with('0')) {
            return Err(TaxonLevelParseError::InvalidDepthFormat);
        }
        Ok(Self::with_depth(depth.parse()?, rank))
    }

    /// The major rank of the taxon or its nearest ancestor at a major rank.
    /// For example, both `G` and `G2` refer to the major rank genus.
    pub fn rank(&self) -> &Rank {
        &self.rank
    }

    /// The depth below the nearest ancestor at a major rank, measured in taxonomic levels.
    /// For example, `G2` has depth 2 below its genus ancestor.
    /// Zero denotes no intermediate level.
    pub fn depth(&self) -> u8 {
        self.depth
    }

    /// Whether the major rank is unclassified, with or without an intermediate level.
    /// For example, this includes both `U` and `U1`.
    pub fn is_unclassified(&self) -> bool {
        self.rank == Rank::Unclassified
    }

    /// Whether the major rank is root, with or without an intermediate level.
    /// For example, this includes both `R` and `R1`.
    pub fn is_root(&self) -> bool {
        self.rank == Rank::Root
    }
}

impl fmt::Display for TaxonLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.rank.abbre())?;
        if self.depth != 0 {
            write!(formatter, "{}", self.depth)?;
        }
        Ok(())
    }
}

// Failures when parsing a taxonomic level.
#[derive(Error)]
pub(crate) enum TaxonLevelParseError {
    #[error("Missing taxonomic rank")]
    Empty,

    #[error("Invalid taxonomic rank, expected U, R, D, K, P, C, O, F, G or S")]
    InvalidRank,

    #[error("Invalid rank depth, expected decimal digits without a sign or leading zeros")]
    InvalidDepthFormat,

    #[error("Invalid rank depth, expected decimal digits from 0 to 255")]
    InvalidDepthInt {
        #[from]
        source: ParseIntError,
    },
}

impl fmt::Debug for TaxonLevelParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Alternate debug keeps the structured representation; default debug
        // displays the message and source under "Caused by:".
        if formatter.alternate() {
            return match self {
                Self::Empty => formatter.write_str("Empty"),
                Self::InvalidRank => formatter.write_str("InvalidRank"),
                Self::InvalidDepthFormat => formatter.write_str("InvalidDepthFormat"),
                Self::InvalidDepthInt { source } => formatter
                    .debug_struct("InvalidDepthInt")
                    .field("source", source)
                    .finish(),
            };
        }

        fmt::Display::fmt(self, formatter)?;
        if let Some(source) = self.source() {
            write!(formatter, "\n\nCaused by:\n    {source}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::error::Error as _;
    use std::num::{IntErrorKind, ParseIntError};

    use super::{Rank, TaxonLevel, TaxonLevelParseError};

    #[test]
    fn rank_parsing_accepts_full_names() {
        for (name, expected) in [
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
            assert_eq!(Rank::parse(name).unwrap(), expected, "{name}");
        }
    }

    #[test]
    fn rank_parse_error_reports_expected_full_names() {
        for name in ["", "G", "G2", "genus", "Unknown", " Genus", "Genus "] {
            let error = Rank::parse(name).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Invalid taxonomic rank name, expected Unclassified, Root, Domain, Kingdom, Phylum, Class, Order, Family, Genus or Species",
                "{name:?}"
            );
        }
    }

    #[test]
    fn taxon_level_parse_error_reports_expected_abbreviations() {
        let error = TaxonLevel::parse("X").unwrap_err();
        assert_eq!(
            error.to_string(),
            "Invalid taxonomic rank, expected U, R, D, K, P, C, O, F, G or S"
        );
    }

    #[test]
    fn zero_depth_means_no_intermediate_level() {
        let rank = TaxonLevel::with_depth(0, Rank::Genus);
        assert_eq!(rank.depth(), 0);
        assert_eq!(rank, TaxonLevel::new(Rank::Genus));
    }

    #[test]
    fn taxon_level_parsing_preserves_rank_and_depth() {
        let rank = TaxonLevel::with_depth(2, Rank::Genus);
        assert_eq!(rank, TaxonLevel::parse("G2").unwrap());
        assert_eq!(rank.rank(), &Rank::Genus);
        assert_eq!(rank.depth(), 2);
        assert_eq!(TaxonLevel::new(Rank::Genus).depth(), 0);
    }

    #[test]
    fn root_check_accepts_intermediate_depths() {
        assert!(TaxonLevel::parse("R1").unwrap().is_root());
    }

    #[test]
    fn unclassified_check_accepts_intermediate_depths() {
        assert!(TaxonLevel::parse("U1").unwrap().is_unclassified());
    }

    #[test]
    fn taxon_level_values_round_trip_rank_abbreviations_and_depths() {
        for (rank, abbreviation) in [
            (Rank::Unclassified, "U"),
            (Rank::Root, "R"),
            (Rank::Domain, "D"),
            (Rank::Kingdom, "K"),
            (Rank::Phylum, "P"),
            (Rank::Class, "C"),
            (Rank::Order, "O"),
            (Rank::Family, "F"),
            (Rank::Genus, "G"),
            (Rank::Species, "S"),
        ] {
            let taxon_level = TaxonLevel::new(rank.clone());
            assert_eq!(TaxonLevel::parse(abbreviation).unwrap(), taxon_level);
            assert_eq!(taxon_level.to_string(), abbreviation);
            assert_eq!(taxon_level.depth(), 0);
            for depth in 1..=255 {
                let taxon_level = TaxonLevel::with_depth(depth, rank.clone());
                let text = format!("{abbreviation}{depth}");
                assert_eq!(TaxonLevel::parse(&text).unwrap(), taxon_level);
                assert_eq!(taxon_level.depth(), depth);
                assert_eq!(taxon_level.to_string(), text);
            }
        }
    }

    #[test]
    fn taxon_level_parsing_treats_a_single_zero_as_no_intermediate_level() {
        for abbreviation in ["U", "R", "D", "K", "P", "C", "O", "F", "G", "S"] {
            let text = format!("{abbreviation}0");
            let rank = TaxonLevel::parse(&text).unwrap();
            assert_eq!(rank, TaxonLevel::parse(abbreviation).unwrap(), "{text}");
            assert_eq!(rank.depth(), 0, "{text}");
            assert_eq!(rank.to_string(), abbreviation);
        }
    }

    #[test]
    fn taxon_level_parsing_rejects_leading_zeros() {
        for abbreviation in ["U", "R", "D", "K", "P", "C", "O", "F", "G", "S"] {
            for depth in ["00", "01", "02", "002", "0255"] {
                let text = format!("{abbreviation}{depth}");
                assert!(TaxonLevel::parse(&text).is_err(), "{text}");
            }
        }
    }

    #[test]
    fn equivalent_taxon_level_values_have_the_same_hash_identity() {
        let unique: HashSet<_> = [
            TaxonLevel::with_depth(2, Rank::Genus),
            TaxonLevel::parse("G2").unwrap(),
        ]
        .into_iter()
        .collect();
        assert_eq!(unique.len(), 1);
    }

    #[test]
    fn taxon_level_values_sort_by_rank_then_depth() {
        let mut ranks = ["S", "G10", "G2", "G", "D", "R", "U", "G1"]
            .into_iter()
            .map(|rank| TaxonLevel::parse(rank).unwrap())
            .collect::<Vec<_>>();
        ranks.sort();
        assert_eq!(
            ranks.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["U", "R", "D", "G", "G1", "G2", "G10", "S"]
        );
    }

    #[test]
    fn invalid_depths_preserve_the_integer_error() {
        for (rank, expected) in [
            ("G256", IntErrorKind::PosOverflow),
            ("Gx", IntErrorKind::InvalidDigit),
        ] {
            let error = TaxonLevel::parse(rank).unwrap_err();
            let TaxonLevelParseError::InvalidDepthInt { source } = &error else {
                panic!("expected an invalid depth error for {rank}");
            };
            assert_eq!(source.kind(), &expected);
            assert!(error.source().unwrap().is::<ParseIntError>());
        }
    }

    #[test]
    fn invalid_depth_display_reports_the_expected_range() {
        for rank in ["G256", "Gx"] {
            let error = TaxonLevel::parse(rank).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Invalid rank depth, expected decimal digits from 0 to 255"
            );
        }
    }

    #[test]
    fn taxon_level_parse_error_debug_includes_the_cause() {
        for rank in ["G256", "Gx"] {
            let error = TaxonLevel::parse(rank).unwrap_err();
            let source = error.source().unwrap();
            assert_eq!(
                format!("{error:?}"),
                format!("{error}\n\nCaused by:\n    {source}")
            );
        }
    }

    #[test]
    fn taxon_level_parse_error_alternate_debug_preserves_the_structure() {
        for rank in ["G256", "Gx"] {
            let error = TaxonLevel::parse(rank).unwrap_err();
            let structured = format!("{error:#?}");
            assert!(structured.starts_with("InvalidDepthInt {\n"));
            assert!(structured.contains("source: ParseIntError {"));
            assert!(!structured.contains("Caused by:"));
        }
    }

    #[test]
    fn taxon_level_parse_errors_without_causes_have_no_source() {
        for error in [
            TaxonLevelParseError::Empty,
            TaxonLevelParseError::InvalidRank,
            TaxonLevelParseError::InvalidDepthFormat,
        ] {
            assert!(error.source().is_none());
        }
    }

    #[test]
    fn taxon_level_parse_error_debug_without_a_cause_matches_display() {
        for error in [
            TaxonLevelParseError::Empty,
            TaxonLevelParseError::InvalidRank,
            TaxonLevelParseError::InvalidDepthFormat,
        ] {
            assert_eq!(format!("{error:?}"), error.to_string());
        }
    }
}

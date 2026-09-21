use super::{Rank, Taxid, TaxonLevel};

/// A taxonomic unit with a taxonomic level, taxid, and scientific name.
/// Obtain taxa from report entries and their ancestor lineages.
///
/// ```
/// use kreport::KrakenReportReader;
/// let input = b"100\t2\t2\tD\t2\tBacteria\n";
/// let mut reader = KrakenReportReader::new(input.as_slice());
/// let entry = reader.read_entry()?.unwrap();
/// assert_eq!(entry.taxon().rank().abbre(), "D");
/// assert_eq!(entry.taxon().taxid().as_str(), "2");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Taxon {
    level: TaxonLevel,
    taxid: Taxid,
    term: String,
}

impl Taxon {
    // Construct a taxon from its taxonomic level, ID and scientific name.
    pub(crate) fn new(level: TaxonLevel, taxid: Taxid, term: String) -> Self {
        Self { level, taxid, term }
    }

    /// The taxonomic level, including its major rank and intermediate depth.
    pub fn level(&self) -> &TaxonLevel {
        &self.level
    }

    /// The major rank of the taxon or its nearest ancestor at a major rank.
    /// For example, taxa at both `G` and `G2` have the major rank genus.
    pub fn rank(&self) -> &Rank {
        self.level.rank()
    }

    /// The distance below the nearest ancestor at a major rank.
    /// For example, a taxon at `G2` is two levels below its genus ancestor.
    /// Zero denotes no intermediate level, such as genus itself (`G`).
    pub fn depth(&self) -> u8 {
        self.level.depth()
    }

    /// Taxonomic ID as supplied by the report.
    pub fn taxid(&self) -> &Taxid {
        &self.taxid
    }

    /// Scientific name without indentation.
    pub fn term(&self) -> &str {
        &self.term
    }

    /// Whether the major rank is unclassified, with or without an intermediate level.
    /// For example, this includes taxa at `U` and `U1`.
    pub fn is_unclassified(&self) -> bool {
        self.level.is_unclassified()
    }

    /// Whether the major rank is root, with or without an intermediate level.
    /// For example, this includes taxa at `R` and `R1`.
    pub fn is_root(&self) -> bool {
        self.level.is_root()
    }
}

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

    /// Whether this taxon is the root of the taxonomic hierarchy.
    /// The root is represented by `R` or its equivalent `R0`.
    ///
    /// See [`Self::is_root_rank`] to also match taxa at intermediate levels such as `R1` and `R2`.
    ///
    /// ```
    /// use kreport::KrakenReportReader;
    /// let input = b"100\t2\t0\tR\t1\troot\n100\t2\t2\tR1\t131567\t  cellular organisms\n";
    /// let mut reader = KrakenReportReader::new(input.as_slice());
    /// let root = reader.read_entry()?.unwrap();
    /// assert!(root.taxon().is_root());
    /// let cellular = reader.read_entry()?.unwrap();
    /// assert!(!cellular.taxon().is_root());
    /// # Ok::<(), kreport::Error>(())
    /// ```
    pub fn is_root(&self) -> bool {
        self.level.is_root()
    }

    /// Whether the major rank is root, with or without an intermediate level.
    /// For example, this includes taxa at `R`, `R0`, `R1` and `R2`.
    ///
    /// See [`Self::is_root`] to match only the root itself (`R` or `R0`).
    ///
    /// ```
    /// use kreport::KrakenReportReader;
    /// let input = b"100\t2\t0\tR1\t131567\tcellular organisms\n100\t2\t2\tD\t2\t  Bacteria\n";
    /// let mut reader = KrakenReportReader::new(input.as_slice());
    /// let cellular = reader.read_entry()?.unwrap();
    /// assert!(cellular.taxon().is_root_rank());
    /// let bacteria = reader.read_entry()?.unwrap();
    /// assert!(!bacteria.taxon().is_root_rank());
    /// # Ok::<(), kreport::Error>(())
    /// ```
    pub fn is_root_rank(&self) -> bool {
        self.level.is_root_rank()
    }
}

use std::{num::ParseFloatError, num::ParseIntError, str::Utf8Error};

use thiserror::Error;

/// Failures produced while decoding and assembling a Kraken2 report.
#[derive(Debug, Error)]
pub enum KrakenReportError {
    #[error("Invalid line with {actual} fields; expected 6 or 8")]
    InvalidFieldCount { actual: usize },

    #[error("Missing rank code")]
    MissingRank,

    #[error("Missing taxid")]
    MissingTaxid,

    #[error("Missing taxon name")]
    MissingTaxonName,

    #[error("Invalid taxon indentation; expected two spaces per taxonomic level")]
    InvalidTaxonIndentation,

    #[error("Invalid UTF-8 in {field}")]
    InvalidUtf8 {
        field: &'static str,
        #[source]
        source: Utf8Error,
    },

    #[error("Invalid floating-point value '{value}' in {field}")]
    InvalidFloat {
        field: &'static str,
        value: String,
        #[source]
        source: ParseFloatError,
    },

    #[error("Invalid integer value '{value}' in {field}")]
    InvalidInteger {
        field: &'static str,
        value: String,
        #[source]
        source: ParseIntError,
    },

    #[error("Invalid taxonomic ancestry state")]
    InvalidAncestryState,
}

#[derive(Debug, Error)]
pub enum TaxonomyError {
    #[error("Taxonomy must not be empty.")]
    Empty,
    #[error("Invalid taxonomy '{0}'. Expected a taxid or 'rank__name', for example '562' or 'D__Bacteria'.")]
    InvalidFormat(String),
    #[error("Invalid taxonomy '{0}'. Both rank and name must be non-empty.")]
    EmptyPart(String),
    #[error("At least one taxonomy must be provided.")]
    EmptySelection,
    #[error("No taxonomic matches found in the kreport file for {0:?}.")]
    NoMatches(Vec<String>),
}

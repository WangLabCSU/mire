//! R input conversion for sequence ranges and tags.
use anyhow::{anyhow, Result};
use bytes::Bytes;
use extendr_api::prelude::*;
use rustc_hash::FxHashMap as HashMap;

use mire_sequence::{SeqRange, SeqRanges, TagRanges};

/// Extract sequence ranges from an R object.
/// The R object must inherit from either `mire_seq_range` or `mire_seq_ranges`.
/// Returns an error if the object is not structured correctly or if the ranges are malformed.
pub(crate) fn parse_ranges(value: &Robj) -> Result<SeqRanges> {
    if value.inherits("mire_seq_range") {
        // Only one single range
        return Ok(std::iter::once(parse_range(value)?).collect());
    }

    if !value.inherits("mire_seq_ranges") {
        return Err(anyhow!(
            "The object does not inherit a valid range class (expected one of: 'mire_seq_range', or 'mire_seq_ranges')."
        ));
    }

    let list = value
        .as_list()
        .ok_or(anyhow!("Failed to extract valid sequence ranges."))?;

    list.values()
        .enumerate()
        .map(|(index, element)| {
            parse_range(&element).map_err(|error| anyhow!("Element {}: {}", index, error))
        })
        .collect()
}

pub(crate) fn parse_range(value: &Robj) -> Result<SeqRange> {
    // Each element should itself be a list of 2 elements
    let pair = value
        .as_list()
        .ok_or_else(|| anyhow!("Robj is not a 2-element list."))?;

    if pair.len() != 2 {
        return Err(anyhow!("Robj must contain exactly 2 values (start, end)."));
    }

    let start = pair
        .elt(0)
        .map_err(|error| anyhow!("Failed to read start: {error}"))?;
    let end = pair
        .elt(1)
        .map_err(|error| anyhow!("Failed to read end: {error}"))?;

    let start_usize = if start.is_null() {
        None
    } else {
        let s = start
            .as_integer_slice()
            .ok_or_else(|| anyhow!("start must be an integer."))?;
        if s.len() != 1 || s[0] <= 0 {
            return Err(anyhow!("start must be a single positive integer."));
        }
        Some((s[0] as usize) - 1) // 0-based
    };

    let end_usize = if end.is_null() {
        None
    } else {
        let e = end
            .as_integer_slice()
            .ok_or_else(|| anyhow!("end must be an integer."))?;
        if e.len() != 1 || e[0] <= 0 {
            return Err(anyhow!("end must be a single positive integer."));
        }
        Some(e[0] as usize) // inclusive R end becomes the exclusive zero-based end
    };
    Ok(SeqRange::build(start_usize, end_usize)?)
}

/// Extracts a tag name from an R object (Robj) used in action annotation.
/// This is used in R interface bindings for embedding.
///
/// # Errors
/// Returns an error if the `"tag"` attribute is missing or not a string.
pub(crate) fn extract_tag_name(robj: &Robj) -> Result<Bytes> {
    robj.get_attrib("tag")
        .and_then(|t| t.as_str())
        .ok_or(anyhow!("'tag' attribute must be provided"))
        .map(|t| Bytes::copy_from_slice(t.as_bytes()))
}

// Create object from R
pub(crate) fn robj_to_tag_ranges(ranges: &Robj) -> Result<Option<TagRanges>> {
    if ranges.is_null() {
        return Ok(None);
    }
    Ok(Some(parse_tags(ranges)?))
}

pub(crate) fn parse_tags(value: &Robj) -> Result<TagRanges> {
    let list = value
        .as_list()
        .ok_or(anyhow!("Expected a list of sequence range objects."))?;
    let tag_ranges = list
        .values()
        .map(|robj| -> Result<(Bytes, SeqRanges)> {
            if !robj.inherits("mire_tag") {
                return Err(anyhow!(
                    "The object does not inherit a valid tag class (expected 'mire_tag')."
                ));
            }
            let tag = extract_tag_name(&robj)?;
            let ranges = parse_ranges(&robj)?;
            // Validate before collecting so an invalid duplicate tag cannot
            // be hidden by a later entry with the same name.
            ranges.validate_extraction()?;
            Ok((tag, ranges))
        })
        .collect::<Result<HashMap<Bytes, SeqRanges>>>()?;
    Ok(TagRanges::new(tag_ranges)?)
}

//! R input conversion for sequence refinement actions.
use anyhow::{anyhow, Result};
use extendr_api::prelude::*;

use crate::sequence::{extract_tag_name, parse_ranges};
use mire_sequence::{SeqAction, SubseqActions};

// Create object from R
pub(super) fn robj_to_seq_actions(ranges: &Robj) -> Result<Option<SubseqActions>> {
    if ranges.is_null() {
        return Ok(None);
    }
    Ok(Some(parse_actions(ranges)?))
}

pub(crate) fn parse_actions(value: &Robj) -> Result<SubseqActions> {
    let mut out = SubseqActions::builder();
    for ref robj in value
        .as_list()
        .ok_or(anyhow!("Expected a list of sequence range objects."))?
        .values()
    {
        let ranges = parse_ranges(robj)?;
        let action = parse_action(robj)?;
        out.add_action(action, ranges)?;
    }
    Ok(out.build()?)
}

pub(crate) fn parse_action(value: &Robj) -> Result<SeqAction> {
    let resolved_action = if value.inherits("mire_trim") {
        SeqAction::Trim
    } else if value.inherits("mire_embed") {
        SeqAction::Embed(extract_tag_name(value)?)
    } else if value.inherits("mire_embed_trim") {
        SeqAction::EmbedTrim(extract_tag_name(value)?)
    } else {
        return Err(anyhow!(
                "The object does not inherit a valid action class (expected one of: 'mire_trim', 'mire_embed', or 'mire_embed_trim')."
            ));
    };
    Ok(resolved_action)
}

//! Public R signatures are compatibility contracts and intentionally retain their argument lists.
#![allow(clippy::too_many_arguments)]
mod actions;
pub(crate) mod koutput_reads;
pub(crate) mod kractor;
pub(crate) mod krcount;
pub(crate) mod kreport;
#[cfg(feature = "bench")]
mod profile;
pub(crate) mod seq_refine;
mod sequence;
mod values;

use extendr_api::prelude::*;
extendr_module! { mod mire; use kreport; use seq_refine; use koutput_reads; use krcount; use kractor; }

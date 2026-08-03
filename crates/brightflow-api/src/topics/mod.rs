//! Topic modelling and intent classification over enriched text tables.
//!
//! Covers the taxonomy (the category set), the labels applied to rows, the
//! overlay that maps clusters onto categories, and the handlers that expose all
//! of it. Fitting is always explicit rather than automatic — it re-labels every
//! existing row, so it is a decision, not a side effect of syncing.

// Topics handlers do numeric scoring + clustering math, where casting between
// integer and float types is part of the algorithm. The engine crate makes
// the same allowances; mirror them here.
#![allow(
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::shadow_unrelated
)]

pub(crate) mod display;
pub mod handlers;
pub mod labels;
pub mod overlay;
pub mod taxonomy;
pub mod types;

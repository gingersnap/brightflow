// Topics handlers do numeric scoring + clustering math, where casting between
// integer and float types is part of the algorithm. The engine crate makes
// the same allowances; mirror them here.
#![allow(
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::shadow_unrelated
)]

mod display;
pub mod handlers;
pub mod overlay;
pub mod types;

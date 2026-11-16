#![cfg_attr(not(test), no_std)]

#[cfg(any(test, feature = "build"))]
extern crate alloc;

#[cfg(any(test, feature = "build"))]
pub mod build;
mod font;
mod style;
mod table;

pub use font::{Font, FontSet, Glyph};
pub use style::TextStyle;
pub use table::{Mapping, RANGE_ENTRY_SIZE, RANK_TABLE_SIZE, RangeTable, RankTable};

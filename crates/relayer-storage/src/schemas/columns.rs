//! Constants which define low-level database column families.

/// Column families alias type
pub type Column = &'static str;

/// Total column number
pub const COUNT: usize = 1;

/// Column to store MMR for beacon headers
pub const COLUMN_BEACON_HEADER_MMR: Column = "beacon-header-mmr";

//! Keys for special values.

/// Tracks the current database version.
pub const MIGRATION_VERSION_KEY: &[u8] = b"db-version";

/// The first tip beacon header for MMR.
pub const BASE_BEACON_HEADER_SLOT: &[u8] = b"base-beacon-header-slot";
/// The current tip beacon header.
pub const TIP_BEACON_HEADER_SLOT: &[u8] = b"tip-beacon-header-slot";

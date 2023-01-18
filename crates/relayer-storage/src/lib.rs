pub mod error;
pub mod prelude;
pub mod schemas;

pub type Slot = u64;

mod storage;
pub use storage::Storage;

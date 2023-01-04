use std::{fmt, result};

use eth_light_client_in_ckb_verification::{mmr, molecule};

use thiserror::Error;

pub type Result<T> = result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("low-level db error: {0}")]
    DB(#[from] rocksdb::Error),

    #[error("mmr error: {0}")]
    MMR(#[from] mmr::lib::Error),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("data error: {0}")]
    Data(String),
}

impl Error {
    pub fn storage<T: fmt::Display>(inner: T) -> Self {
        Self::Storage(inner.to_string())
    }

    pub fn data<T: fmt::Display>(inner: T) -> Self {
        Self::Data(inner.to_string())
    }
}

impl From<molecule::error::VerificationError> for Error {
    fn from(error: molecule::error::VerificationError) -> Self {
        Self::Data(error.to_string())
    }
}

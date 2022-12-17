use std::{marker::PhantomData, path::Path, sync::Arc};

use rocksdb::{
    prelude::{
        GetColumnFamilys as _, GetPinned as _, GetPinnedCF as _, OpenCF as _, Put as _, PutCF as _,
    },
    ColumnFamily, ColumnFamilyDescriptor, DBPinnableSlice, Options, DB,
};

use crate::{
    error::{Error, Result},
    schemas::columns::{self, Column},
};

mod cache;
mod mmr;
mod reader;
mod writer;

use cache::Cache;

#[derive(Clone)]
pub struct Storage<S> {
    pub(crate) db: Arc<DB>,
    pub(crate) cache: Arc<Cache>,
    _phantom_data: PhantomData<S>,
}

impl<S> Storage<S> {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let cf_names = {
            let mut cf_names = Vec::with_capacity(columns::COUNT);
            cf_names.push(columns::COLUMN_BEACON_HEADER_MMR.to_string());
            cf_names
        };
        let cf_descriptors: Vec<_> = cf_names
            .iter()
            .map(|c| ColumnFamilyDescriptor::new(c, Options::default()))
            .collect();

        let opts = {
            let mut opts = Options::default();
            opts.create_if_missing(true);
            opts.create_missing_column_families(true);
            opts
        };

        let db = DB::open_cf_descriptors(&opts, path.as_ref(), cf_descriptors)?;
        let cache = Cache::default();
        let storage = Self {
            db: Arc::new(db),
            cache: Arc::new(cache),
            _phantom_data: PhantomData,
        };

        Ok(storage)
    }

    pub(crate) fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<DBPinnableSlice>> {
        self.db.get_pinned(key.as_ref()).map_err(Into::into)
    }

    pub(crate) fn put<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, value: V) -> Result<()> {
        self.db
            .put(key.as_ref(), value.as_ref())
            .map_err(Into::into)
    }

    pub(crate) fn get_cf<K: AsRef<[u8]>>(
        &self,
        col: Column,
        key: K,
    ) -> Result<Option<DBPinnableSlice>> {
        let cf = cf_handle(&self.db, col)?;
        self.db.get_pinned_cf(cf, key.as_ref()).map_err(Into::into)
    }

    pub(crate) fn put_cf<K: AsRef<[u8]>, V: AsRef<[u8]>>(
        &self,
        col: Column,
        key: K,
        value: V,
    ) -> Result<()> {
        let cf = cf_handle(&self.db, col)?;
        self.db
            .put_cf(cf, key.as_ref(), value.as_ref())
            .map_err(Into::into)
    }
}

pub(crate) fn cf_handle(db: &DB, col: Column) -> Result<&ColumnFamily> {
    db.cf_handle(col)
        .ok_or_else(|| Error::storage(format!("column {} not found", col)))
}

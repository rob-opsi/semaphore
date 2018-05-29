use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use rocksdb::{compaction_filter::Decision, ColumnFamilyDescriptor, Error as RocksDbError, Options,
              DB as RocksDb};
use serde::de::IgnoredAny;
use serde::ser::Serialize;
use serde_cbor;

use traits::Cachable;

/// Represents an error from the store.
#[derive(Debug, Fail)]
pub enum StoreError {
    /// Indicates that the store could not be opened.
    #[fail(display = "cannot open store")]
    CannotOpen(#[cause] RocksDbError),
    /// Indicates that writing to the db failed.
    #[fail(display = "cannot write to database")]
    WriteError(#[cause] RocksDbError),
    /// Indicates that reading from the db failed.
    #[fail(display = "cannot read from database")]
    ReadError(#[cause] RocksDbError),
    /// Raised if repairs failed
    #[fail(display = "could not repair storage")]
    RepairFailed(#[cause] RocksDbError),
    /// Raised on deserialization errors.
    #[fail(display = "cannot deseralize value from database")]
    DeserializeError(#[cause] serde_cbor::error::Error),
}

/// Represents the store for the persistence layer.
#[derive(Debug)]
pub struct Store {
    db: RocksDb,
    path: PathBuf,
}

#[derive(Debug, PartialEq)]
enum FamilyType {
    Queue,
    Cache,
}

impl Store {
    /// Opens a store for the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Store, StoreError> {
        let path = path.as_ref().to_path_buf();
        let opts = get_database_options();
        let cfs = vec![
            ColumnFamilyDescriptor::new("cache", get_column_family_options(FamilyType::Cache)),
            ColumnFamilyDescriptor::new("queue", get_column_family_options(FamilyType::Queue)),
        ];
        let db = RocksDb::open_cf_descriptors(&opts, &path, cfs).map_err(StoreError::CannotOpen)?;
        Ok(Store { db, path })
    }

    /// Attempts to repair the store.
    pub fn repair<P: AsRef<Path>>(path: P) -> Result<(), StoreError> {
        RocksDb::repair(get_database_options(), path).map_err(StoreError::RepairFailed)
    }

    /// Returns the path of the store.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Caches a certain value.
    pub fn cache_set<C: Cachable>(
        &self,
        key: &str,
        value: &C,
        ttl: Option<Duration>,
    ) -> Result<(), StoreError> {
        #[derive(Serialize)]
        pub struct CacheItem<'a, T: Serialize + 'a>(Option<DateTime<Utc>>, u32, &'a T);
        self.db
            .put_cf(
                self.db.cf_handle("cache").unwrap(),
                key.as_bytes(),
                &serde_cbor::to_vec(&CacheItem(
                    ttl.map(|x| Utc::now() + x),
                    C::cache_version(),
                    value,
                )).unwrap(),
            )
            .map_err(StoreError::WriteError)
    }

    /// Remove a key from the cache.
    pub fn cache_remove(&self, key: &str) -> Result<(), StoreError> {
        self.db
            .delete_cf(self.db.cf_handle("cache").unwrap(), key.as_bytes())
            .map_err(StoreError::WriteError)
    }

    /// Looks up a value in the cache.
    pub fn cache_get<C: Cachable>(&self, key: &str) -> Result<Option<C>, StoreError> {
        #[derive(Deserialize)]
        pub struct CacheItem<T>(Option<DateTime<Utc>>, u32, T);
        match self.db
            .get_cf(self.db.cf_handle("cache").unwrap(), key.as_bytes())
        {
            Ok(Some(value)) => {
                let item: CacheItem<C> =
                    serde_cbor::from_slice(&value).map_err(StoreError::DeserializeError)?;
                if item.1 != C::cache_version() {
                    return Ok(None);
                }
                match item.0 {
                    None => Ok(Some(item.2)),
                    Some(ts) if ts > Utc::now() => Ok(Some(item.2)),
                    _ => Ok(None),
                }
            }
            Ok(None) => Ok(None),
            Err(err) => Err(StoreError::ReadError(err)),
        }
    }

    /// Looks up a value in the cache pruning invalid items.
    ///
    /// This is similar to `cache_get` but in case the value coming back from the cache
    /// does not correspond go the given format the value is dropped and `None` is
    /// returned instead of producing an error.
    pub fn cache_get_safe<D: Cachable>(&self, key: &str) -> Result<Option<D>, StoreError> {
        self.cache_get(key).or_else(|err| match err {
            StoreError::DeserializeError(..) => {
                self.cache_remove(key).ok();
                Ok(None)
            }
            err => Err(err),
        })
    }
}

fn ttl_compaction_filter(_level: u32, _key: &[u8], value: &[u8]) -> Decision {
    #[derive(Deserialize)]
    pub struct TtlInfo(Option<DateTime<Utc>>, u32, IgnoredAny);

    serde_cbor::from_slice::<TtlInfo>(value)
        .ok()
        .and_then(|x| x.0)
        .map_or(Decision::Keep, |value| {
            if value < Utc::now() {
                Decision::Remove
            } else {
                Decision::Keep
            }
        })
}

fn get_column_family_options(family: FamilyType) -> Options {
    let mut cf_opts = Options::default();
    cf_opts.set_max_write_buffer_number(4);
    if family == FamilyType::Cache {
        cf_opts.set_compaction_filter("ttl", ttl_compaction_filter);
    }
    cf_opts
}

fn get_database_options() -> Options {
    let mut db_opts = Options::default();
    db_opts.create_missing_column_families(true);
    db_opts.create_if_missing(true);
    db_opts
}

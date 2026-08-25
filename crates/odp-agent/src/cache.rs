use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheRecord {
    pub body: Vec<u8>,
    pub etag: Option<String>,
    pub expires_at: SystemTime,
    pub final_url: String,
    pub last_modified: Option<String>,
    pub status: u16,
    pub stored_at: SystemTime,
}

pub trait Cache: Send + Sync {
    fn delete(&self, key: &str) -> Result<(), String>;
    fn get(&self, key: &str) -> Result<Option<CacheRecord>, String>;
    fn set(&self, key: String, record: CacheRecord) -> Result<(), String>;
}

#[derive(Default)]
pub struct MemoryCache {
    records: RwLock<BTreeMap<String, CacheRecord>>,
}

impl MemoryCache {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Cache for MemoryCache {
    fn delete(&self, key: &str) -> Result<(), String> {
        self.records
            .write()
            .map_err(|_| "memory cache lock is poisoned".to_owned())?
            .remove(key);
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<CacheRecord>, String> {
        Ok(self
            .records
            .read()
            .map_err(|_| "memory cache lock is poisoned".to_owned())?
            .get(key)
            .cloned())
    }

    fn set(&self, key: String, record: CacheRecord) -> Result<(), String> {
        self.records
            .write()
            .map_err(|_| "memory cache lock is poisoned".to_owned())?
            .insert(key, record);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheFallbacks {
    pub collection: Duration,
    pub offering: Duration,
    pub service_document: Duration,
}

impl Default for CacheFallbacks {
    fn default() -> Self {
        Self {
            collection: Duration::from_secs(60 * 60),
            offering: Duration::from_secs(5 * 60),
            service_document: Duration::from_secs(4 * 60 * 60),
        }
    }
}

pub(crate) fn default_cache() -> Arc<dyn Cache> {
    Arc::new(MemoryCache::new())
}

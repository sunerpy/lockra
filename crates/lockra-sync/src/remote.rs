//! The storage a sync space lives in, as this crate needs it: list a directory, read, write and
//! delete objects, with etags. lockra-remote implements it for S3 and WebDAV; [`MemoryRemote`]
//! stands in for both in tests.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;

use crate::SyncError;

/// A storage call in flight.
pub type RemoteFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, SyncError>> + Send + 'a>>;

/// An object in a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
    /// Its name within the listed directory.
    pub name: String,
    /// Its etag, when the storage gives one.
    pub etag: Option<String>,
    /// Its size in bytes.
    pub size: u64,
}

/// When a write may replace what is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutCondition {
    /// Always.
    Always,
    /// Only when nothing is there yet.
    IfAbsent,
    /// Only when the object still has this etag.
    IfMatch(String),
}

/// A sync space's storage.
pub trait RemoteStore: Send + Sync {
    /// The storage honours [`PutCondition`]s (S3 does; WebDAV through lockra-remote does not, and
    /// writes as if every condition were [`PutCondition::Always`]).
    fn conditional_puts(&self) -> bool;
    /// The objects directly in `dir` (a path ending in `/`); empty when the directory is absent.
    fn list<'a>(&'a self, dir: &'a str) -> RemoteFuture<'a, Vec<ObjectMeta>>;
    /// The object at `path` and its etag; `None` when absent.
    fn get<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, Option<(Vec<u8>, Option<String>)>>;
    /// Write `bytes` to `path`, creating directories as needed; the new etag when the storage says.
    /// A condition that fails is [`SyncError::Conflict`].
    fn put<'a>(&'a self, path: &'a str, bytes: Vec<u8>, condition: PutCondition) -> RemoteFuture<'a, Option<String>>;
    /// Remove the object at `path`; an absent object is not an error.
    fn delete<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, ()>;
}

/// A storage in memory: etags count the writes. With `conditional` off it ignores conditions, the
/// way WebDAV behaves. A planned failure answers the next call.
#[derive(Debug, Default)]
pub struct MemoryRemote {
    objects: Mutex<BTreeMap<String, (Vec<u8>, String)>>,
    writes: AtomicU64,
    conditional: bool,
    failure: Mutex<Option<SyncError>>,
    calls: Mutex<Vec<String>>,
}

impl MemoryRemote {
    /// An empty storage that honours conditions when `conditional`.
    pub fn new(conditional: bool) -> Self {
        Self { conditional, ..Self::default() }
    }

    /// The object at `path`, as stored.
    pub fn object(&self, path: &str) -> Option<Vec<u8>> {
        self.objects.lock().get(path).map(|(bytes, _)| bytes.clone())
    }

    /// Write behind the devices' backs (a test's tampering or rollback); a fresh etag.
    pub fn set_object(&self, path: &str, bytes: Vec<u8>) {
        let etag = self.next_etag();
        self.objects.lock().insert(path.to_owned(), (bytes, etag));
    }

    /// Every stored path.
    pub fn paths(&self) -> Vec<String> {
        self.objects.lock().keys().cloned().collect()
    }

    /// The next call fails with `error`.
    pub fn fail_next(&self, error: SyncError) {
        *self.failure.lock() = Some(error);
    }

    /// The calls so far, as `list dir`, `get path`, `put path`, `delete path`.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().clone()
    }

    fn next_etag(&self) -> String {
        format!("\"{}\"", self.writes.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn enter(&self, call: String) -> Result<(), SyncError> {
        self.calls.lock().push(call);
        match self.failure.lock().take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl RemoteStore for MemoryRemote {
    fn conditional_puts(&self) -> bool {
        self.conditional
    }

    fn list<'a>(&'a self, dir: &'a str) -> RemoteFuture<'a, Vec<ObjectMeta>> {
        Box::pin(async move {
            self.enter(format!("list {dir}"))?;
            let objects = self.objects.lock();
            Ok(objects
                .iter()
                .filter_map(|(path, (bytes, etag))| {
                    let name = path.strip_prefix(dir)?;
                    (!name.is_empty() && !name.contains('/')).then(|| ObjectMeta { name: name.to_owned(), etag: Some(etag.clone()), size: bytes.len() as u64 })
                })
                .collect())
        })
    }

    fn get<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, Option<(Vec<u8>, Option<String>)>> {
        Box::pin(async move {
            self.enter(format!("get {path}"))?;
            Ok(self.objects.lock().get(path).map(|(bytes, etag)| (bytes.clone(), Some(etag.clone()))))
        })
    }

    fn put<'a>(&'a self, path: &'a str, bytes: Vec<u8>, condition: PutCondition) -> RemoteFuture<'a, Option<String>> {
        Box::pin(async move {
            self.enter(format!("put {path}"))?;
            let mut objects = self.objects.lock();
            if self.conditional {
                let current = objects.get(path).map(|(_, etag)| etag.as_str());
                let allowed = match &condition {
                    PutCondition::Always => true,
                    PutCondition::IfAbsent => current.is_none(),
                    PutCondition::IfMatch(etag) => current == Some(etag.as_str()),
                };
                if !allowed {
                    return Err(SyncError::Conflict);
                }
            }
            let etag = self.next_etag();
            objects.insert(path.to_owned(), (bytes, etag.clone()));
            Ok(Some(etag))
        })
    }

    fn delete<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, ()> {
        Box::pin(async move {
            self.enter(format!("delete {path}"))?;
            self.objects.lock().remove(path);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_memory_store_lists_reads_writes_and_deletes_with_etags() {
        let store = MemoryRemote::new(true);
        assert!(store.list("a/").await.unwrap().is_empty());
        let first = store.put("a/x", b"1".to_vec(), PutCondition::IfAbsent).await.unwrap().unwrap();
        store.put("a/b/y", b"2".to_vec(), PutCondition::Always).await.unwrap();
        let listing = store.list("a/").await.unwrap();
        assert_eq!(listing, [ObjectMeta { name: "x".into(), etag: Some(first.clone()), size: 1 }], "only the objects directly inside");
        assert_eq!(store.get("a/x").await.unwrap(), Some((b"1".to_vec(), Some(first.clone()))));
        assert_eq!(store.put("a/x", b"3".to_vec(), PutCondition::IfAbsent).await.err(), Some(SyncError::Conflict));
        assert_eq!(store.put("a/x", b"3".to_vec(), PutCondition::IfMatch("\"9\"".into())).await.err(), Some(SyncError::Conflict));
        let second = store.put("a/x", b"3".to_vec(), PutCondition::IfMatch(first)).await.unwrap().unwrap();
        assert_eq!(store.object("a/x"), Some(b"3".to_vec()));
        store.delete("a/x").await.unwrap();
        store.delete("a/x").await.unwrap();
        assert_eq!(store.get("a/x").await.unwrap(), None);
        assert_eq!(store.paths(), ["a/b/y"]);
        assert_ne!(second, "\"1\"");
        assert!(store.conditional_puts());
        assert_eq!(store.calls().len(), 11);
    }

    #[tokio::test]
    async fn a_store_without_conditions_overwrites_and_planned_failures_answer_once() {
        let store = MemoryRemote::new(false);
        store.put("x", b"1".to_vec(), PutCondition::IfAbsent).await.unwrap();
        store.put("x", b"2".to_vec(), PutCondition::IfAbsent).await.unwrap();
        assert_eq!(store.object("x"), Some(b"2".to_vec()));
        assert!(!store.conditional_puts());
        store.fail_next(SyncError::Network("offline".into()));
        assert_eq!(store.get("x").await.err(), Some(SyncError::Network("offline".into())));
        assert!(store.get("x").await.unwrap().is_some());
        store.set_object("y", b"z".to_vec());
        assert_eq!(store.object("y"), Some(b"z".to_vec()));
    }
}

//! A Lockra relay as a sync space's storage (docs/formats.md §9, "The relay"; docs/relay.md): the
//! relay's HTTP API over the same HTTPS client as S3 and WebDAV, with the space's access token
//! (lockra-sync's [`SpaceAccess`], derived from the sync key) as a bearer token. A relay holds a
//! write's condition, so this device's own writes catch a copied vault at once, as on S3.
//!
//! [`RelayWatch`] waits on the relay for the space to change (a listing the relay holds back until
//! a snapshot is written or removed), so that a run follows within a moment rather than at the
//! next interval.

use std::fmt;
use std::time::Duration;

use lockra_sync::{MAX_OBJECT_BYTES, ObjectMeta, PutCondition, RemoteFuture, RemoteStore, SpaceAccess, SyncError};
use reqwest::header::{CONTENT_LENGTH, ETAG, IF_MATCH, IF_NONE_MATCH};
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::http_client;

/// The sync engine's root directory, which a relay keeps each space under by its id.
const ROOT: &str = "lockra-sync-v1/";
/// How long a watch asks the relay to hold a listing back (its own limit may be lower).
const WAIT_SECONDS: u64 = 25;
/// The first wait after a failed watch request; it doubles up to [`MAX_BACKOFF`].
const MIN_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(300);
/// A relay (or a proxy before it) that answers at once although nothing changed is asked again
/// only after this long.
const QUIET: Duration = Duration::from_secs(30);
/// Never more than one watch request in this time.
const MIN_INTERVAL: Duration = Duration::from_secs(1);
/// A listing of a relay's largest space is a few kilobytes.
const MAX_LISTING_BYTES: u64 = 1024 * 1024;

/// A sync space's storage on a Lockra relay, ready for requests.
pub struct RelayStore {
    client: Client,
    base: Url,
    token: Zeroizing<String>,
}

impl fmt::Debug for RelayStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RelayStore").field("base", &self.base.as_str()).finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct Listing {
    objects: Vec<Listed>,
}

#[derive(Deserialize)]
struct Listed {
    name: String,
    size: u64,
    etag: String,
}

impl RelayStore {
    /// The relay at `url` (checked as [`lockra_sync::StorageConfig::validate`] does), for the space
    /// whose access `access` is; nothing contacted yet.
    pub fn open(url: &str, access: &SpaceAccess) -> Result<Self, SyncError> {
        let mut base = Url::parse(url.trim()).map_err(|_| SyncError::Storage("not a relay address".into()))?;
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        Ok(Self { client: http_client()?, base, token: Zeroizing::new(access.relay_token().to_owned()) })
    }

    /// The API's address for space `id`'s device `name` (the listing for an empty name).
    fn url(&self, id: Uuid, name: &str) -> Result<Url, SyncError> {
        self.base.join(&format!("v1/spaces/{id}/devices/{name}")).map_err(|_| SyncError::Storage("not a relay address".into()))
    }

    /// The space and object an engine path names: `lockra-sync-v1/<space id>/devices/<name>`, the
    /// name empty for the directory. A relay keeps nothing else.
    fn locate(&self, path: &str, directory: bool) -> Result<Url, SyncError> {
        let (id, name) = locate(path).ok_or_else(|| SyncError::Storage(format!("a relay keeps no {path}")))?;
        if name.is_empty() != directory {
            return Err(SyncError::Storage(format!("a relay keeps no {path}")));
        }
        self.url(id, name)
    }

    fn get_request(&self, url: Url) -> reqwest::RequestBuilder {
        self.client.get(url).bearer_auth(self.token.as_str())
    }
}

/// `(space id, object name)` of an engine path under [`ROOT`].
fn locate(path: &str) -> Option<(Uuid, &str)> {
    let (id, rest) = path.strip_prefix(ROOT)?.split_once('/')?;
    let name = rest.strip_prefix("devices/")?;
    let id = Uuid::try_parse(id).ok()?;
    (!name.contains('/')).then_some((id, name))
}

/// What the engine tells apart in a relay's refusal.
fn refused(status: StatusCode) -> SyncError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => SyncError::Denied,
        StatusCode::PRECONDITION_FAILED => SyncError::Conflict,
        StatusCode::NOT_FOUND => SyncError::Storage("no Lockra relay at this address".into()),
        StatusCode::PAYLOAD_TOO_LARGE => SyncError::Storage("the snapshot is larger than the relay keeps".into()),
        StatusCode::INSUFFICIENT_STORAGE => SyncError::Storage("the relay is full".into()),
        StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_MANY_REQUESTS => SyncError::Network(format!("the relay answered {status}")),
        status if status.is_server_error() => SyncError::Network(format!("the relay answered {status}")),
        status => SyncError::Storage(format!("the relay answered {status}")),
    }
}

/// A request that did not get an answer: the relay could not be reached.
fn unreached(error: &reqwest::Error) -> SyncError {
    SyncError::Network(error.to_string())
}

async fn send(request: reqwest::RequestBuilder) -> Result<Response, SyncError> {
    request.send().await.map_err(|e| unreached(&e.without_url()))
}

/// The body of `response`, refused past `limit` bytes.
async fn bounded_body(mut response: Response, limit: u64) -> Result<Vec<u8>, SyncError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| unreached(&e.without_url()))? {
        if (bytes.len() + chunk.len()) as u64 > limit {
            return Err(SyncError::Corrupted);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn etag_of(response: &Response) -> Option<String> {
    response.headers().get(ETAG).and_then(|v| v.to_str().ok()).map(str::to_owned)
}

impl RemoteStore for RelayStore {
    fn conditional_puts(&self) -> bool {
        true
    }

    fn list<'a>(&'a self, dir: &'a str) -> RemoteFuture<'a, Vec<ObjectMeta>> {
        Box::pin(async move {
            let response = send(self.get_request(self.locate(dir, true)?)).await?;
            if !response.status().is_success() {
                return Err(refused(response.status()));
            }
            let body = bounded_body(response, MAX_LISTING_BYTES).await?;
            let listing: Listing = serde_json::from_slice(&body).map_err(|_| SyncError::Storage("not a Lockra relay's listing".into()))?;
            Ok(listing.objects.into_iter().map(|o| ObjectMeta { name: o.name, etag: Some(o.etag), size: o.size }).collect())
        })
    }

    fn get<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, Option<(Vec<u8>, Option<String>)>> {
        Box::pin(async move {
            let response = send(self.get_request(self.locate(path, false)?)).await?;
            match response.status() {
                StatusCode::NOT_FOUND => return Ok(None),
                status if !status.is_success() => return Err(refused(status)),
                _ => {}
            }
            // A larger object than any snapshot is not read, and neither is more than the size the
            // relay gave.
            let declared = response.headers().get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
            if declared.is_some_and(|size| size > MAX_OBJECT_BYTES) {
                return Err(SyncError::Corrupted);
            }
            let etag = etag_of(&response);
            let bytes = bounded_body(response, declared.unwrap_or(MAX_OBJECT_BYTES)).await?;
            Ok(Some((bytes, etag)))
        })
    }

    fn put<'a>(&'a self, path: &'a str, bytes: Vec<u8>, condition: PutCondition) -> RemoteFuture<'a, Option<String>> {
        Box::pin(async move {
            let request = self.client.put(self.locate(path, false)?).bearer_auth(self.token.as_str()).body(bytes);
            let request = match &condition {
                PutCondition::Always => request,
                PutCondition::IfAbsent => request.header(IF_NONE_MATCH, "*"),
                PutCondition::IfMatch(etag) => request.header(IF_MATCH, etag.as_str()),
            };
            let response = send(request).await?;
            if !response.status().is_success() {
                return Err(refused(response.status()));
            }
            Ok(etag_of(&response))
        })
    }

    fn delete<'a>(&'a self, path: &'a str) -> RemoteFuture<'a, ()> {
        Box::pin(async move {
            let response = send(self.client.delete(self.locate(path, false)?).bearer_auth(self.token.as_str())).await?;
            match response.status() {
                StatusCode::NOT_FOUND => Ok(()),
                status if status.is_success() => Ok(()),
                status => Err(refused(status)),
            }
        })
    }
}

/// A watch on a space at a relay; dropping it stops the watch.
pub struct RelayWatch {
    task: tokio::task::AbortHandle,
}

impl fmt::Debug for RelayWatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RelayWatch").finish_non_exhaustive()
    }
}

impl Drop for RelayWatch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl RelayWatch {
    /// Wait on the relay at `url` for the directory `dir` (the space's devices, as the engine
    /// names it) to change: `changed` is called each time a snapshot there is written or removed,
    /// from a task of the current Tokio runtime.
    pub fn start(url: &str, access: &SpaceAccess, dir: &str, changed: Box<dyn Fn() + Send + Sync>) -> Result<Self, SyncError> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| SyncError::Storage("no runtime to watch the relay from".into()))?;
        let store = RelayStore::open(url, access)?;
        let listing = store.locate(dir, true)?;
        let task = runtime.spawn(watch(store, listing, changed));
        Ok(Self { task: task.abort_handle() })
    }
}

/// Ask for the listing again and again, the relay holding each answer back until something
/// changes; a new revision after the first is a change. A failed request waits longer each time.
async fn watch(store: RelayStore, mut listing: Url, changed: Box<dyn Fn() + Send + Sync>) {
    listing.query_pairs_mut().append_pair("wait", &WAIT_SECONDS.to_string());
    let mut known: Option<String> = None;
    let mut backoff = MIN_BACKOFF;
    loop {
        let started = tokio::time::Instant::now();
        let mut request = store.get_request(listing.clone());
        if let Some(known) = &known {
            request = request.header(IF_NONE_MATCH, known.as_str());
        }
        match request.send().await {
            Ok(response) if response.status() == StatusCode::NOT_MODIFIED => backoff = MIN_BACKOFF,
            Ok(response) if response.status().is_success() => {
                let revision = etag_of(&response);
                if known.is_some() && revision != known {
                    changed();
                }
                let answered_at_once = revision.is_none() || revision == known;
                known = revision;
                backoff = MIN_BACKOFF;
                if answered_at_once {
                    tokio::time::sleep(QUIET).await;
                }
            }
            _ => {
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
        tokio::time::sleep_until(started + MIN_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_paths_map_to_the_api_and_nothing_else_does() {
        let id = Uuid::new_v4();
        let access = SpaceAccess::of(&lockra_sync::SyncKey::generate().unwrap());
        let store = RelayStore::open("https://relay.example.com/lockra", &access).unwrap();
        let dir = format!("lockra-sync-v1/{id}/devices/");
        assert_eq!(store.locate(&dir, true).unwrap().as_str(), format!("https://relay.example.com/lockra/v1/spaces/{id}/devices/"));
        let object = format!("{dir}0123456789abcdef0123456789abcdef.lks");
        assert_eq!(
            store.locate(&object, false).unwrap().as_str(),
            format!("https://relay.example.com/lockra/v1/spaces/{id}/devices/0123456789abcdef0123456789abcdef.lks")
        );
        for bad in
            ["contract/a.lks".to_owned(), format!("lockra-sync-v1/{id}/other/x"), format!("{dir}sub/x.lks"), "lockra-sync-v1/not-an-id/devices/".to_owned()]
        {
            assert!(store.locate(&bad, false).is_err() && store.locate(&bad, true).is_err(), "{bad}");
        }
        // A directory where an object is wanted, and the other way round.
        assert!(store.locate(&dir, false).is_err());
        assert!(store.locate(&object, true).is_err());
        assert!(format!("{store:?}").contains("relay.example.com") && !format!("{store:?}").contains(access.relay_token()));
        assert!(RelayStore::open("not an address", &access).is_err());
    }

    #[test]
    fn refusals_map_to_what_the_engine_tells_apart() {
        assert_eq!(refused(StatusCode::UNAUTHORIZED), SyncError::Denied);
        assert_eq!(refused(StatusCode::FORBIDDEN), SyncError::Denied);
        assert_eq!(refused(StatusCode::PRECONDITION_FAILED), SyncError::Conflict);
        for status in [StatusCode::NOT_FOUND, StatusCode::PAYLOAD_TOO_LARGE, StatusCode::INSUFFICIENT_STORAGE, StatusCode::BAD_REQUEST] {
            assert!(matches!(refused(status), SyncError::Storage(_)), "{status}");
        }
        for status in [StatusCode::REQUEST_TIMEOUT, StatusCode::TOO_MANY_REQUESTS, StatusCode::SERVICE_UNAVAILABLE, StatusCode::BAD_GATEWAY] {
            assert!(matches!(refused(status), SyncError::Network(_)), "{status}");
        }
    }
}

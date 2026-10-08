//! The relay's HTTP API (docs/formats.md, "Relay"):
//!
//! ```text
//! GET    /healthz                              ok
//! GET    /v1/                                  what this relay is
//! GET    /v1/spaces/{id}/devices/              the space's snapshots (JSON); ETag: the listing's revision;
//!                                              If-None-Match: it → 304; ?wait=N holds a 304 back up to
//!                                              N seconds, until something changes
//! GET    /v1/spaces/{id}/devices/{tag}.lks     a snapshot, its ETag
//! PUT    /v1/spaces/{id}/devices/{tag}.lks     write one; If-None-Match: * or If-Match: "…" → 412
//! DELETE /v1/spaces/{id}/devices/{tag}.lks     remove one
//! ```
//!
//! Every `/v1/spaces` request carries `Authorization: Bearer <token>`, the space's access token: 32
//! bytes in unpadded Base64url. The first write of a space binds it to its token.

use std::convert::Infallible;
use std::error::Error as StdError;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use data_encoding::BASE64URL_NOPAD;
use http::header::{self, HeaderMap, HeaderValue};
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt as _, Full, LengthLimitError, Limited};
use hyper::body::Body;
use uuid::Uuid;

use crate::config::{Config, IpNet, Limits, canonical};
use crate::limit::{Rate, client_key};
use crate::store::{Access, Condition, Listing, Store, StoreError, empty_revision, is_object_name, space_id};

/// How long a client may take to send a snapshot.
const BODY_TIMEOUT: Duration = Duration::from_secs(60);
/// What a refused rate asks the client to wait, in seconds.
const RETRY_AFTER: &str = "10";
/// The relay's version (Lockra's release).
pub const VERSION: &str = env!("LOCKRA_VERSION");

/// The relay's state: the spaces, the settings and the rates.
pub struct App {
    pub(crate) store: Store,
    pub(crate) limits: Limits,
    trusted: Vec<IpNet>,
    per_client: Rate<IpAddr>,
    per_space: Rate<Uuid>,
    new_spaces: Rate<IpAddr>,
    served: AtomicU64,
    limited: AtomicU64,
}

impl App {
    /// The relay for `config`, over `store`.
    pub fn new(store: Store, config: &Config) -> Self {
        let limits = config.limits;
        Self {
            store,
            limits,
            trusted: config.trusted_proxies.clone(),
            per_client: Rate::new(limits.requests_per_minute, Duration::from_secs(60)),
            per_space: Rate::new(limits.space_requests_per_minute, Duration::from_secs(60)),
            new_spaces: Rate::new(limits.spaces_per_hour, Duration::from_secs(3600)),
            served: AtomicU64::new(0),
            limited: AtomicU64::new(0),
        }
    }

    /// Forget the rates that are full again.
    pub(crate) fn sweep(&self, now: Instant) {
        self.per_client.sweep(now);
        self.per_space.sweep(now);
        self.new_spaces.sweep(now);
    }

    /// Requests served and refused for their rate since the last call.
    pub(crate) fn take_counts(&self) -> (u64, u64) {
        (self.served.swap(0, Ordering::Relaxed), self.limited.swap(0, Ordering::Relaxed))
    }
}

/// The answer to `request`, which came from `peer`.
pub async fn handle<B>(app: &App, peer: IpAddr, request: Request<B>) -> Response<Full<Bytes>>
where
    B: Body<Data = Bytes>,
    B::Error: Into<Box<dyn StdError + Send + Sync>>,
{
    app.served.fetch_add(1, Ordering::Relaxed);
    let path = request.uri().path().to_owned();
    let method = request.method().as_str().to_owned();
    match path.as_str() {
        "/healthz" if method == "GET" => return text(StatusCode::OK, "ok\n"),
        "/v1" | "/v1/" if method == "GET" => {
            return json(StatusCode::OK, &serde_json::json!({ "service": "lockra-relay", "api": 1, "version": VERSION }));
        }
        "/healthz" | "/v1" | "/v1/" => return not_allowed("GET"),
        _ => {}
    }
    let Some(rest) = path.strip_prefix("/v1/spaces/") else { return error(StatusCode::NOT_FOUND, "not_found") };
    let Some((id, rest)) = rest.split_once('/') else { return error(StatusCode::NOT_FOUND, "not_found") };
    let Some(object) = rest.strip_prefix("devices/") else { return error(StatusCode::NOT_FOUND, "not_found") };
    let Some(id) = space_id(id) else { return error(StatusCode::NOT_FOUND, "not_found") };
    if !object.is_empty() && !is_object_name(object) {
        return error(StatusCode::NOT_FOUND, "not_found");
    }
    let client = client_key(client_ip(peer, request.headers(), &app.trusted));
    let now = Instant::now();
    if !app.per_client.allow(&client, now) {
        return limited(app);
    }
    let Some(access) = access(request.headers()) else {
        let mut response = error(StatusCode::UNAUTHORIZED, "unauthorized");
        response.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return response;
    };
    if !app.per_space.allow(&id, now) {
        return limited(app);
    }
    if object.is_empty() {
        return match method.as_str() {
            "GET" => list(app, id, &access, &request).await,
            _ => not_allowed("GET"),
        };
    }
    let object = object.to_owned();
    match method.as_str() {
        "GET" => match app.store.get(id, &access, &object).await {
            Ok(Some((bytes, etag))) => {
                let mut response = respond(StatusCode::OK, bytes, "application/octet-stream");
                set_etag(&mut response, &etag);
                response
            }
            Ok(None) => error(StatusCode::NOT_FOUND, "not_found"),
            Err(e) => store_error(e),
        },
        "PUT" => {
            let condition = match condition(request.headers()) {
                Ok(condition) => condition,
                Err((status, code)) => return error(status, code),
            };
            if !app.store.exists(id) && !app.new_spaces.allow(&client, now) {
                return limited(app);
            }
            let bytes = match read_body(request, app.limits.max_object_bytes).await {
                Ok(bytes) => bytes,
                Err((status, code)) => return error(status, code),
            };
            match app.store.put(id, &access, &object, bytes, condition).await {
                Ok(etag) => {
                    let mut response = respond(StatusCode::OK, Bytes::new(), "text/plain; charset=utf-8");
                    set_etag(&mut response, &etag);
                    response
                }
                Err(e) => store_error(e),
            }
        }
        "DELETE" => match app.store.delete(id, &access, &object).await {
            Ok(()) => respond(StatusCode::NO_CONTENT, Bytes::new(), "text/plain; charset=utf-8"),
            Err(e) => store_error(e),
        },
        _ => not_allowed("GET, PUT, DELETE"),
    }
}

/// The space's listing: at once, or, asked to wait with the revision the client has, when it
/// changes or the wait is over (then 304).
async fn list<B>(app: &App, id: Uuid, access: &Access, request: &Request<B>) -> Response<Full<Bytes>> {
    let known = request.headers().get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).map(str::trim).map(str::to_owned);
    let wait = request
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|pair| pair.strip_prefix("wait=")))
        .and_then(|n| n.parse::<u64>().ok())
        .map_or(Duration::ZERO, |n| Duration::from_secs(n).min(app.limits.max_wait));
    if let Some(known) = known.as_deref().filter(|_| !wait.is_zero()) {
        let deadline = tokio::time::Instant::now() + wait;
        match app.store.watch(id, access).await {
            Ok(Some(mut revision)) => {
                while *revision.borrow_and_update() == known {
                    tokio::select! {
                        changed = revision.changed() => if changed.is_err() { break },
                        () = tokio::time::sleep_until(deadline) => break,
                    }
                }
            }
            // A space the relay does not keep changes only by a write, which nobody waits on.
            Ok(None) if known == empty_revision() => tokio::time::sleep_until(deadline).await,
            Ok(None) => {}
            Err(e) => return store_error(e),
        }
    }
    let listing: Listing = match app.store.list(id, access).await {
        Ok(listing) => listing,
        Err(e) => return store_error(e),
    };
    if known.as_deref() == Some(listing.revision.as_str()) {
        let mut response = respond(StatusCode::NOT_MODIFIED, Bytes::new(), "application/json");
        set_etag(&mut response, &listing.revision);
        return response;
    }
    let mut response = json(StatusCode::OK, &listing);
    set_etag(&mut response, &listing.revision);
    response
}

/// The client: the peer, or, when the peer is a trusted proxy, the last address its
/// `X-Forwarded-For` names that is not one of the proxies.
fn client_ip(peer: IpAddr, headers: &HeaderMap, trusted: &[IpNet]) -> IpAddr {
    let is_trusted = |ip: IpAddr| trusted.iter().any(|net| net.contains(ip));
    if !is_trusted(peer) {
        return canonical(peer);
    }
    let hops: Vec<IpAddr> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .filter_map(|hop| hop.trim().parse().ok())
        .collect();
    hops.iter().rev().copied().find(|hop| !is_trusted(*hop)).or_else(|| hops.first().copied()).map_or(canonical(peer), canonical)
}

/// The space's access, from `Authorization: Bearer <token>`: 32 bytes, unpadded Base64url.
fn access(headers: &HeaderMap) -> Option<Access> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = BASE64URL_NOPAD.decode(token.trim().as_bytes()).ok().filter(|t| t.len() == 32)?;
    Some(Access::of_token(&token))
}

/// A refused request: its status and error code.
type Refusal = (StatusCode, &'static str);

/// The write's condition: `If-None-Match: *` or `If-Match: "<etag>"`, not both.
fn condition(headers: &HeaderMap) -> Result<Condition, Refusal> {
    let header = |name| headers.get(name).map(|v| v.to_str().map(|s| s.trim().to_owned()));
    match (header(header::IF_NONE_MATCH), header(header::IF_MATCH)) {
        (None, None) => Ok(Condition::Always),
        (Some(Ok(star)), None) if star == "*" => Ok(Condition::IfAbsent),
        (None, Some(Ok(etag))) if !etag.is_empty() && etag != "*" => Ok(Condition::IfMatch(etag)),
        _ => Err((StatusCode::BAD_REQUEST, "bad_condition")),
    }
}

/// The request's body, at most `max` bytes, within [`BODY_TIMEOUT`].
async fn read_body<B>(request: Request<B>, max: u64) -> Result<Bytes, Refusal>
where
    B: Body<Data = Bytes>,
    B::Error: Into<Box<dyn StdError + Send + Sync>>,
{
    let declared = request.headers().get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    if declared.is_some_and(|n| n > max) {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, "too_large"));
    }
    let body = Limited::new(request.into_body(), usize::try_from(max).unwrap_or(usize::MAX));
    match tokio::time::timeout(BODY_TIMEOUT, body.collect()).await {
        Ok(Ok(collected)) => Ok(collected.to_bytes()),
        Ok(Err(e)) if e.downcast_ref::<LengthLimitError>().is_some() => Err((StatusCode::PAYLOAD_TOO_LARGE, "too_large")),
        Ok(Err(_)) => Err((StatusCode::BAD_REQUEST, "bad_body")),
        Err(_) => Err((StatusCode::REQUEST_TIMEOUT, "timeout")),
    }
}

fn store_error(error: StoreError) -> Response<Full<Bytes>> {
    match error {
        StoreError::Forbidden => self::error(StatusCode::FORBIDDEN, "forbidden"),
        StoreError::Precondition => self::error(StatusCode::PRECONDITION_FAILED, "precondition_failed"),
        StoreError::TooLarge => self::error(StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
        StoreError::Full(what) => json(StatusCode::INSUFFICIENT_STORAGE, &serde_json::json!({ "error": "full", "limit": what })),
        StoreError::Gone => {
            let mut response = self::error(StatusCode::SERVICE_UNAVAILABLE, "try_again");
            response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
            response
        }
        StoreError::Io(e) => {
            tracing::error!(error = %e, "the disk failed");
            self::error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
        }
    }
}

fn limited(app: &App) -> Response<Full<Bytes>> {
    app.limited.fetch_add(1, Ordering::Relaxed);
    let mut response = error(StatusCode::TOO_MANY_REQUESTS, "rate_limited");
    response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static(RETRY_AFTER));
    response
}

fn not_allowed(allow: &'static str) -> Response<Full<Bytes>> {
    let mut response = error(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
    response.headers_mut().insert(header::ALLOW, HeaderValue::from_static(allow));
    response
}

fn error(status: StatusCode, code: &str) -> Response<Full<Bytes>> {
    json(status, &serde_json::json!({ "error": code }))
}

fn json(status: StatusCode, value: &impl serde::Serialize) -> Response<Full<Bytes>> {
    // Plain data structures serialize to JSON without failing.
    let body = serde_json::to_vec(value).unwrap_or_default();
    respond(status, Bytes::from(body), "application/json")
}

fn text(status: StatusCode, body: &'static str) -> Response<Full<Bytes>> {
    respond(status, Bytes::from_static(body.as_bytes()), "text/plain; charset=utf-8")
}

fn respond(status: StatusCode, body: Bytes, content_type: &'static str) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(body));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}

fn set_etag(response: &mut Response<Full<Bytes>>, etag: &str) {
    if let Ok(value) = HeaderValue::from_str(etag) {
        response.headers_mut().insert(header::ETAG, value);
    }
}

/// Lets a `service_fn` answer every request.
pub(crate) async fn serve<B>(app: &App, peer: IpAddr, request: Request<B>) -> Result<Response<Full<Bytes>>, Infallible>
where
    B: Body<Data = Bytes>,
    B::Error: Into<Box<dyn StdError + Send + Sync>>,
{
    Ok(handle(app, peer, request).await)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use http::Method;

    use super::*;

    const A: &str = "0123456789abcdef0123456789abcdef.lks";
    const B: &str = "fedcba9876543210fedcba9876543210.lks";
    const PEER: &str = "203.0.113.7";

    fn token(n: u8) -> String {
        BASE64URL_NOPAD.encode(&[n; 32])
    }

    fn app_with(dir: &std::path::Path, limits: Limits, trusted: &[&str]) -> App {
        let config = Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            data_dir: PathBuf::from(dir),
            trusted_proxies: trusted.iter().map(|t| IpNet::parse(t).unwrap()).collect(),
            limits,
        };
        App::new(Store::open(dir, limits).unwrap(), &config)
    }

    fn app(dir: &std::path::Path) -> App {
        app_with(dir, Limits { max_object_bytes: 64, max_wait: Duration::from_secs(2), ..Limits::default() }, &[])
    }

    struct Call<'a> {
        method: Method,
        uri: String,
        token: Option<String>,
        headers: Vec<(&'a str, String)>,
        body: Bytes,
    }

    fn call<'a>(method: Method, uri: impl Into<String>) -> Call<'a> {
        Call { method, uri: uri.into(), token: Some(token(1)), headers: Vec::new(), body: Bytes::new() }
    }

    impl<'a> Call<'a> {
        fn token(mut self, token: Option<String>) -> Self {
            self.token = token;
            self
        }
        fn header(mut self, name: &'a str, value: impl Into<String>) -> Self {
            self.headers.push((name, value.into()));
            self
        }
        fn body(mut self, body: &'static [u8]) -> Self {
            self.body = Bytes::from_static(body);
            self
        }
        async fn on(self, app: &App) -> (StatusCode, HeaderMap, Bytes) {
            self.from(app, PEER).await
        }
        async fn from(self, app: &App, peer: &str) -> (StatusCode, HeaderMap, Bytes) {
            let mut request = Request::builder().method(self.method).uri(self.uri);
            if let Some(token) = self.token {
                request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
            }
            for (name, value) in self.headers {
                request = request.header(name, value);
            }
            let response = handle(app, peer.parse().unwrap(), request.body(Full::new(self.body)).unwrap()).await;
            let (parts, body) = response.into_parts();
            (parts.status, parts.headers, body.collect().await.unwrap().to_bytes())
        }
    }

    fn space() -> String {
        Uuid::new_v4().to_string()
    }

    fn devices(space: &str) -> String {
        format!("/v1/spaces/{space}/devices/")
    }

    fn object(space: &str, name: &str) -> String {
        format!("/v1/spaces/{space}/devices/{name}")
    }

    fn json_of(bytes: &Bytes) -> serde_json::Value {
        serde_json::from_slice(bytes).unwrap()
    }

    #[tokio::test]
    async fn health_and_what_the_relay_is() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let (status, headers, body) = call(Method::GET, "/healthz").token(None).on(&app).await;
        assert_eq!((status, body.as_ref()), (StatusCode::OK, b"ok\n".as_slice()));
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        let (status, _, body) = call(Method::GET, "/v1/").token(None).on(&app).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json_of(&body), serde_json::json!({ "service": "lockra-relay", "api": 1, "version": VERSION }));
        let (status, headers, _) = call(Method::POST, "/healthz").on(&app).await;
        assert_eq!((status, headers[header::ALLOW].to_str().unwrap()), (StatusCode::METHOD_NOT_ALLOWED, "GET"));
        for path in ["/", "/v2/spaces", "/v1/spaces/", "/v1/spaces/x", "/v1/spaces/not-an-id/devices/", "/v1/spaces/x/files/"] {
            assert_eq!(call(Method::GET, path).on(&app).await.0, StatusCode::NOT_FOUND, "{path}");
        }
        let id = space();
        assert_eq!(call(Method::GET, format!("/v1/spaces/{id}/other/")).on(&app).await.0, StatusCode::NOT_FOUND);
        assert_eq!(call(Method::GET, object(&id, "notes.txt")).on(&app).await.0, StatusCode::NOT_FOUND);
        assert_eq!(call(Method::GET, format!("/v1/spaces/{}/devices/", id.to_uppercase())).on(&app).await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_space_is_written_listed_read_and_emptied() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let id = space();
        let (status, headers, body) = call(Method::GET, devices(&id)).on(&app).await;
        assert_eq!((status, json_of(&body)), (StatusCode::OK, serde_json::json!({ "objects": [] })));
        assert_eq!(headers[header::ETAG].to_str().unwrap(), empty_revision());
        assert_eq!(call(Method::GET, object(&id, A)).on(&app).await.0, StatusCode::NOT_FOUND);

        let (status, headers, _) = call(Method::PUT, object(&id, A)).header("if-none-match", "*").body(b"sealed").on(&app).await;
        assert_eq!(status, StatusCode::OK);
        let etag = headers[header::ETAG].to_str().unwrap().to_owned();
        let (status, headers, body) = call(Method::GET, object(&id, A)).on(&app).await;
        assert_eq!((status, body.as_ref()), (StatusCode::OK, b"sealed".as_slice()));
        assert_eq!((headers[header::ETAG].to_str().unwrap(), headers[header::CONTENT_TYPE].to_str().unwrap()), (etag.as_str(), "application/octet-stream"));
        let (_, headers, body) = call(Method::GET, devices(&id)).on(&app).await;
        assert_eq!(json_of(&body), serde_json::json!({ "objects": [{ "name": A, "size": 6, "etag": etag }] }));
        let revision = headers[header::ETAG].to_str().unwrap().to_owned();
        // Unchanged since: 304.
        let (status, headers, body) = call(Method::GET, devices(&id)).header("if-none-match", revision.clone()).on(&app).await;
        assert_eq!((status, body.len()), (StatusCode::NOT_MODIFIED, 0));
        assert_eq!(headers[header::ETAG].to_str().unwrap(), revision);

        assert_eq!(call(Method::PUT, object(&id, A)).header("if-none-match", "*").body(b"again").on(&app).await.0, StatusCode::PRECONDITION_FAILED);
        assert_eq!(call(Method::PUT, object(&id, A)).header("if-match", "\"other\"").body(b"again").on(&app).await.0, StatusCode::PRECONDITION_FAILED);
        assert_eq!(call(Method::PUT, object(&id, A)).header("if-match", etag).body(b"again").on(&app).await.0, StatusCode::OK);
        assert_eq!(call(Method::DELETE, object(&id, A)).on(&app).await.0, StatusCode::NO_CONTENT);
        assert_eq!(call(Method::DELETE, object(&id, A)).on(&app).await.0, StatusCode::NO_CONTENT);
        assert_eq!(call(Method::GET, object(&id, A)).on(&app).await.0, StatusCode::NOT_FOUND);
        assert_eq!(call(Method::POST, object(&id, A)).on(&app).await.0, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(call(Method::PUT, devices(&id)).on(&app).await.0, StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn the_token_is_required_and_binds_the_space() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let id = space();
        let (status, headers, _) = call(Method::GET, devices(&id)).token(None).on(&app).await;
        assert_eq!((status, headers[header::WWW_AUTHENTICATE].to_str().unwrap()), (StatusCode::UNAUTHORIZED, "Bearer"));
        for bad in ["short", "!!!!", &BASE64URL_NOPAD.encode(&[1; 31])] {
            assert_eq!(call(Method::GET, devices(&id)).token(Some(bad.to_owned())).on(&app).await.0, StatusCode::UNAUTHORIZED, "{bad}");
        }
        let basic = call(Method::GET, devices(&id)).token(None).header("authorization", format!("Basic {}", token(1))).on(&app).await;
        assert_eq!(basic.0, StatusCode::UNAUTHORIZED);
        let lower = call(Method::GET, devices(&id)).token(None).header("authorization", format!("bearer {}", token(1))).on(&app).await;
        assert_eq!(lower.0, StatusCode::OK);
        assert_eq!(call(Method::PUT, object(&id, A)).body(b"x").on(&app).await.0, StatusCode::OK);
        for request in [
            call(Method::GET, devices(&id)),
            call(Method::GET, object(&id, A)),
            call(Method::PUT, object(&id, B)).body(b"x"),
            call(Method::DELETE, object(&id, A)),
        ] {
            assert_eq!(request.token(Some(token(2))).on(&app).await.0, StatusCode::FORBIDDEN);
        }
    }

    #[tokio::test]
    async fn bad_writes_are_refused_before_anything_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(dir.path(), Limits { max_object_bytes: 4, max_spaces: 1, ..Limits::default() }, &[]);
        let id = space();
        assert_eq!(call(Method::PUT, object(&id, A)).body(b"12345").on(&app).await.0, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(call(Method::PUT, object(&id, A)).header("content-length", "99").body(b"1").on(&app).await.0, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(call(Method::PUT, object(&id, A)).header("if-none-match", "*").header("if-match", "\"x\"").on(&app).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(call(Method::PUT, object(&id, A)).header("if-none-match", "\"x\"").on(&app).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(call(Method::PUT, object(&id, A)).header("if-match", "*").on(&app).await.0, StatusCode::BAD_REQUEST);
        assert!(!app.store.exists(id.parse().unwrap()));
        assert_eq!(call(Method::PUT, object(&id, A)).body(b"1234").on(&app).await.0, StatusCode::OK);
        let (status, _, body) = call(Method::PUT, object(&space(), A)).body(b"1").on(&app).await;
        assert_eq!((status, json_of(&body)), (StatusCode::INSUFFICIENT_STORAGE, serde_json::json!({ "error": "full", "limit": "spaces" })));
    }

    #[tokio::test]
    async fn a_listing_waits_for_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let app = std::sync::Arc::new(app(dir.path()));
        let id = space();
        call(Method::PUT, object(&id, A)).body(b"one").on(&app).await;
        let (_, headers, _) = call(Method::GET, devices(&id)).on(&app).await;
        let revision = headers[header::ETAG].to_str().unwrap().to_owned();
        // Nothing changes: 304 after the wait, which is held to the relay's longest (2 s here).
        let started = Instant::now();
        let (status, _, _) = call(Method::GET, format!("{}?wait=600", devices(&id))).header("if-none-match", revision.clone()).on(&app).await;
        assert_eq!(status, StatusCode::NOT_MODIFIED);
        assert!(started.elapsed() >= Duration::from_secs(2) && started.elapsed() < Duration::from_secs(10));
        // A write during the wait answers it at once, with the new listing.
        let waiter = {
            let (app, id, revision) = (std::sync::Arc::clone(&app), id.clone(), revision.clone());
            tokio::spawn(async move { call(Method::GET, format!("{}?wait=30", devices(&id))).header("if-none-match", revision).on(&app).await })
        };
        tokio::time::sleep(Duration::from_millis(200)).await;
        call(Method::PUT, object(&id, B)).body(b"two").on(&app).await;
        let (status, headers, body) = waiter.await.unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_ne!(headers[header::ETAG].to_str().unwrap(), revision);
        assert_eq!(json_of(&body)["objects"].as_array().unwrap().len(), 2);
        // A revision the client does not have yet: at once.
        let (status, _, _) = call(Method::GET, format!("{}?wait=30", devices(&id))).header("if-none-match", "\"old\"").on(&app).await;
        assert_eq!(status, StatusCode::OK);
        // A space the relay does not keep: the wait holds for an empty one, not for another.
        let started = Instant::now();
        let (status, _, _) = call(Method::GET, format!("{}?wait=1", devices(&space()))).header("if-none-match", empty_revision()).on(&app).await;
        assert!(status == StatusCode::NOT_MODIFIED && started.elapsed() >= Duration::from_secs(1));
        let (status, _, _) = call(Method::GET, format!("{}?wait=1", devices(&space()))).header("if-none-match", "\"x\"").on(&app).await;
        assert_eq!(status, StatusCode::OK);
        // Another token cannot wait on the space either.
        assert_eq!(
            call(Method::GET, format!("{}?wait=1", devices(&id))).header("if-none-match", "\"x\"").token(Some(token(9))).on(&app).await.0,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn rates_hold_per_client_per_space_and_for_new_spaces() {
        let dir = tempfile::tempdir().unwrap();
        let limits = Limits { requests_per_minute: 3, space_requests_per_minute: 1000, spaces_per_hour: 1, ..Limits::default() };
        let app = app_with(dir.path(), limits, &["10.0.0.0/8"]);
        let id = space();
        for _ in 0..3 {
            assert_eq!(call(Method::GET, devices(&id)).on(&app).await.0, StatusCode::OK);
        }
        let (status, headers, _) = call(Method::GET, devices(&id)).on(&app).await;
        assert_eq!((status, headers[header::RETRY_AFTER].to_str().unwrap()), (StatusCode::TOO_MANY_REQUESTS, RETRY_AFTER));
        // Another client, through the trusted proxy; the proxy itself is no client.
        let via = |client: &str| call(Method::PUT, object(&space(), A)).header("x-forwarded-for", format!("{client}, 10.0.0.9")).body(b"x");
        assert_eq!(via("198.51.100.1").from(&app, "10.1.2.3").await.0, StatusCode::OK);
        // Its second new space within the hour.
        assert_eq!(via("198.51.100.1").from(&app, "10.1.2.3").await.0, StatusCode::TOO_MANY_REQUESTS);
        // The header from an untrusted peer is not believed: that peer is the client.
        assert_eq!(via("198.51.100.2").from(&app, PEER).await.0, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(app.take_counts().1, 3);
        assert_eq!(app.take_counts(), (0, 0));
        app.sweep(Instant::now() + Duration::from_secs(3600));

        let per_space = app_with(dir.path(), Limits { space_requests_per_minute: 2, ..Limits::default() }, &[]);
        let id = space();
        assert_eq!(call(Method::GET, devices(&id)).from(&per_space, "198.51.100.3").await.0, StatusCode::OK);
        assert_eq!(call(Method::GET, devices(&id)).from(&per_space, "198.51.100.4").await.0, StatusCode::OK);
        assert_eq!(call(Method::GET, devices(&id)).from(&per_space, "198.51.100.5").await.0, StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn the_client_is_the_last_untrusted_hop() {
        let trusted = [IpNet::parse("10.0.0.0/8").unwrap()];
        let headers = |value: &str| {
            let mut map = HeaderMap::new();
            map.insert("x-forwarded-for", HeaderValue::from_str(value).unwrap());
            map
        };
        let peer: IpAddr = "10.0.0.1".parse().unwrap();
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert_eq!(client_ip(peer, &headers("198.51.100.1"), &trusted), ip("198.51.100.1"));
        // A client can put anything in front; the hop the proxy added is the last untrusted one.
        assert_eq!(client_ip(peer, &headers("1.1.1.1, 198.51.100.1, 10.0.0.2"), &trusted), ip("198.51.100.1"));
        assert_eq!(client_ip(peer, &headers("10.0.0.3"), &trusted), ip("10.0.0.3"));
        assert_eq!(client_ip(peer, &headers("not an address"), &trusted), peer);
        assert_eq!(client_ip(peer, &HeaderMap::new(), &trusted), peer);
        assert_eq!(client_ip(ip("198.51.100.9"), &headers("1.1.1.1"), &trusted), ip("198.51.100.9"));
        assert_eq!(client_ip(ip("::ffff:198.51.100.9"), &HeaderMap::new(), &[]), ip("198.51.100.9"));
    }

    #[tokio::test]
    async fn a_failing_disk_is_an_internal_error_and_a_gone_space_asks_again() {
        assert_eq!(store_error(StoreError::Io(std::io::Error::other("disk"))).status(), StatusCode::INTERNAL_SERVER_ERROR);
        let gone = store_error(StoreError::Gone);
        assert_eq!((gone.status(), gone.headers()[header::RETRY_AFTER].to_str().unwrap()), (StatusCode::SERVICE_UNAVAILABLE, "1"));
        assert_eq!(store_error(StoreError::TooLarge).status(), StatusCode::PAYLOAD_TOO_LARGE);
        let mut response = respond(StatusCode::OK, Bytes::new(), "text/plain");
        set_etag(&mut response, "bad\nvalue");
        assert!(response.headers().get(header::ETAG).is_none());
    }

    #[tokio::test]
    async fn a_body_that_fails_or_never_ends_is_refused() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        struct Failing;
        impl Body for Failing {
            type Data = Bytes;
            type Error = std::io::Error;
            fn poll_frame(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
                Poll::Ready(Some(Err(std::io::Error::other("cut"))))
            }
        }
        struct Endless;
        impl Body for Endless {
            type Data = Bytes;
            type Error = std::io::Error;
            fn poll_frame(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
                Poll::Pending
            }
        }
        fn request<B>(body: B) -> Request<B> {
            Request::builder().method(Method::PUT).body(body).unwrap()
        }
        assert_eq!(read_body(request(Failing), 10).await.unwrap_err().0, StatusCode::BAD_REQUEST);
        tokio::time::pause();
        assert_eq!(read_body(request(Endless), 10).await.unwrap_err().0, StatusCode::REQUEST_TIMEOUT);
    }
}

//! Accepting connections, serving them over HTTP/1.1, and the hourly upkeep (idle spaces, rates,
//! counts); stopping lets the requests in flight finish first.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, watch};
use tokio::task::{JoinHandle, JoinSet};

use crate::api::{self, App};
use crate::config::Config;
use crate::store::Store;

/// How long a client may take to send a request's head.
const HEADER_TIMEOUT: Duration = Duration::from_secs(20);
/// How long the requests in flight may take to finish when the relay stops.
const GRACE: Duration = Duration::from_secs(10);
/// How often idle spaces go and the rates are swept.
const UPKEEP_EVERY: Duration = Duration::from_secs(3600);

/// A relay that is serving.
pub struct Relay {
    addr: SocketAddr,
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl Relay {
    /// Read the spaces in, listen on `config.bind`, and serve until [`Relay::stop`].
    pub async fn start(config: Config) -> io::Result<Self> {
        let store = tokio::task::spawn_blocking({
            let (dir, limits) = (config.data_dir.clone(), config.limits);
            move || Store::open(&dir, limits)
        })
        .await
        .map_err(io::Error::other)??;
        let listener = TcpListener::bind(config.bind).await?;
        let addr = listener.local_addr()?;
        let app = Arc::new(App::new(store, &config));
        let (stop, stopped) = watch::channel(false);
        let task = tokio::spawn(serve(listener, app, stopped));
        Ok(Self { addr, stop, task })
    }

    /// Where it listens.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stop accepting, let the requests in flight finish (for [`GRACE`] at most), and return.
    pub async fn stop(self) {
        let _ = self.stop.send(true);
        let _ = self.task.await;
    }
}

/// Serve with `config` until `shutdown` completes.
pub async fn run(config: Config, shutdown: impl Future<Output = ()>) -> io::Result<()> {
    let relay = Relay::start(config).await?;
    tracing::info!(addr = %relay.addr(), version = api::VERSION, "lockra-relay is serving");
    shutdown.await;
    tracing::info!("lockra-relay is stopping");
    relay.stop().await;
    Ok(())
}

async fn serve(listener: TcpListener, app: Arc<App>, mut stopped: watch::Receiver<bool>) {
    let permits = Arc::new(Semaphore::new(app.limits.max_connections));
    let mut connections = JoinSet::new();
    let upkeep = tokio::spawn(upkeep(Arc::clone(&app), stopped.clone()));
    tracing::info!(spaces = app.store.spaces(), bytes = app.store.bytes(), "spaces read in");
    loop {
        tokio::select! {
            _ = stopped.changed() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    // At the limit, the connection is closed at once: the client retries later.
                    let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else { continue };
                    let (app, stopped) = (Arc::clone(&app), stopped.clone());
                    connections.spawn(async move {
                        connection(stream, peer, app, stopped).await;
                        drop(permit);
                    });
                }
                Err(error) => {
                    // Out of file descriptors, most likely: wait a moment rather than spin.
                    tracing::warn!(%error, "accepting a connection failed");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
        }
        while connections.try_join_next().is_some() {}
    }
    drop(listener);
    let _ = tokio::time::timeout(GRACE, async { while connections.join_next().await.is_some() {} }).await;
    connections.abort_all();
    let _ = upkeep.await;
}

/// One client connection: requests one after another, until it closes or the relay stops.
async fn connection(stream: TcpStream, peer: SocketAddr, app: Arc<App>, mut stopped: watch::Receiver<bool>) {
    let _ = stream.set_nodelay(true);
    let service = service_fn(move |request| {
        let app = Arc::clone(&app);
        async move { api::serve(&app, peer.ip(), request).await }
    });
    let mut builder = http1::Builder::new();
    builder.timer(TokioTimer::new()).header_read_timeout(HEADER_TIMEOUT);
    let connection = builder.serve_connection(TokioIo::new(stream), service);
    tokio::pin!(connection);
    tokio::select! {
        _ = connection.as_mut() => {}
        _ = stopped.changed() => {
            connection.as_mut().graceful_shutdown();
            let _ = connection.await;
        }
    }
}

/// Every hour: remove the idle spaces, forget the full rates, and log the counts (no client, no
/// space named).
async fn upkeep(app: Arc<App>, mut stopped: watch::Receiver<bool>) {
    let mut every = tokio::time::interval(UPKEEP_EVERY);
    every.tick().await;
    loop {
        tokio::select! {
            _ = stopped.changed() => return,
            _ = every.tick() => {}
        }
        let removed = app.store.expire(SystemTime::now()).await;
        app.sweep(Instant::now());
        let (served, limited) = app.take_counts();
        tracing::info!(spaces = app.store.spaces(), bytes = app.store.bytes(), removed, served, limited, "the last hour");
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    use super::*;
    use crate::config::Limits;

    fn config(dir: &std::path::Path) -> Config {
        Config { bind: "127.0.0.1:0".parse().unwrap(), data_dir: dir.to_owned(), trusted_proxies: Vec::new(), limits: Limits::default() }
    }

    async fn exchange(addr: SocketAddr, request: &str) -> String {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut answer = Vec::new();
        stream.read_to_end(&mut answer).await.unwrap();
        String::from_utf8_lossy(&answer).into_owned()
    }

    #[tokio::test]
    async fn it_serves_over_tcp_and_stops() {
        let dir = tempfile::tempdir().unwrap();
        let relay = Relay::start(config(dir.path())).await.unwrap();
        let addr = relay.addr();
        let answer = exchange(addr, "GET /healthz HTTP/1.1\r\nHost: relay\r\nConnection: close\r\n\r\n").await;
        assert!(answer.starts_with("HTTP/1.1 200 OK") && answer.ends_with("ok\n"), "{answer}");
        let id = uuid::Uuid::new_v4();
        let token = data_encoding::BASE64URL_NOPAD.encode(&[7; 32]);
        let put = format!(
            "PUT /v1/spaces/{id}/devices/0123456789abcdef0123456789abcdef.lks HTTP/1.1\r\nHost: relay\r\nAuthorization: Bearer {token}\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello"
        );
        assert!(exchange(addr, &put).await.starts_with("HTTP/1.1 200 OK"));
        relay.stop().await;
        assert!(TcpStream::connect(addr).await.is_err());
        // The space is there for the next start.
        let relay = Relay::start(config(dir.path())).await.unwrap();
        let get = format!("GET /v1/spaces/{id}/devices/ HTTP/1.1\r\nHost: relay\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n");
        assert!(exchange(relay.addr(), &get).await.contains("0123456789abcdef0123456789abcdef.lks"));
        relay.stop().await;
    }

    #[tokio::test]
    async fn run_serves_until_shutdown_and_a_taken_port_fails_the_start() {
        let dir = tempfile::tempdir().unwrap();
        let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let busy = Config { bind: taken.local_addr().unwrap(), ..config(dir.path()) };
        assert!(Relay::start(busy).await.is_err());
        run(config(dir.path()), async {}).await.unwrap();
        // A data directory that cannot be made.
        let file = dir.path().join("file");
        std::fs::write(&file, b"x").unwrap();
        assert!(Relay::start(config(&file)).await.is_err());
    }

    #[tokio::test]
    async fn the_connection_limit_closes_the_extra_ones() {
        let dir = tempfile::tempdir().unwrap();
        let relay = Relay::start(Config { limits: Limits { max_connections: 1, ..Limits::default() }, ..config(dir.path()) }).await.unwrap();
        // The first holds the only permit (its head never ends); the second is closed unanswered.
        let mut first = TcpStream::connect(relay.addr()).await.unwrap();
        first.write_all(b"GET /healthz HTTP/1.1\r\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut second = TcpStream::connect(relay.addr()).await.unwrap();
        let mut byte = [0u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(5), second.read(&mut byte)).await.unwrap();
        // Closed (or reset) without a byte of an answer.
        assert!(matches!(read, Ok(0) | Err(_)), "{read:?}");
        drop(first);
        relay.stop().await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_upkeep_runs_every_hour_until_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(dir.path());
        let app = Arc::new(App::new(Store::open(dir.path(), config.limits).unwrap(), &config));
        let (stop, stopped) = watch::channel(false);
        let task = tokio::spawn(upkeep(Arc::clone(&app), stopped));
        tokio::time::sleep(UPKEEP_EVERY * 2 + Duration::from_secs(1)).await;
        stop.send(true).unwrap();
        task.await.unwrap();
    }
}

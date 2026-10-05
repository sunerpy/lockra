//! Finding the hub on the local network: a probe (a preamble under the device's key) broadcast
//! and sent to the addresses the device knows, on the hub's port; only a hub that holds the key
//! answers, and its answer proves it does. An onlooker sees a new nonce and hint every time.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::{Instant, timeout_at};

use crate::Key;
use crate::wire::{ANSWER_LEN, Kind, Preamble};

/// How long a device waits for its hub's answer.
pub const DISCOVERY_WAIT: Duration = Duration::from_millis(1500);

/// The hub that answers the probe for `kind` under `key` on `port`, broadcast and sent to each of
/// `known`; `None` when none answers in [`DISCOVERY_WAIT`].
pub async fn discover(kind: Kind, key: &Key, port: u16, known: &[IpAddr], broadcast: bool) -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await.ok()?;
    let probe = Preamble::new(kind, key).ok()?;
    let bytes = probe.to_bytes();
    let mut targets: Vec<SocketAddr> = known.iter().filter(|ip| ip.is_ipv4()).map(|ip| SocketAddr::new(*ip, port)).collect();
    if broadcast && socket.set_broadcast(true).is_ok() {
        targets.push(SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), port));
    }
    for target in &targets {
        let _ = socket.send_to(&bytes, target).await;
    }
    let deadline = Instant::now() + DISCOVERY_WAIT;
    let mut answer = [0u8; 64];
    loop {
        // A target that refused the probe (Windows reports the ICMP answer as an error on the
        // next read) leaves the others to answer.
        let Ok((length, from)) = timeout_at(deadline, socket.recv_from(&mut answer)).await.ok()? else { continue };
        if length == ANSWER_LEN && probe.answered_by(&answer[..length], key) {
            return Some(from.ip());
        }
    }
}

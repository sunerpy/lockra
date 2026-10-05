//! The addresses a hub answers: this computer and the local network, nothing routed beyond it.

use std::net::{IpAddr, Ipv4Addr, UdpSocket};

/// This computer's address on the local network, as its route out picks it (no packet is sent:
/// a UDP socket only chooses its address when it connects), for a pairing offer. None when the
/// computer has no route, or the address it would use is not a local one.
pub fn local_addresses() -> Vec<IpAddr> {
    let chosen = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).and_then(|socket| {
        // TEST-NET-1 (RFC 5737): no one is there, and nothing is sent to it.
        socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9))?;
        socket.local_addr()
    });
    match chosen {
        Ok(addr) if local_address(addr.ip()) && !addr.ip().is_loopback() => vec![addr.ip()],
        _ => Vec::new(),
    }
}

/// A loopback, private or link-local address (an IPv4 address mapped into IPv6 counts as itself).
pub fn local_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => local_address(IpAddr::V4(v4)),
            None => v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_computer_and_the_local_network_only() {
        for local in
            ["127.0.0.1", "10.1.2.3", "172.16.0.1", "172.31.255.254", "192.168.1.20", "169.254.10.1", "::1", "fd12:3456::1", "fe80::1", "::ffff:192.168.1.20"]
        {
            assert!(local_address(local.parse().unwrap()), "{local}");
        }
        // This computer's own, when it has a route out, is one the hub answers.
        assert!(local_addresses().into_iter().all(|ip| local_address(ip) && !ip.is_loopback()));
        for routed in ["8.8.8.8", "172.32.0.1", "100.64.0.1", "203.0.113.5", "2001:db8::1", "::ffff:8.8.8.8", "0.0.0.0", "255.255.255.255"] {
            assert!(!local_address(routed.parse().unwrap()), "{routed}");
        }
    }
}

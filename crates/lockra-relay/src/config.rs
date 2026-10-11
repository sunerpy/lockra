//! What a relay listens on, where it keeps the spaces, and what a client may spend: flags first,
//! then `LOCKRA_RELAY_*` environment variables, then the defaults (`lockra-relay --help`).

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

/// The relay's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The address and port it listens on: behind a proxy that terminates TLS (the hosted relay's
    /// load balancer, a Caddy or nginx of one's own), or this computer only.
    pub bind: SocketAddr,
    /// Where the spaces are kept.
    pub data_dir: PathBuf,
    /// The proxies whose `X-Forwarded-For` names the client (for the per-client limits); a request
    /// from anywhere else is the client itself.
    pub trusted_proxies: Vec<IpNet>,
    /// What the clients may spend.
    pub limits: Limits,
}

/// What the clients may spend, so that one cannot exhaust the relay for the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The largest snapshot: far above an authenticator's accounts (a few kilobytes each 4 KiB
    /// block), at most the 16 MiB a device reads.
    pub max_object_bytes: u64,
    /// The snapshots of one space together.
    pub max_space_bytes: u64,
    /// Devices in one space.
    pub max_objects: usize,
    /// Everything the relay keeps.
    pub max_total_bytes: u64,
    /// Spaces the relay keeps.
    pub max_spaces: usize,
    /// A space no device has reached for this long is removed. Its devices still hold everything:
    /// the next run of any of them writes its snapshot again.
    pub idle: Duration,
    /// Requests per minute from one client address (an IPv6 /64 counts as one).
    pub requests_per_minute: u32,
    /// Requests per minute to one space.
    pub space_requests_per_minute: u32,
    /// New spaces per hour from one client address.
    pub spaces_per_hour: u32,
    /// The longest a listing waits for a change before it answers that nothing changed.
    pub max_wait: Duration,
    /// Connections served at once.
    pub max_connections: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_object_bytes: 4 * MIB,
            max_space_bytes: 32 * MIB,
            max_objects: 64,
            max_total_bytes: 4 * GIB,
            max_spaces: 100_000,
            idle: Duration::from_secs(400 * DAY),
            requests_per_minute: 120,
            space_requests_per_minute: 600,
            spaces_per_hour: 20,
            max_wait: Duration::from_secs(30),
            max_connections: 1024,
        }
    }
}

/// The largest snapshot any device reads (lockra-sync `MAX_OBJECT_BYTES`).
pub const MAX_OBJECT_BYTES_CEILING: u64 = 16 * MIB;

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const GIB: u64 = 1024 * MIB;
const DAY: u64 = 24 * 60 * 60;

/// An address range: `172.31.0.0/16`, `::1/128`, or an address alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpNet {
    addr: IpAddr,
    prefix: u8,
}

impl IpNet {
    /// The range from its text.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let (addr, prefix) = match text.split_once('/') {
            Some((addr, prefix)) => (addr, Some(prefix)),
            None => (text, None),
        };
        let addr: IpAddr = addr.parse().map_err(|_| format!("not an address range: {text}"))?;
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(prefix) => prefix.parse::<u8>().ok().filter(|p| *p <= max).ok_or_else(|| format!("not an address range: {text}"))?,
            None => max,
        };
        Ok(Self { addr, prefix })
    }

    /// Whether `ip` is in the range (an IPv4 address mapped into IPv6 counts as itself).
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, canonical(ip)) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => masked(u128::from(u32::from(net)), 32, self.prefix) == masked(u128::from(u32::from(ip)), 32, self.prefix),
            (IpAddr::V6(net), IpAddr::V6(ip)) => masked(u128::from(net), 128, self.prefix) == masked(u128::from(ip), 128, self.prefix),
            _ => false,
        }
    }
}

/// `bits` of `value` (a `width`-bit address) with all but the first `prefix` cleared.
fn masked(value: u128, width: u8, prefix: u8) -> u128 {
    if prefix == 0 { 0 } else { value >> (width - prefix) }
}

/// An IPv4 address mapped into IPv6 as the IPv4 address.
pub(crate) fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Serve with these settings.
    Run(Config),
    /// Print the help.
    Help,
    /// Print the version.
    Version,
}

/// The help text.
pub const HELP: &str = "\
lockra-relay: keeps Lockra's end-to-end encrypted sync spaces for devices without storage of their own.

Usage: lockra-relay [options]

Options (each also as an environment variable):
  --bind ADDR                   LOCKRA_RELAY_BIND                  listen here (127.0.0.1:8090)
  --data DIR                    LOCKRA_RELAY_DATA                  keep the spaces here (./lockra-relay-data)
  --trust-proxy RANGES          LOCKRA_RELAY_TRUST_PROXY           proxies whose X-Forwarded-For names the client,
                                                                   comma-separated (none)
  --max-object-bytes SIZE       LOCKRA_RELAY_MAX_OBJECT_BYTES      largest snapshot (4M, at most 16M)
  --max-space-bytes SIZE        LOCKRA_RELAY_MAX_SPACE_BYTES       one space's snapshots together (32M)
  --max-objects N               LOCKRA_RELAY_MAX_OBJECTS           devices in one space (64)
  --max-total-bytes SIZE        LOCKRA_RELAY_MAX_TOTAL_BYTES       everything kept (4G)
  --max-spaces N                LOCKRA_RELAY_MAX_SPACES            spaces kept (100000)
  --idle-days N                 LOCKRA_RELAY_IDLE_DAYS             remove a space unused this long (400)
  --requests-per-minute N       LOCKRA_RELAY_REQUESTS_PER_MINUTE   per client address (120)
  --space-requests-per-minute N LOCKRA_RELAY_SPACE_REQUESTS_PER_MINUTE  per space (600)
  --spaces-per-hour N           LOCKRA_RELAY_SPACES_PER_HOUR       new spaces per client address (20)
  --max-wait SECONDS            LOCKRA_RELAY_MAX_WAIT              longest wait for a change (30)
  --max-connections N           LOCKRA_RELAY_MAX_CONNECTIONS       connections served at once (1024)
  -h, --help                    print this help
  -V, --version                 print the version

Serve it behind HTTPS (a reverse proxy or a load balancer): Lockra connects to relays over HTTPS only.
SIZE is bytes, or a number with K, M or G (binary multiples).
Docs: https://firlab.app/lockra/backup/relay
";

/// The options, as `(flag, environment variable)`.
const OPTIONS: &[(&str, &str)] = &[
    ("--bind", "LOCKRA_RELAY_BIND"),
    ("--data", "LOCKRA_RELAY_DATA"),
    ("--trust-proxy", "LOCKRA_RELAY_TRUST_PROXY"),
    ("--max-object-bytes", "LOCKRA_RELAY_MAX_OBJECT_BYTES"),
    ("--max-space-bytes", "LOCKRA_RELAY_MAX_SPACE_BYTES"),
    ("--max-objects", "LOCKRA_RELAY_MAX_OBJECTS"),
    ("--max-total-bytes", "LOCKRA_RELAY_MAX_TOTAL_BYTES"),
    ("--max-spaces", "LOCKRA_RELAY_MAX_SPACES"),
    ("--idle-days", "LOCKRA_RELAY_IDLE_DAYS"),
    ("--requests-per-minute", "LOCKRA_RELAY_REQUESTS_PER_MINUTE"),
    ("--space-requests-per-minute", "LOCKRA_RELAY_SPACE_REQUESTS_PER_MINUTE"),
    ("--spaces-per-hour", "LOCKRA_RELAY_SPACES_PER_HOUR"),
    ("--max-wait", "LOCKRA_RELAY_MAX_WAIT"),
    ("--max-connections", "LOCKRA_RELAY_MAX_CONNECTIONS"),
];

impl Command {
    /// The command `args` (without the program's name) ask for, with `env` for what they leave out.
    pub fn parse(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let mut given: Vec<(&str, String)> = Vec::new();
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(Self::Help),
                "-V" | "--version" => return Ok(Self::Version),
                _ => {}
            }
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag, Some(value.to_owned())),
                None => (arg.as_str(), None),
            };
            let Some((flag, _)) = OPTIONS.iter().find(|(name, _)| *name == flag) else {
                return Err(format!("unknown option: {arg}"));
            };
            let value = match inline {
                Some(value) => value,
                None => args.next().cloned().ok_or_else(|| format!("{flag} needs a value"))?,
            };
            given.push((flag, value));
        }
        let value = |flag: &str| -> Option<String> {
            given.iter().rev().find(|(f, _)| *f == flag).map(|(_, v)| v.clone()).or_else(|| {
                let (_, var) = OPTIONS.iter().find(|(f, _)| *f == flag)?;
                env(var).filter(|v| !v.trim().is_empty())
            })
        };
        let defaults = Limits::default();
        let limits = Limits {
            max_object_bytes: size(value("--max-object-bytes"), "--max-object-bytes", defaults.max_object_bytes)?,
            max_space_bytes: size(value("--max-space-bytes"), "--max-space-bytes", defaults.max_space_bytes)?,
            max_objects: number(value("--max-objects"), "--max-objects", defaults.max_objects)?,
            max_total_bytes: size(value("--max-total-bytes"), "--max-total-bytes", defaults.max_total_bytes)?,
            max_spaces: number(value("--max-spaces"), "--max-spaces", defaults.max_spaces)?,
            idle: Duration::from_secs(number::<u64>(value("--idle-days"), "--idle-days", 400)?.saturating_mul(DAY)),
            requests_per_minute: number(value("--requests-per-minute"), "--requests-per-minute", defaults.requests_per_minute)?,
            space_requests_per_minute: number(value("--space-requests-per-minute"), "--space-requests-per-minute", defaults.space_requests_per_minute)?,
            spaces_per_hour: number(value("--spaces-per-hour"), "--spaces-per-hour", defaults.spaces_per_hour)?,
            max_wait: Duration::from_secs(number::<u64>(value("--max-wait"), "--max-wait", 30)?),
            max_connections: number(value("--max-connections"), "--max-connections", defaults.max_connections)?,
        };
        if limits.max_object_bytes > MAX_OBJECT_BYTES_CEILING {
            return Err("--max-object-bytes is at most 16M: a device reads no larger snapshot".into());
        }
        if limits.max_wait > Duration::from_secs(300) {
            return Err("--max-wait is at most 300 seconds".into());
        }
        let bind = value("--bind").unwrap_or_else(|| "127.0.0.1:8090".into());
        let bind = bind.trim().parse::<SocketAddr>().map_err(|_| format!("--bind is not an address and port: {bind}"))?;
        let trusted_proxies = value("--trust-proxy")
            .map(|ranges| ranges.split(',').filter(|r| !r.trim().is_empty()).map(IpNet::parse).collect::<Result<Vec<_>, _>>())
            .transpose()?
            .unwrap_or_default();
        let data_dir = PathBuf::from(value("--data").unwrap_or_else(|| "lockra-relay-data".into()));
        Ok(Self::Run(Config { bind, data_dir, trusted_proxies, limits }))
    }
}

/// A count or a size given as `text`, positive; `default` when not given.
fn number<T: std::str::FromStr + PartialOrd + Default>(text: Option<String>, flag: &str, default: T) -> Result<T, String> {
    match text {
        None => Ok(default),
        Some(text) => text.trim().parse::<T>().ok().filter(|n| *n > T::default()).ok_or_else(|| format!("{flag} is not a positive number: {text}")),
    }
}

/// A size in bytes: a number, or a number with `K`, `M` or `G` (`KiB`, `MiB`, `GiB` too).
fn size(text: Option<String>, flag: &str, default: u64) -> Result<u64, String> {
    let Some(text) = text else { return Ok(default) };
    let trimmed = text.trim().to_ascii_uppercase();
    let trimmed = trimmed.strip_suffix("IB").or_else(|| trimmed.strip_suffix('B')).unwrap_or(&trimmed);
    let (digits, unit) = match trimmed.char_indices().last() {
        Some((i, 'K')) => (&trimmed[..i], KIB),
        Some((i, 'M')) => (&trimmed[..i], MIB),
        Some((i, 'G')) => (&trimmed[..i], GIB),
        _ => (trimmed, 1),
    };
    digits.trim().parse::<u64>().ok().and_then(|n| n.checked_mul(unit)).filter(|n| *n > 0).ok_or_else(|| format!("{flag} is not a size: {text}"))
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;

    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn run(list: &[&str], env: &[(&str, &str)]) -> Result<Config, String> {
        let env: Vec<(String, String)> = env.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        match Command::parse(&args(list), &|name| env.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))? {
            Command::Run(config) => Ok(config),
            other => Err(format!("{other:?}")),
        }
    }

    #[test]
    fn the_defaults_listen_on_this_computer_only() {
        let config = run(&[], &[]).unwrap();
        assert_eq!(config.bind, "127.0.0.1:8090".parse().unwrap());
        assert_eq!(config.data_dir, PathBuf::from("lockra-relay-data"));
        assert!(config.trusted_proxies.is_empty());
        assert_eq!(config.limits, Limits::default());
        assert_eq!(config.limits.idle, Duration::from_secs(400 * DAY));
    }

    #[test]
    fn flags_win_over_the_environment_and_take_either_form() {
        let config = run(
            &["--bind", "0.0.0.0:9000", "--data=/var/lib/lockra-relay", "--max-object-bytes", "2M", "--max-objects=8", "--trust-proxy", "172.31.0.0/16, ::1"],
            &[("LOCKRA_RELAY_BIND", "127.0.0.1:1"), ("LOCKRA_RELAY_MAX_SPACES", "10"), ("LOCKRA_RELAY_IDLE_DAYS", "30"), ("LOCKRA_RELAY_MAX_WAIT", " ")],
        )
        .unwrap();
        assert_eq!(config.bind, "0.0.0.0:9000".parse().unwrap());
        assert_eq!(config.data_dir, PathBuf::from("/var/lib/lockra-relay"));
        assert_eq!(config.limits.max_object_bytes, 2 * MIB);
        assert_eq!(config.limits.max_objects, 8);
        assert_eq!(config.limits.max_spaces, 10);
        assert_eq!(config.limits.idle, Duration::from_secs(30 * DAY));
        // An empty variable is not given.
        assert_eq!(config.limits.max_wait, Duration::from_secs(30));
        assert_eq!(config.trusted_proxies.len(), 2);
        // The last of a repeated flag.
        assert_eq!(run(&["--max-objects", "3", "--max-objects", "4"], &[]).unwrap().limits.max_objects, 4);
    }

    #[test]
    fn sizes_take_binary_multiples() {
        for (text, bytes) in [("4096", 4096), ("4K", 4 * KIB), ("4kib", 4 * KIB), ("3M", 3 * MIB), ("3MB", 3 * MIB), ("1G", GIB), ("1GiB", GIB)] {
            assert_eq!(size(Some(text.into()), "--x", 1), Ok(bytes), "{text}");
        }
        for bad in ["", "0", "-1", "M", "1T", "lots", "99999999999999G"] {
            assert!(size(Some(bad.into()), "--x", 1).is_err(), "{bad}");
        }
    }

    #[test]
    fn bad_input_is_explained() {
        assert!(run(&["--port", "1"], &[]).unwrap_err().contains("unknown option"));
        assert!(run(&["--bind"], &[]).unwrap_err().contains("needs a value"));
        assert!(run(&["--bind", "localhost"], &[]).unwrap_err().contains("--bind"));
        assert!(run(&["--max-objects", "0"], &[]).unwrap_err().contains("positive"));
        assert!(run(&["--max-object-bytes", "17M"], &[]).unwrap_err().contains("16M"));
        assert!(run(&["--max-wait", "301"], &[]).unwrap_err().contains("300"));
        assert!(run(&["--trust-proxy", "10.0.0.0/33"], &[]).unwrap_err().contains("address range"));
        assert!(run(&["--trust-proxy", "proxy"], &[]).unwrap_err().contains("address range"));
        assert!(run(&["--trust-proxy", "10.0.0.0/x"], &[]).unwrap_err().contains("address range"));
    }

    #[test]
    fn help_and_version_stop_before_anything_else() {
        assert_eq!(Command::parse(&args(&["--bind", "x", "-h"]), &|_| None), Ok(Command::Help));
        assert_eq!(Command::parse(&args(&["--version"]), &|_| None), Ok(Command::Version));
        assert_eq!(Command::parse(&args(&["-V"]), &|_| None), Ok(Command::Version));
        assert!(HELP.contains("--trust-proxy") && OPTIONS.iter().all(|(flag, var)| HELP.contains(flag) && HELP.contains(var)));
    }

    #[test]
    fn address_ranges_hold_what_they_say() {
        let vpc = IpNet::parse("172.31.0.0/16").unwrap();
        assert!(vpc.contains("172.31.11.31".parse().unwrap()));
        assert!(!vpc.contains("172.32.0.1".parse().unwrap()));
        // IPv4 mapped into IPv6 is the IPv4 address.
        assert!(vpc.contains(IpAddr::V6(Ipv6Addr::from(0xffff_ac1f_0b1f_u128))));
        assert!(!vpc.contains("::1".parse().unwrap()));
        let single = IpNet::parse("203.0.113.7").unwrap();
        assert!(single.contains("203.0.113.7".parse().unwrap()) && !single.contains("203.0.113.8".parse().unwrap()));
        let v6 = IpNet::parse("2001:db8::/32").unwrap();
        assert!(v6.contains("2001:db8:1::1".parse().unwrap()) && !v6.contains("2001:db9::1".parse().unwrap()));
        assert!(IpNet::parse("0.0.0.0/0").unwrap().contains("8.8.8.8".parse().unwrap()));
        assert!(!IpNet::parse("::/0").unwrap().contains("8.8.8.8".parse().unwrap()));
    }
}

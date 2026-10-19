//! What a relay listens on, where it keeps the spaces, and what a client may spend: flags first,
//! then `LOCKRA_RELAY_*` environment variables, then the defaults (`lockra-relay --help`). The
//! commands for a running relay (`lockra-relay new-code`, …) are parsed here too.

use std::collections::BTreeMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use data_encoding::HEXLOWER;
use uuid::Uuid;

use crate::admin::AdminRequest;

/// The relay's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The address and port it listens on: behind a proxy that terminates TLS (a relay's load balancer, a Caddy or nginx of one's own), or this computer only.
    pub bind: SocketAddr,
    /// Where the spaces are kept.
    pub data_dir: PathBuf,
    /// The proxies whose `X-Forwarded-For` names the client (for the per-client limits); a request
    /// from anywhere else is the client itself.
    pub trusted_proxies: Vec<IpNet>,
    /// Which spaces may use the relay: open registration, exact space and access-hash pairs, or
    /// those and the spaces a registration code admitted.
    pub admission: Admission,
    /// What the clients may spend.
    pub limits: Limits,
}

/// Which sync spaces the relay serves. A closed relay compares the SHA-256 of the presented access
/// token with the configured hash; neither the token nor the sync key is configured here.
#[derive(Clone, PartialEq, Eq, Default)]
pub enum Admission {
    /// Any first write may create a space, as in the public relay protocol.
    #[default]
    Open,
    /// Only these exact `space id -> SHA-256(access token)` pairs may read, write or recreate a
    /// space. The map is non-empty when parsed from the command line.
    Closed(Box<BTreeMap<Uuid, [u8; 32]>>),
    /// The spaces listed here (as with `Closed`, possibly none) and those a registration code the
    /// relay made admitted (`lockra-relay new-code`), which `<data>/lockra-relay-v1/admission.json`
    /// keeps. A new space needs a code.
    Code(Box<BTreeMap<Uuid, [u8; 32]>>),
}

impl Admission {
    /// The spaces the settings list (none for open registration).
    pub(crate) fn listed(&self) -> &BTreeMap<Uuid, [u8; 32]> {
        static NONE: BTreeMap<Uuid, [u8; 32]> = BTreeMap::new();
        match self {
            Self::Open => &NONE,
            Self::Closed(spaces) | Self::Code(spaces) => spaces,
        }
    }

    /// The registration's name, as `--registration` takes it.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed(_) => "closed",
            Self::Code(_) => "code",
        }
    }
}

impl fmt::Debug for Admission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => f.write_str("Open"),
            Self::Closed(spaces) => write!(f, "Closed({} spaces)", spaces.len()),
            Self::Code(spaces) => write!(f, "Code({} spaces listed)", spaces.len()),
        }
    }
}

/// A command for a running relay, sent over its admin socket in its data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminCommand {
    /// The relay's data directory (`--data`, `LOCKRA_RELAY_DATA`).
    pub data_dir: PathBuf,
    /// What the relay is asked.
    pub request: AdminRequest,
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
    /// Ask the relay serving a data directory (`lockra-relay new-code`, …).
    Admin(AdminCommand),
    /// Print the help.
    Help,
    /// Print the version.
    Version,
}

/// The help text.
pub const HELP: &str = "\
lockra-relay: keeps Lockra's end-to-end encrypted sync spaces for devices without storage of their own.

Usage: lockra-relay [options]
       lockra-relay COMMAND [--data DIR] …    (a command for the relay serving DIR)

Options (each also as an environment variable):
  --bind ADDR                   LOCKRA_RELAY_BIND                  listen here (127.0.0.1:8090)
  --data DIR                    LOCKRA_RELAY_DATA                  keep the spaces here (./lockra-relay-data)
  --trust-proxy RANGES          LOCKRA_RELAY_TRUST_PROXY           proxies whose X-Forwarded-For names the client,
                                                                   comma-separated (none)
  --registration MODE           LOCKRA_RELAY_REGISTRATION          new spaces: open, code (each needs a
                                                                   registration code) or closed (open)
  --allowed-spaces LIST         LOCKRA_RELAY_ALLOWED_SPACES        with closed (or code) registration:
                                                                   comma-separated SPACE_ID=ACCESS_SHA256 pairs
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

Commands for the relay serving --data DIR (LOCKRA_RELAY_DATA), run as its user or root:
  new-code [--note TEXT] [--expires 7d]   a registration code: it admits one new space (code
                                          registration), until it expires (hours h or days d, at most 365d)
  codes                                   the codes not used yet
  drop-code ID                            cancel a code
  spaces                                  the spaces: admitted, and those on disk it does not serve
  admit SPACE_ID [--note TEXT]            serve a space on disk that no code admitted
  revoke SPACE_ID                         stop serving a space a code or admit admitted (its files stay)

Serve it behind HTTPS (a reverse proxy or a load balancer): Lockra connects to relays over HTTPS only.
SIZE is bytes, or a number with K, M or G (binary multiples).
Docs: https://firlab.app/lockra/backup/relay
";

/// The options, as `(flag, environment variable)`.
const OPTIONS: &[(&str, &str)] = &[
    ("--bind", "LOCKRA_RELAY_BIND"),
    ("--data", "LOCKRA_RELAY_DATA"),
    ("--trust-proxy", "LOCKRA_RELAY_TRUST_PROXY"),
    ("--registration", "LOCKRA_RELAY_REGISTRATION"),
    ("--allowed-spaces", "LOCKRA_RELAY_ALLOWED_SPACES"),
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

/// The commands for a running relay.
const ADMIN_COMMANDS: &[&str] = &["new-code", "codes", "drop-code", "spaces", "admit", "revoke"];

/// How long a code lasts unless `--expires` says otherwise, and at most.
const CODE_LIFETIME: Duration = Duration::from_secs(7 * DAY);
const MAX_CODE_LIFETIME: Duration = Duration::from_secs(365 * DAY);
/// A note on a code or a space: a few words for the administrator.
const MAX_NOTE_CHARS: usize = 200;

impl Command {
    /// The command `args` (without the program's name) ask for, with `env` for what they leave out.
    pub fn parse(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        if let Some(command) = args.first().filter(|first| ADMIN_COMMANDS.contains(&first.as_str())) {
            return admin_command(command, &args[1..], env);
        }
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
        let admission = admission(value("--registration"), value("--allowed-spaces"))?;
        let data_dir = PathBuf::from(value("--data").unwrap_or_else(|| "lockra-relay-data".into()));
        Ok(Self::Run(Config { bind, data_dir, trusted_proxies, admission, limits }))
    }
}

fn admission(registration: Option<String>, allowed: Option<String>) -> Result<Admission, String> {
    match registration.as_deref().map(str::trim).unwrap_or("open") {
        "open" if allowed.is_some() => Err("--allowed-spaces is only valid with --registration closed or code".into()),
        "open" => Ok(Admission::Open),
        "code" => Ok(Admission::Code(Box::new(allowed.as_deref().map(allowed_spaces).transpose()?.unwrap_or_default()))),
        "closed" => {
            let text = allowed.ok_or("--registration closed needs --allowed-spaces")?;
            let spaces = allowed_spaces(&text)?;
            if spaces.is_empty() {
                return Err("--registration closed needs at least one allowed space".into());
            }
            Ok(Admission::Closed(Box::new(spaces)))
        }
        other => Err(format!("--registration must be open, code or closed, not {other}")),
    }
}

/// `SPACE_ID=ACCESS_SHA256` pairs, comma-separated; an empty list is none.
fn allowed_spaces(text: &str) -> Result<BTreeMap<Uuid, [u8; 32]>, String> {
    let mut spaces = BTreeMap::new();
    for item in text.split(',').filter(|item| !item.trim().is_empty()) {
        let (id, hash) = item.trim().split_once('=').ok_or_else(|| format!("not SPACE_ID=ACCESS_SHA256 in --allowed-spaces: {item}"))?;
        let id = Uuid::try_parse(id)
            .ok()
            .filter(|parsed| parsed.hyphenated().to_string() == id)
            .ok_or_else(|| format!("not a canonical space id in --allowed-spaces: {id}"))?;
        let decoded = HEXLOWER
            .decode(hash.as_bytes())
            .ok()
            .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok())
            .ok_or_else(|| format!("not a lowercase 64-digit access SHA-256 in --allowed-spaces for {id}"))?;
        if spaces.insert(id, decoded).is_some() {
            return Err(format!("space {id} appears more than once in --allowed-spaces"));
        }
    }
    Ok(spaces)
}

/// A command for the relay serving the data directory: `command` and the arguments after it.
fn admin_command(command: &str, args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Command, String> {
    let mut data = None;
    let mut note = None;
    let mut expires = None;
    let mut positional = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if matches!(arg.as_str(), "-h" | "--help") {
            return Ok(Command::Help);
        }
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value.to_owned())),
            _ => (arg.as_str(), None),
        };
        let slot = match flag {
            "--data" => &mut data,
            "--note" if matches!(command, "new-code" | "admit") => &mut note,
            "--expires" if command == "new-code" => &mut expires,
            flag if flag.starts_with('-') => return Err(format!("{command} takes no option {flag}")),
            _ => {
                positional.push(arg.clone());
                continue;
            }
        };
        let value = match inline {
            Some(value) => value,
            None => args.next().cloned().ok_or_else(|| format!("{flag} needs a value"))?,
        };
        *slot = Some(value);
    }
    let note = note.map(|note| note.trim().to_owned()).unwrap_or_default();
    if note.chars().count() > MAX_NOTE_CHARS || note.chars().any(char::is_control) {
        return Err(format!("--note is at most {MAX_NOTE_CHARS} characters of one line"));
    }
    let wants_id = matches!(command, "drop-code" | "admit" | "revoke");
    match (wants_id, positional.len()) {
        (true, 1) | (false, 0) => {}
        (true, _) => return Err(format!("{command} takes one {}", if command == "drop-code" { "ID" } else { "SPACE_ID" })),
        (false, _) => return Err(format!("{command} takes no {}", positional.join(" "))),
    }
    let space = || crate::store::space_id(&positional[0]).ok_or_else(|| format!("not a space id (a lowercase UUID): {}", positional[0]));
    let request = match command {
        "new-code" => AdminRequest::NewCode { note, expires_secs: lifetime(expires.as_deref())?.as_secs() },
        "codes" => AdminRequest::Codes,
        "drop-code" => AdminRequest::DropCode { id: positional[0].trim().to_ascii_lowercase() },
        "spaces" => AdminRequest::Spaces,
        "admit" => AdminRequest::Admit { space: space()?, note },
        _ => AdminRequest::Revoke { space: space()? },
    };
    let data_dir = PathBuf::from(data.or_else(|| env("LOCKRA_RELAY_DATA").filter(|v| !v.trim().is_empty())).unwrap_or_else(|| "lockra-relay-data".into()));
    Ok(Command::Admin(AdminCommand { data_dir, request }))
}

/// A code's lifetime: `12h`, `7d`, or days alone; [`CODE_LIFETIME`] when not given.
fn lifetime(text: Option<&str>) -> Result<Duration, String> {
    let Some(text) = text.map(|t| t.trim().to_ascii_lowercase()) else { return Ok(CODE_LIFETIME) };
    let (digits, unit) = match text.strip_suffix('h') {
        Some(hours) => (hours, 3600),
        None => (text.strip_suffix('d').unwrap_or(&text), DAY),
    };
    let lifetime = digits.trim().parse::<u64>().ok().filter(|n| *n > 0).and_then(|n| n.checked_mul(unit)).map(Duration::from_secs);
    match lifetime {
        Some(lifetime) if lifetime <= MAX_CODE_LIFETIME => Ok(lifetime),
        Some(_) => Err("--expires is at most 365d".into()),
        None => Err(format!("--expires is hours (12h) or days (7d), not {text}")),
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
        assert_eq!(config.admission, Admission::Open);
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
    fn closed_registration_needs_an_exact_nonempty_allowlist() {
        let id = Uuid::parse_str("123e4567-e89b-42d3-a456-426614174000").unwrap();
        let first = format!("{id}={}", "11".repeat(32));
        let second_id = Uuid::parse_str("123e4567-e89b-42d3-a456-426614174001").unwrap();
        let both = format!("{first},{second_id}={}", "22".repeat(32));
        let config = run(&["--registration", "closed", "--allowed-spaces", &both], &[]).unwrap();
        assert_eq!(config.admission, Admission::Closed(Box::new(BTreeMap::from([(id, [0x11; 32]), (second_id, [0x22; 32])]))));
        assert_eq!(format!("{:?}", config.admission), "Closed(2 spaces)");
        assert_eq!(config.admission.name(), "closed");

        let from_env = run(&[], &[("LOCKRA_RELAY_REGISTRATION", "closed"), ("LOCKRA_RELAY_ALLOWED_SPACES", &first)]).unwrap();
        assert_eq!(from_env.admission.listed().keys().collect::<Vec<_>>(), [&id]);
        // A flag still wins over the environment.
        assert_eq!(
            run(&["--registration", "open"], &[("LOCKRA_RELAY_REGISTRATION", "closed"), ("LOCKRA_RELAY_ALLOWED_SPACES", &first)]).unwrap_err(),
            "--allowed-spaces is only valid with --registration closed or code"
        );

        assert_eq!(run(&["--registration", "closed"], &[]).unwrap_err(), "--registration closed needs --allowed-spaces");
        assert_eq!(run(&["--allowed-spaces", &first], &[]).unwrap_err(), "--allowed-spaces is only valid with --registration closed or code");
        assert!(run(&["--registration", "private", "--allowed-spaces", &first], &[]).unwrap_err().contains("open, code or closed"));
        for bad in [
            "",
            "not-a-space=1111111111111111111111111111111111111111111111111111111111111111",
            "123E4567-E89B-42D3-A456-426614174000=1111111111111111111111111111111111111111111111111111111111111111",
            "123e4567-e89b-42d3-a456-426614174000=short",
            "123e4567-e89b-42d3-a456-426614174000=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ] {
            assert!(run(&["--registration", "closed", "--allowed-spaces", bad], &[]).is_err(), "{bad}");
        }
        let duplicate = format!("{first},{first}");
        assert!(run(&["--registration", "closed", "--allowed-spaces", &duplicate], &[]).unwrap_err().contains("more than once"));
    }

    #[test]
    fn code_registration_takes_an_optional_list() {
        let id = Uuid::parse_str("123e4567-e89b-42d3-a456-426614174000").unwrap();
        let config = run(&["--registration", "code"], &[]).unwrap();
        assert_eq!(config.admission, Admission::Code(Box::default()));
        assert_eq!((format!("{:?}", config.admission), config.admission.name()), ("Code(0 spaces listed)".to_owned(), "code"));
        let listed = run(&[], &[("LOCKRA_RELAY_REGISTRATION", "code"), ("LOCKRA_RELAY_ALLOWED_SPACES", &format!("{id}={}", "11".repeat(32)))]).unwrap();
        assert_eq!(listed.admission, Admission::Code(Box::new(BTreeMap::from([(id, [0x11; 32])]))));
        assert!(run(&["--registration", "code", "--allowed-spaces", "bad"], &[]).is_err());
        assert_eq!((Admission::Open.name(), Admission::Open.listed().len()), ("open", 0));
    }

    fn admin(list: &[&str], env: &[(&str, &str)]) -> Result<AdminCommand, String> {
        let env: Vec<(String, String)> = env.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        match Command::parse(&args(list), &|name| env.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))? {
            Command::Admin(command) => Ok(command),
            other => Err(format!("{other:?}")),
        }
    }

    #[test]
    fn the_commands_for_a_running_relay_find_its_data_directory() {
        let space = "123e4567-e89b-42d3-a456-426614174000";
        let id = Uuid::parse_str(space).unwrap();
        let made = admin(&["new-code", "--note", " phone ", "--expires=12h", "--data", "/srv/relay"], &[("LOCKRA_RELAY_DATA", "/other")]).unwrap();
        assert_eq!(
            made,
            AdminCommand { data_dir: PathBuf::from("/srv/relay"), request: AdminRequest::NewCode { note: "phone".into(), expires_secs: 12 * 3600 } }
        );
        assert_eq!(
            admin(&["new-code"], &[("LOCKRA_RELAY_DATA", "/var/lib/lockra-relay")]).unwrap(),
            AdminCommand { data_dir: PathBuf::from("/var/lib/lockra-relay"), request: AdminRequest::NewCode { note: String::new(), expires_secs: 7 * DAY } }
        );
        assert_eq!(admin(&["codes"], &[]).unwrap(), AdminCommand { data_dir: PathBuf::from("lockra-relay-data"), request: AdminRequest::Codes });
        assert_eq!(admin(&["drop-code", "1A2B3C4D"], &[]).unwrap().request, AdminRequest::DropCode { id: "1a2b3c4d".into() });
        assert_eq!(admin(&["spaces"], &[]).unwrap().request, AdminRequest::Spaces);
        assert_eq!(admin(&["admit", space, "--note", "old laptop"], &[]).unwrap().request, AdminRequest::Admit { space: id, note: "old laptop".into() });
        assert_eq!(admin(&["revoke", space], &[]).unwrap().request, AdminRequest::Revoke { space: id });
        assert_eq!(admin(&["new-code", "--expires", "30"], &[]).unwrap().request, AdminRequest::NewCode { note: String::new(), expires_secs: 30 * DAY });
        assert_eq!(Command::parse(&args(&["codes", "--help"]), &|_| None), Ok(Command::Help));

        for (bad, why) in [
            (&["new-code", "--expires", "366d"][..], "at most 365d"),
            (&["new-code", "--expires", "0"], "hours (12h) or days (7d)"),
            (&["new-code", "--expires", "soon"], "hours (12h) or days (7d)"),
            (&["new-code", "--expires"], "needs a value"),
            (&["new-code", "extra"], "takes no extra"),
            (&["codes", "--note", "x"], "takes no option --note"),
            (&["revoke"], "takes one SPACE_ID"),
            (&["drop-code"], "takes one ID"),
            (&["admit", "123E4567-E89B-42D3-A456-426614174000"], "not a space id"),
            (&["spaces", "--bind", "x"], "takes no option --bind"),
        ] {
            let error = admin(bad, &[]).unwrap_err();
            assert!(error.contains(why), "{bad:?}: {error}");
        }
        let long = "x".repeat(201);
        assert!(admin(&["new-code", "--note", &long], &[]).unwrap_err().contains("at most 200"));
        assert!(admin(&["new-code", "--note", "two\nlines"], &[]).unwrap_err().contains("one line"));
        assert!(HELP.contains("new-code") && ADMIN_COMMANDS.iter().all(|command| HELP.contains(command)));
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

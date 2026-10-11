# The Lockra relay

`lockra-relay` (crates/lockra-relay) keeps sync spaces for devices that have no storage of their
own. Lockra runs one, the built-in relay at `https://lockra-relay.onethinker.top`; anyone can run
another from the same binary. For a device it is one more kind of storage, beside an S3 bucket, a
WebDAV folder and a cloud drive's folder (docs/formats.md §9): the same end-to-end encrypted
snapshots, one per device, each written by its device only.

The relay opens none of them. The data key that seals the snapshots never leaves the devices, and
neither does the sync key: a space's devices show the relay an access token derived from the sync
key, and it keeps a hash of that token. What it can and cannot learn is in docs/security.md, "Sync"
and "The relay".

## Running one

Lockra connects to a relay over HTTPS only (plain HTTP only to the same computer, for tests). The
relay itself speaks plain HTTP/1.1: put it behind something that ends TLS, a reverse proxy or a load
balancer, and tell it which proxies to believe about the client's address (`--trust-proxy`).

### The release binary

Every release has a static Linux binary for x64 and ARM64,
`lockra-relay_<version>_linux_<x64|arm64>.tar.gz` (the binary, its systemd unit and the licence), in
`SHA256SUMS` and the release's attestations:

```sh
version=0.8.1 arch=x64
gh release download "v$version" --repo sunerpy/lockra --pattern "lockra-relay_${version}_linux_${arch}.tar.gz" --pattern SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
gh attestation verify "lockra-relay_${version}_linux_${arch}.tar.gz" --repo sunerpy/lockra
tar -xzf "lockra-relay_${version}_linux_${arch}.tar.gz"
sudo install -m 0755 lockra-relay /usr/local/bin/lockra-relay
sudo cp lockra-relay.service /etc/systemd/system/ && sudo systemctl enable --now lockra-relay
```

The unit (`deploy/relay/lockra-relay.service`) runs it under systemd as a dynamic user, its spaces
in `/var/lib/lockra-relay`, listening on `127.0.0.1:8090` for a proxy on the same machine. Behind a
load balancer on another machine, change its `LOCKRA_RELAY_BIND` to `0.0.0.0:8090` and its
`LOCKRA_RELAY_TRUST_PROXY` to the balancer's addresses, and let only the balancer reach the port.

### Docker, with HTTPS

`deploy/relay/compose.yaml` builds the relay's image and puts Caddy in front of it, which gets a
certificate for your domain from Let's Encrypt. Point the domain's DNS at the machine, open ports 80
and 443, then from the repository's root:

```sh
RELAY_HOST=relay.example.com docker compose -f deploy/relay/compose.yaml up -d
curl https://relay.example.com/healthz   # ok
```

The image alone: `docker build -f deploy/relay/Dockerfile -t lockra-relay .`; it listens on 8090
and keeps the spaces in the volume at `/data`.

### From the source

`cargo build --release -p lockra-relay` (Rust 1.98); the binary is `target/release/lockra-relay`.

## Settings

Each flag has an environment variable; a flag wins over the variable, and either over the default.

| Flag                          | Variable                                 | Default               | What it is                                                                               |
| ----------------------------- | ---------------------------------------- | --------------------- | ---------------------------------------------------------------------------------------- |
| `--bind`                      | `LOCKRA_RELAY_BIND`                      | `127.0.0.1:8090`      | The address and port it listens on.                                                      |
| `--data`                      | `LOCKRA_RELAY_DATA`                      | `./lockra-relay-data` | Where the spaces are kept.                                                               |
| `--trust-proxy`               | `LOCKRA_RELAY_TRUST_PROXY`               | none                  | Proxies (addresses or ranges, comma-separated) whose `X-Forwarded-For` names the client. |
| `--max-object-bytes`          | `LOCKRA_RELAY_MAX_OBJECT_BYTES`          | `4M`                  | The largest snapshot (at most `16M`, the largest a device reads).                        |
| `--max-space-bytes`           | `LOCKRA_RELAY_MAX_SPACE_BYTES`           | `32M`                 | One space's snapshots together.                                                          |
| `--max-objects`               | `LOCKRA_RELAY_MAX_OBJECTS`               | `64`                  | Devices in one space.                                                                    |
| `--max-total-bytes`           | `LOCKRA_RELAY_MAX_TOTAL_BYTES`           | `4G`                  | Everything the relay keeps.                                                              |
| `--max-spaces`                | `LOCKRA_RELAY_MAX_SPACES`                | `100000`              | Spaces the relay keeps.                                                                  |
| `--idle-days`                 | `LOCKRA_RELAY_IDLE_DAYS`                 | `400`                 | A space no device reached for this long is removed.                                      |
| `--requests-per-minute`       | `LOCKRA_RELAY_REQUESTS_PER_MINUTE`       | `120`                 | Per client address (an IPv6 /64 counts as one).                                          |
| `--space-requests-per-minute` | `LOCKRA_RELAY_SPACE_REQUESTS_PER_MINUTE` | `600`                 | Per space.                                                                               |
| `--spaces-per-hour`           | `LOCKRA_RELAY_SPACES_PER_HOUR`           | `20`                  | New spaces per client address.                                                           |
| `--max-wait`                  | `LOCKRA_RELAY_MAX_WAIT`                  | `30`                  | The longest, in seconds, a listing waits for a change.                                   |
| `--max-connections`           | `LOCKRA_RELAY_MAX_CONNECTIONS`           | `1024`                | Connections served at once; more are closed at once.                                     |

Sizes are bytes, or a number with `K`, `M` or `G` (binary multiples). `lockra-relay --help` prints
the same list. `RUST_LOG` sets the log level (`info` by default).

With `--trust-proxy`, the client is the last address in `X-Forwarded-For` that is not one of the
proxies; a request from anywhere else is its own client. A load balancer that appends to the header
(AWS's does) works as it is; a proxy should replace the header rather than append to it when it is
the only one in front (deploy/relay/Caddyfile does).

## What it keeps

```
<data>/lockra-relay-v1/spaces/<space id>/access               SHA-256 of the access token's 32 bytes, hex
<data>/lockra-relay-v1/spaces/<space id>/devices/<tag>.lks    a device's snapshot, as the device sealed it
```

A space is made by its first write, bound to the token that wrote it; a request with another token
is refused (403). The relay keeps nothing but snapshots: names other than a device tag and `.lks`
are refused. Each write goes to a temporary file that takes the snapshot's name once it is on disk,
so a reader gets the old snapshot or the new one. The relay reads the directory in when it starts
and keeps an index in memory; an idle space (`--idle-days`) is removed with its files. Its devices
lose nothing by that: each of them holds the whole space, and the next run of any writes its
snapshot again.

The relay logs when it starts and stops, an hourly count (spaces, bytes, requests, refusals, spaces
removed), and failures of its disk. It logs no client address, no space id and no request.

Backing up `<data>` keeps the spaces available after a lost disk; the files are as encrypted as the
devices made them. To upgrade, stop the relay, replace the binary and start it again; the requests
in flight finish first (10 seconds at most).

## The API

The relay's HTTP API is in docs/formats.md §9, "The relay": a listing, reads, writes with
conditions, removals, and a listing that waits for a change, all under the space's access token.
`GET /healthz` answers `ok`, and `GET /v1/` names the service, the API version and the relay's
version.

## The built-in relay

Lockra's own relay answers at `https://lockra-relay.onethinker.top`: the release binary as a systemd
service on a server in AWS's Seoul region, behind an application load balancer that ends TLS and
keeps no access or connection logs. It runs with the defaults above. Whoever runs a relay can see
what docs/security.md says a relay sees, and can make it unavailable; for a space that must not
depend on Lockra's server, run a relay of your own, or use storage of your own.

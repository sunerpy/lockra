# Running your own relay

This page explains how to run a Lockra relay of your own, so that your devices sync through a
server you control rather than Lockra's built-in relay.

<StatusTag status="available" /> Available from version 0.8.1.

A relay keeps a sync space's encrypted files and tells the waiting devices when one of them writes
([Through a Lockra relay](/backup/sync#through-a-lockra-relay)). It cannot open the files: the key
that encrypts them never leaves the devices. A relay of your own means that nobody else sees when
your devices sync or from where, and that your space does not depend on Lockra's server.

## What you need

- A Linux server, x64 or ARM64, that your devices can reach. A small virtual machine is enough.
- A domain name for it, and HTTPS in front of the relay: Lockra connects to a relay over `https://`
  only. The relay itself speaks plain HTTP, so a reverse proxy or a load balancer in front of it
  ends HTTPS. Caddy does this with a free certificate, below.

## With Docker and Caddy

The repository's `deploy/relay/compose.yaml` builds the relay and runs it with Caddy in front of
it; Caddy gets a certificate for your domain from Let's Encrypt.

1. Point the domain's DNS at the server, and open ports 80 and 443 to it.
2. On the server, start both from the repository's root:

   ```sh
   git clone --depth 1 https://github.com/sunerpy/lockra && cd lockra
   RELAY_HOST=relay.example.com docker compose -f deploy/relay/compose.yaml up -d
   curl https://relay.example.com/healthz   # ok
   ```

The spaces are kept in the Docker volume `relay-data`.

## With the release package

Every release carries the relay for Linux, `lockra-relay_<version>_linux_x64.tar.gz` and
`lockra-relay_<version>_linux_arm64.tar.gz`, listed in the release's `SHA256SUMS`. Each holds the
program, which needs no other library, and a systemd service for it.

1. Download the package for your server, check it and install the program:

   ```sh
   version=0.8.1 arch=x64
   base="https://github.com/sunerpy/lockra/releases/download/v$version"
   curl -fL -O "$base/lockra-relay_${version}_linux_${arch}.tar.gz" -O "$base/SHA256SUMS"
   sha256sum --check --ignore-missing SHA256SUMS
   tar -xzf "lockra-relay_${version}_linux_${arch}.tar.gz"
   sudo install -m 0755 lockra-relay /usr/local/bin/lockra-relay
   ```

2. Install the service and start it. It runs the relay as an unprivileged user, keeps the spaces
   in `/var/lib/lockra-relay`, and listens on `127.0.0.1:8090` for a proxy on the same server:

   ```sh
   sudo cp lockra-relay.service /etc/systemd/system/
   sudo systemctl enable --now lockra-relay
   ```

3. Put HTTPS in front of it. With Caddy on the same server, this `Caddyfile` gets the certificate
   and passes the requests on:

   ```text
   relay.example.com {
   	reverse_proxy 127.0.0.1:8090 {
   		header_up X-Forwarded-For {remote_host}
   	}
   }
   ```

With `gh`, `gh attestation verify <package> --repo sunerpy/lockra` also proves that the package was
built by the release workflow from Lockra's source.

## Behind a load balancer

When a load balancer of a cloud provider ends HTTPS, let the relay listen on the network, and name
the balancer's addresses so that its limits count each device's address rather than the
balancer's:

```sh
lockra-relay --bind 0.0.0.0:8090 --trust-proxy 10.0.0.0/16 --data /var/lib/lockra-relay
```

Allow port 8090 from the balancer only, and use `/healthz` as its health check. In the service,
the same settings are the lines `LOCKRA_RELAY_BIND` and `LOCKRA_RELAY_TRUST_PROXY`.

## Connecting your devices

On the first device, open **Settings › Sync**, choose **Start syncing**, keep **Lockra relay**,
choose **A relay of your own**, and enter the relay's address in **Relay address**, for example
`https://relay.example.com`. The invitation carries the address, so the other devices join by
scanning it, as with the built-in relay. A space that already syncs elsewhere moves to your relay
with **Change storage settings** on each device.

## Settings

The defaults suit a household or a small team: a device's file up to 4 MiB, a space up to 64
devices and 32 MiB, 4 GiB in all, 120 requests a minute from each network address, and a space
removed 400 days after a device last reached it. `lockra-relay --help` lists every setting, each
also as an environment variable;
[the relay's documentation](https://github.com/sunerpy/lockra/blob/main/docs/relay.md) explains
them.

## What it keeps

- **Encrypted files and nothing else.** One folder per space, with each device's encrypted file
  and a fingerprint of the value the space's devices identify themselves with. No account, no
  device name, no recovery key and no master password reach the relay.
- **Logs without addresses.** The relay logs when it starts and stops, an hourly count of spaces,
  bytes and requests, and failures of its disk. It logs no network address, no space and no
  request; your proxy logs what you configure it to.
- **Backups and upgrades.** Backing up the data folder keeps the spaces available after a lost
  disk; the files there are as encrypted as the devices made them. Without a backup, the devices
  still hold every account, and write the space again on their next sync. To upgrade, install the
  new program and restart the service.

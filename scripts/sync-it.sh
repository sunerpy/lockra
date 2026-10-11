#!/usr/bin/env bash
# The sync storage against real servers (crates/lockra-remote/tests/live.rs): the Versity S3 gateway
# (conditional writes as AWS has them) and rclone's WebDAV server, in Docker, on free ports of
# 127.0.0.1, with a throwaway password; both containers are removed afterwards. MinIO no longer
# publishes community images (2025), so it is not used. Every wait is a condition with a deadline.
# Usage: scripts/sync-it.sh   (make sync-it)
set -euo pipefail
cd "$(dirname "$0")/.."
for tool in docker curl cargo; do
  command -v "$tool" >/dev/null || { echo "sync-it: $tool not installed"; exit 2; }
done
s3_image=${LOCKRA_IT_S3_IMAGE:-versity/versitygw:v1.8.0}
rclone_image=${LOCKRA_IT_RCLONE_IMAGE:-rclone/rclone:1.69}

free_port() {
  for p in $(seq "$1" "$2"); do
    if ! (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null; then echo "$p"; return; fi
  done
  echo "sync-it: no free port in $1-$2" >&2
  exit 2
}
s3_port=$(free_port 19000 19099)
dav_port=$(free_port 19100 19199)
user=lockra-it
password=$(od -An -N18 -tx1 /dev/urandom | tr -d ' \n')
s3=lockra-it-s3-$$
webdav=lockra-it-webdav-$$
cleanup() {
  docker rm -f "$s3" "$webdav" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# The posix backend treats every directory under its root as a bucket.
docker run -d --name "$s3" -p "127.0.0.1:$s3_port:7070" -e ROOT_ACCESS_KEY_ID="$user" -e ROOT_SECRET_ACCESS_KEY="$password" \
  --entrypoint sh "$s3_image" -c 'mkdir -p /gw/lockra-it && exec /usr/local/bin/versitygw posix /gw' >/dev/null
docker run -d --name "$webdav" -p "127.0.0.1:$dav_port:8080" "$rclone_image" \
  serve webdav /data --addr :8080 --user "$user" --pass "$password" --etag-hash auto >/dev/null
timeout 60 sh -c "until [ \"\$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:$s3_port/)\" != 000 ]; do sleep 0.5; done" ||
  { docker logs "$s3" | tail -n 20; exit 1; }
timeout 60 sh -c "until [ \"\$(curl -s -o /dev/null -w '%{http_code}' -u $user:$password -X PROPFIND -H 'Depth: 0' http://127.0.0.1:$dav_port/)\" = 207 ]; do sleep 0.5; done" ||
  { docker logs "$webdav" | tail -n 20; exit 1; }
echo "sync-it: S3 on 127.0.0.1:$s3_port, WebDAV on 127.0.0.1:$dav_port"

LOCKRA_IT_REQUIRED=1 \
  LOCKRA_IT_S3_ENDPOINT="http://127.0.0.1:$s3_port" LOCKRA_IT_S3_BUCKET=lockra-it \
  LOCKRA_IT_S3_ACCESS_KEY="$user" LOCKRA_IT_S3_SECRET_KEY="$password" \
  LOCKRA_IT_WEBDAV_URL="http://127.0.0.1:$dav_port/" LOCKRA_IT_WEBDAV_USER="$user" LOCKRA_IT_WEBDAV_PASSWORD="$password" \
  cargo test --locked -p lockra-remote --test live -- --test-threads 2
echo "sync-it: OK"

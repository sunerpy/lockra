# lockra-relay: keeps Lockra's end-to-end encrypted sync spaces and opens none of them (docs/relay.md).
# Build from the repository's root: docker build -f deploy/relay/Dockerfile -t lockra-relay .
FROM rust:1.98-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p lockra-relay && mkdir -p /data

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/lockra-relay /usr/local/bin/lockra-relay
# An empty named volume takes this directory's owner: the relay's user can write it.
COPY --from=build --chown=65532:65532 /data /data
ENV LOCKRA_RELAY_BIND=0.0.0.0:8090 LOCKRA_RELAY_DATA=/data
VOLUME /data
EXPOSE 8090
USER nonroot
ENTRYPOINT ["/usr/local/bin/lockra-relay"]

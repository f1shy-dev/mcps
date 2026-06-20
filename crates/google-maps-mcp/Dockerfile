FROM rust:1.95-alpine AS build

WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY crates/mcp-shared/Cargo.toml crates/mcp-shared/Cargo.toml
COPY crates/google-maps-mcp/Cargo.toml crates/google-maps-mcp/Cargo.toml
COPY crates/ssh-mcp/Cargo.toml crates/ssh-mcp/Cargo.toml
RUN mkdir -p crates/mcp-shared/src crates/google-maps-mcp/src crates/ssh-mcp/src && \
    printf '' > crates/mcp-shared/src/lib.rs && \
    printf 'fn main() {}' > crates/google-maps-mcp/src/main.rs && \
    printf 'fn main() {}' > crates/ssh-mcp/src/main.rs
RUN apk add --no-cache build-base && cargo fetch --locked

COPY crates/mcp-shared/src crates/mcp-shared/src
COPY crates/google-maps-mcp/src crates/google-maps-mcp/src
RUN cargo build -p google-maps-mcp --release && strip /src/target/release/google-maps-mcp

FROM alpine:3.22

RUN apk add --no-cache ca-certificates
COPY --from=build /src/target/release/google-maps-mcp /usr/local/bin/google-maps-mcp

ENV HOME=/tmp
USER 65534:65534
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/google-maps-mcp"]

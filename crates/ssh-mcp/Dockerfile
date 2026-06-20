FROM rust:1.95-alpine AS build

WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY crates/mcp-shared/Cargo.toml crates/mcp-shared/Cargo.toml
COPY crates/ssh-mcp/Cargo.toml crates/ssh-mcp/Cargo.toml
RUN mkdir -p crates/mcp-shared/src crates/ssh-mcp/src && \
    printf '' > crates/mcp-shared/src/lib.rs && \
    printf 'fn main() {}' > crates/ssh-mcp/src/main.rs
RUN cargo fetch --locked

COPY crates/mcp-shared/src crates/mcp-shared/src
COPY crates/ssh-mcp/src crates/ssh-mcp/src
RUN cargo build -p ssh-mcp --release && strip /src/target/release/ssh-mcp

FROM alpine:3.22

RUN apk add --no-cache ca-certificates openssh-client-default

COPY --from=build /src/target/release/ssh-mcp /usr/local/bin/ssh-mcp

USER 65534:65534
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/ssh-mcp"]

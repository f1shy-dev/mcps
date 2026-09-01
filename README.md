# 🌸 MCPs

Small, independent MCP servers.

| MCP | Status | Notes |
| --- | --- | --- |
| [`ssh-mcp`](crates/ssh-mcp) | working | Restricted SSH-over-MCP command runner. |
| [`google-maps-mcp`](crates/google-maps-mcp) | working | Google Maps Platform tools with local SQLite budget gating and caching. |

Shared:

- [`mcp-shared`](crates/mcp-shared): JSON-RPC, Streamable HTTP, auth, and typed tool schema helpers.

## Commands

```bash
cargo build -p ssh-mcp --release
SSH_MCP_CONFIG=crates/ssh-mcp/config.example.toml cargo run -p ssh-mcp
docker build -f crates/ssh-mcp/Dockerfile -t ssh-mcp:local .

cargo build -p google-maps-mcp --release
GOOGLE_MAPS_API_KEY=... cargo run -p google-maps-mcp
docker build -f crates/google-maps-mcp/Dockerfile -t google-maps-mcp:local .
```

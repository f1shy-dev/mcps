# 🌸 MCPs

Small, independent MCP servers.

| MCP | Status | Notes |
| --- | --- | --- |
| [`ssh-mcp`](crates/ssh-mcp) | working | Restricted SSH-over-MCP command runner. |

Shared:

- [`mcp-shared`](crates/mcp-shared): JSON-RPC and Streamable HTTP helpers.

## Commands

```bash
cargo build -p ssh-mcp --release
SSH_MCP_CONFIG=crates/ssh-mcp/config.example.toml cargo run -p ssh-mcp
docker build -f crates/ssh-mcp/Dockerfile -t ssh-mcp:local .
```

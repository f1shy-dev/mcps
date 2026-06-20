# 🌸 MCPs

Small, independent MCP servers. Each crate builds and runs as its own service.

| MCP | Status | Notes |
| --- | --- | --- |
| [`ssh-mcp`](crates/ssh-mcp) | working | Restricted SSH-over-MCP command runner. |
| `obsidian-vfs-mcp` | planned | Obsidian vault VFS MCP. |

Shared:

- [`mcp-shared`](crates/mcp-shared): JSON-RPC and Streamable HTTP helpers.

## Commands

```bash
cargo build -p ssh-mcp --release
SSH_MCP_CONFIG=crates/ssh-mcp/config.example.toml cargo run -p ssh-mcp
docker build -f crates/ssh-mcp/Dockerfile -t ssh-mcp:local .
```

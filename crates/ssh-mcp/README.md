# SSH MCP

`ssh-mcp` is an independent Streamable HTTP MCP server for running non-interactive SSH commands against a fixed server-side target allowlist.

## Security Model

- The caller can only pass `target`, `command`, and `timeout_seconds`.
- Target host, port, user, private key path, and known-hosts path are server-side config.
- Password and keyboard-interactive authentication are disabled.
- Agent forwarding, TTY allocation, and SSH forwarding are disabled.
- Output is capped.
- Command length and timeout are capped.

## Tools

### `ssh_targets`

Lists configured targets without exposing private key paths.

### `ssh_run`

Runs a command on one configured target:

```json
{
  "target": "example",
  "command": "hostname",
  "timeout_seconds": 60
}
```

Response:

```json
{
  "target": "example",
  "exit_code": 0,
  "timed_out": false,
  "stdout": "example\n",
  "stderr": "",
  "stdout_truncated": false,
  "stderr_truncated": false
}
```

## Local Run

```bash
SSH_MCP_CONFIG=crates/ssh-mcp/config.example.toml cargo run -p ssh-mcp
```

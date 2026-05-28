# IRC MCP Server

Standalone IRC MCP server for rusty-bidule agent integration.

## Features

- Connect to IRC networks with optional TLS/SSL support
- Join channels and send/receive messages
- DCC file transfer support (auto-accept mode)
- **Automatic zip extraction** - Downloaded zip files are automatically extracted to subdirectories with security protections (path traversal prevention, file count limits)
- Message history storage with full-text search
- MCP streamable_http interface (JSON-RPC 2.0)
- Persistent SQLite database

## Installation

```bash
cargo build --release
```

## Configuration

Edit `irc-mcp-config.yaml`:

```yaml
server:
  address: "irc.undernet.org"  # IRC server hostname
  port: 6667                    # IRC server port
  use_tls: false                # Enable TLS/SSL

identity:
  nickname: "rusty-bot"         # Bot nickname
  username: "rusty"             # Username
  realname: "Rusty Bidule IRC Bot"  # Real name

channels:
  - "#bookz"                    # Auto-join channels

dcc:
  enabled: true
  download_directory: "./data/irc-downloads"
  max_file_size_bytes: 104857600  # 100 MB
  auto_accept: true
  allowed_extensions: []        # Empty = allow all

storage:
  database_path: "./data/irc-history.db"
  message_retention_days: 90

mcp:
  listen_address: "127.0.0.1"
  port: 5001
```

## Running

```bash
cargo run -- --config irc-mcp-config.yaml
```

Or with release build:

```bash
./target/release/irc-mcp-server --config irc-mcp-config.yaml
```

## Testing

Run unit and integration tests:

```bash
cargo test
```

Manual MCP protocol testing:

```bash
./test-irc-mcp.sh
```

## Integration with rusty-bidule

Add to rusty-bidule's `config/config.local.yaml`:

```yaml
mcp_servers:
  - name: irc-server
    transport: streamable_http
    url: http://127.0.0.1:5001/mcp
    timeout: 30
    client_session_timeout_seconds: 300
```

Enable network permissions in rusty-bidule TUI:

```
/permissions network on
```

## MCP Tools

### Connection Management
- **irc_connect** - Connect to IRC server
- **irc_disconnect** - Disconnect from server
- **irc_status** - Get connection status

### Channel Operations
- **irc_join_channel** - Join a channel
- **irc_part_channel** - Leave a channel
- **irc_send_message** - Send message to channel/user
- **irc_get_messages** - Retrieve message history
- **irc_get_channel_users** - List channel users

### DCC Operations
- **irc_list_dcc_transfers** - List file transfers (includes extracted_files array for zips)
- **irc_get_dcc_file_info** - Get transfer details (includes extraction_status, extraction_error, extracted_files)
- **irc_read_dcc_file** - Read file content

### Utility
- **irc_send_raw** - Send raw IRC command
- **irc_search_history** - Full-text search

## Example Usage

Connect to IRC and join #bookz:

```bash
curl -X POST http://127.0.0.1:5001/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"irc_connect","arguments":{}}}'

curl -X POST http://127.0.0.1:5001/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"irc_join_channel","arguments":{"channel":"#bookz"}}}'
```

Get recent messages:

```bash
curl -X POST http://127.0.0.1:5001/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"irc_get_messages","arguments":{"target":"#bookz","limit":10}}}'
```

## Architecture

```
┌─────────────┐        HTTP/MCP         ┌──────────────┐
│ rusty-bidule│◄──────────────────────►│ IRC MCP      │
│   Agent     │    port 5001            │ Server       │
└─────────────┘                         │              │
                                        │ ┌──────────┐ │
                                        │ │ Axum     │ │
                                        │ │ Server   │ │
                                        │ └────┬─────┘ │
                                        │      │       │
                                        │ ┌────▼────┐  │
                                        │ │IRC Client│  │
                                        │ │(irc crate│  │
                                        │ └────┬────┘  │
                                        │      │       │
                                        │ ┌────▼────┐  │
                                        │ │ SQLite  │  │
                                        │ │ Storage │  │
                                        │ └─────────┘  │
                                        └──────┬───────┘
                                               │
                                        ┌──────▼───────┐
                                        │ IRC Network  │
                                        │ (Undernet)   │
                                        └──────────────┘
```

## Security

- Server binds to 127.0.0.1 by default (localhost only)
- DCC filenames sanitized to prevent directory traversal
- File size limits enforced
- Optional file extension filtering

## Troubleshooting

**Server won't start:**
- Check config file syntax: `yamllint irc-mcp-config.yaml`
- Ensure port 5001 is not in use: `lsof -i :5001`

**Can't connect to IRC:**
- Check server address and port
- Verify network connectivity
- Try with TLS disabled first

**DCC transfers failing:**
- Check download directory permissions
- Verify file size limits
- Check firewall rules for incoming connections

## License

Part of the rusty-bidule project.

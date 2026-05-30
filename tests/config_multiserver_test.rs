// tests/config_multiserver_test.rs
use irc_mcp_server::config::IrcMcpConfig;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_parse_multiserver_config() {
    let config_content = r##"
servers:
  - name: "server1"
    address: "irc.example.org"
    port: 6667
    use_tls: false
    identity:
      nickname: "bot1"
      username: "bot"
      realname: "Test Bot"
    channels:
      - "#test"
    dcc:
      enabled: true
      download_directory: "./data/server1"
      max_file_size_bytes: 1048576
      auto_accept: false
      allowed_extensions: []

storage:
  database_path: "./test.db"
  message_retention_days: 30
  cleanup_interval_hours: 12

mcp:
  listen_address: "127.0.0.1"
  port: 5001
  default_server: "server1"
"##;

    let mut file = NamedTempFile::new().unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    let config = IrcMcpConfig::from_file(file.path()).unwrap();
    assert_eq!(config.servers.len(), 1);
    assert_eq!(config.servers[0].name, "server1");
    assert_eq!(config.servers[0].address, "irc.example.org");
    assert_eq!(config.servers[0].identity.nickname, "bot1");
    assert_eq!(config.mcp.default_server, "server1");
    assert_eq!(config.storage.cleanup_interval_hours, 12);
}

#[test]
fn test_parse_config_with_sasl() {
    let config_content = r##"
servers:
  - name: "libera"
    address: "irc.libera.chat"
    port: 6697
    use_tls: true
    password: "my_password"
    sasl:
      enabled: true
      username: "myaccount"
    identity:
      nickname: "testbot"
      username: "test"
      realname: "Test"
    channels: []
    dcc:
      enabled: false

storage:
  database_path: "./test.db"
  message_retention_days: 90
  cleanup_interval_hours: 24

mcp:
  listen_address: "127.0.0.1"
  port: 5001
  default_server: "libera"
"##;

    let mut file = NamedTempFile::new().unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    let config = IrcMcpConfig::from_file(file.path()).unwrap();
    assert_eq!(config.servers[0].password, Some("my_password".to_string()));
    assert_eq!(config.servers[0].sasl.enabled, true);
    assert_eq!(
        config.servers[0].sasl.username,
        Some("myaccount".to_string())
    );
}

#[test]
fn test_validate_duplicate_server_names() {
    let config_content = r##"
servers:
  - name: "server1"
    address: "irc.example.org"
    port: 6667
    use_tls: false
    identity:
      nickname: "bot1"
      username: "bot"
      realname: "Test"
    channels: []
    dcc:
      enabled: false
  - name: "server1"
    address: "irc.other.org"
    port: 6667
    use_tls: false
    identity:
      nickname: "bot2"
      username: "bot"
      realname: "Test"
    channels: []
    dcc:
      enabled: false

storage:
  database_path: "./test.db"
  message_retention_days: 90
  cleanup_interval_hours: 24

mcp:
  listen_address: "127.0.0.1"
  port: 5001
  default_server: "server1"
"##;

    let mut file = NamedTempFile::new().unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    let result = IrcMcpConfig::from_file(file.path());
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Duplicate server name"));
}

#[test]
fn test_validate_invalid_default_server() {
    let config_content = r##"
servers:
  - name: "server1"
    address: "irc.example.org"
    port: 6667
    use_tls: false
    identity:
      nickname: "bot1"
      username: "bot"
      realname: "Test"
    channels: []
    dcc:
      enabled: false

storage:
  database_path: "./test.db"
  message_retention_days: 90
  cleanup_interval_hours: 24

mcp:
  listen_address: "127.0.0.1"
  port: 5001
  default_server: "nonexistent"
"##;

    let mut file = NamedTempFile::new().unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    let result = IrcMcpConfig::from_file(file.path());
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("default_server 'nonexistent' not found"));
}

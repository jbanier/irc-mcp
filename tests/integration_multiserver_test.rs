// tests/integration_multiserver_test.rs
use irc_mcp_server::config::IrcMcpConfig;
use std::io::Write;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_multiserver_config_loads() {
    let config_content = r#"
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
  - name: "server2"
    address: "irc.example2.org"
    port: 6697
    use_tls: true
    password: "testpass"
    sasl:
      enabled: true
      username: "testuser"
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
"#;

    let mut file = NamedTempFile::new().unwrap();
    file.write_all(config_content.as_bytes()).unwrap();

    let config = IrcMcpConfig::from_file(file.path()).unwrap();

    assert_eq!(config.servers.len(), 2);
    assert_eq!(config.servers[0].name, "server1");
    assert_eq!(config.servers[1].name, "server2");
    assert_eq!(config.servers[1].sasl.enabled, true);
    assert_eq!(config.mcp.default_server, "server1");
}

use irc_mcp_server::config::IrcMcpConfig;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_load_valid_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test_config.yaml");

    let yaml_content = "server:\n  address: \"irc.test.org\"\n  port: 6667\n  use_tls: false\n\nidentity:\n  nickname: \"testbot\"\n  username: \"test\"\n  realname: \"Test Bot\"\n\nchannels:\n  - \"#test\"\n\ndcc:\n  enabled: true\n  download_directory: \"./downloads\"\n  max_file_size_bytes: 10485760\n  auto_accept: true\n  allowed_extensions: []\n\nstorage:\n  database_path: \"./test.db\"\n  message_retention_days: 30\n\nmcp:\n  listen_address: \"127.0.0.1\"\n  port: 5001\n";

    fs::write(&config_path, yaml_content).unwrap();

    let config = IrcMcpConfig::from_file(&config_path).unwrap();
    assert_eq!(config.server.address, "irc.test.org");
    assert_eq!(config.server.port, 6667);
    assert_eq!(config.identity.nickname, "testbot");
    assert_eq!(config.channels.len(), 1);
    assert_eq!(config.dcc.enabled, true);
}

#[test]
fn test_missing_required_field_fails() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test_config_invalid.yaml");

    let yaml_content = "server:\n  address: \"irc.test.org\"\n  # missing port\n";

    fs::write(&config_path, yaml_content).unwrap();

    let result = IrcMcpConfig::from_file(&config_path);
    assert!(result.is_err());
}

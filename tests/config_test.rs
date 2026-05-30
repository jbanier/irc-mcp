use irc_mcp_server::config::IrcMcpConfig;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_load_valid_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test_config.yaml");

    let yaml_content = "servers:\n  - name: \"testserver\"\n    address: \"irc.test.org\"\n    port: 6667\n    use_tls: false\n    identity:\n      nickname: \"testbot\"\n      username: \"test\"\n      realname: \"Test Bot\"\n    channels:\n      - \"#test\"\n    dcc:\n      enabled: true\n      download_directory: \"./downloads\"\n      max_file_size_bytes: 10485760\n      auto_accept: true\n      allowed_extensions: []\n\nstorage:\n  database_path: \"./test.db\"\n  message_retention_days: 30\n  cleanup_interval_hours: 24\n\nmcp:\n  listen_address: \"127.0.0.1\"\n  port: 5001\n  default_server: \"testserver\"\n";

    fs::write(&config_path, yaml_content).unwrap();

    let config = IrcMcpConfig::from_file(&config_path).unwrap();
    assert_eq!(config.servers.len(), 1);
    assert_eq!(config.servers[0].name, "testserver");
    assert_eq!(config.servers[0].address, "irc.test.org");
    assert_eq!(config.servers[0].port, 6667);
    assert_eq!(config.servers[0].identity.nickname, "testbot");
    assert_eq!(config.servers[0].channels.len(), 1);
    assert_eq!(config.servers[0].dcc.enabled, true);
}

#[test]
fn test_missing_required_field_fails() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test_config_invalid.yaml");

    let yaml_content =
        "servers:\n  - name: \"testserver\"\n    address: \"irc.test.org\"\n    # missing port\n";

    fs::write(&config_path, yaml_content).unwrap();

    let result = IrcMcpConfig::from_file(&config_path);
    assert!(result.is_err());
}

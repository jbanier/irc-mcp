use base64::{engine::general_purpose::STANDARD, Engine};

/// Encode credentials for SASL PLAIN mechanism
/// Format: base64(username\0username\0password)
pub fn encode_sasl_plain(username: &str, password: &str) -> String {
    let auth_string = format!("{}\0{}\0{}", username, username, password);
    STANDARD.encode(auth_string.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_basic() {
        let result = encode_sasl_plain("alice", "secret");
        assert!(!result.is_empty());
        assert!(result
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '=' || c == '+' || c == '/'));
    }

    #[test]
    fn test_encode_empty_password() {
        let result = encode_sasl_plain("user", "");
        assert!(!result.is_empty());
    }
}

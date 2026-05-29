use irc_mcp_server::irc::sasl::encode_sasl_plain;

#[test]
fn test_sasl_plain_encoding() {
    let encoded = encode_sasl_plain("myuser", "mypass");
    let expected = "bXl1c2VyAG15dXNlcgBteXBhc3M="; // base64("\0myuser\0mypass")
    assert_eq!(encoded, expected);
}

#[test]
fn test_sasl_plain_encoding_special_chars() {
    let encoded = encode_sasl_plain("user@host", "p@ss!123");
    let expected = "dXNlckBob3N0AHVzZXJAaG9zdABwQHNzITEyMw==";
    assert_eq!(encoded, expected);
}

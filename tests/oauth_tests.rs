use omaplayer::{
    base64url_encode, extract_query_param, generate_code_challenge, generate_code_verifier,
    percent_decode, sha256,
};

#[test]
fn test_sha256_known_vectors() {
    // Test vector: empty string
    let h_empty = sha256(b"");
    let hex_empty = h_empty.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    assert_eq!(
        hex_empty,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );

    // Test vector: "abc"
    let h_abc = sha256(b"abc");
    let hex_abc = h_abc.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    assert_eq!(
        hex_abc,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );

    // Test vector: longer string
    let h_long = sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
    let hex_long = h_long.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    assert_eq!(
        hex_long,
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn test_base64url_encoding() {
    assert_eq!(base64url_encode(b""), "");
    assert_eq!(base64url_encode(b"f"), "Zg");
    assert_eq!(base64url_encode(b"fo"), "Zm8");
    assert_eq!(base64url_encode(b"foo"), "Zm9v");
    assert_eq!(base64url_encode(b"foob"), "Zm9vYg");
    assert_eq!(base64url_encode(b"fooba"), "Zm9vYmE");
    assert_eq!(base64url_encode(b"foobar"), "Zm9vYmFy");

    // Check URL-safe characters '-' and '_' instead of '+' and '/'
    assert_eq!(base64url_encode(&[0xfb, 0xff, 0xfe]), "-__-");

    // SHA256 of "abc" encoded with Base64URL
    let h = sha256(b"abc");
    let b64 = base64url_encode(&h);
    assert_eq!(b64, "ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0");
    assert_eq!(b64.len(), 43);
    assert!(!b64.contains('='));
    assert!(!b64.contains('+'));
    assert!(!b64.contains('/'));
}

#[test]
fn test_pkce_generation() {
    let verifier = generate_code_verifier();
    // PKCE RFC 7636 allows 43 to 128 chars
    assert!(verifier.len() >= 43 && verifier.len() <= 128);
    for c in verifier.chars() {
        assert!(
            c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~',
            "Invalid PKCE character: {}",
            c
        );
    }

    let challenge = generate_code_challenge(&verifier);
    assert_eq!(challenge.len(), 43);
    for c in challenge.chars() {
        assert!(
            c.is_ascii_alphanumeric() || c == '-' || c == '_',
            "Invalid Challenge character: {}",
            c
        );
    }
}

#[test]
fn test_query_param_parser() {
    let req = "GET /callback?code=NApCCgBkWtQ123&state=test_state_987 HTTP/1.1";
    assert_eq!(
        extract_query_param(req, "code"),
        Some("NApCCgBkWtQ123".to_string())
    );
    assert_eq!(
        extract_query_param(req, "state"),
        Some("test_state_987".to_string())
    );
    assert_eq!(extract_query_param(req, "error"), None);

    let err_req = "GET /callback?error=access_denied&state=xyz HTTP/1.1";
    assert_eq!(
        extract_query_param(err_req, "error"),
        Some("access_denied".to_string())
    );
    assert_eq!(extract_query_param(err_req, "code"), None);

    let pct_req = "GET /callback?name=Ozan%20%C3%96zdil HTTP/1.1";
    assert_eq!(
        extract_query_param(pct_req, "name"),
        Some("Ozan Özdil".to_string())
    );
}

#[test]
fn test_percent_decode() {
    assert_eq!(percent_decode("hello+world"), "hello world");
    assert_eq!(percent_decode("hello%20world"), "hello world");
    assert_eq!(percent_decode("%C3%96zdil"), "Özdil");
}

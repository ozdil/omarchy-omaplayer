use omaplayer::{sanitize_terminal_str, strip_ansi};

#[test]
fn test_osc52_clipboard_injection() {
    let payload = "\x1b]52;c;ZXhpdCAw\x07Malicious Track";
    let cleaned = sanitize_terminal_str(payload, 64, 256);
    assert!(!cleaned.contains("\x1b]52"));
    assert!(!cleaned.contains("ZXhpdCAw"));
    assert_eq!(cleaned, "Malicious Track");
}

#[test]
fn test_osc8_hyperlink_injection() {
    let payload = "\x1b]8;;https://evil.attacker.com/exploit\x1b\\Click for Free Premium\x1b]8;;\x1b\\";
    let cleaned = sanitize_terminal_str(payload, 64, 256);
    assert!(!cleaned.contains("evil.attacker.com"));
    assert!(!cleaned.contains("\x1b]8"));
    assert_eq!(cleaned, "Click for Free Premium");
}

#[test]
fn test_csi_cursor_and_screen_clear_injection() {
    let payload = "\x1b[2J\x1b[H\x1b[?25lEvil Track Title\x1b[10;20H";
    let cleaned = sanitize_terminal_str(payload, 64, 256);
    assert!(!cleaned.contains("\x1b["));
    assert_eq!(cleaned, "Evil Track Title");
}

#[test]
fn test_bidi_override_injection() {
    let payload = "Normal Song \u{202e}ReverseTitle\u{202c} • \u{200e}\u{200f}\u{061c}\u{2066}\u{2067}\u{2068}\u{2069}Artist";
    let cleaned = sanitize_terminal_str(payload, 64, 256);
    for bidi_char in ['\u{202e}', '\u{202c}', '\u{200e}', '\u{200f}', '\u{061c}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}'] {
        assert!(!cleaned.contains(bidi_char));
    }
    assert_eq!(cleaned, "Normal Song ReverseTitle • Artist");
}

#[test]
fn test_c0_c1_del_control_characters() {
    let payload = "Track\x00\x07\x08\x0b\x0c\x0e\x0f\x1b\x7fName";
    let cleaned = sanitize_terminal_str(payload, 64, 256);
    for bad_code in 0..32u8 {
        assert!(!cleaned.contains(bad_code as char));
    }
    assert!(!cleaned.contains('\x7f'));
    assert_eq!(cleaned, "TrackName");
}

#[test]
fn test_markup_and_delimiter_filtering() {
    let payload = "<script>alert('pwn')</script> & \"quoted\" `backticks` \\backslash\\";
    let cleaned = sanitize_terminal_str(payload, 64, 256);
    for char_del in ['<', '>', '&', '\'', '"', '`', '\\'] {
        assert!(!cleaned.contains(char_del));
    }
}

#[test]
fn test_strict_byte_and_char_bounding() {
    let huge_payload = "A".repeat(70000);
    let cleaned = sanitize_terminal_str(&huge_payload, 40, 256);
    assert_eq!(cleaned.len(), 40);
    assert_eq!(cleaned, "A".repeat(40));
}

#[test]
fn test_strip_ansi() {
    let raw = "\x1b[1;32mGreen Text\x1b[0m \x1b]8;;http://foo.bar\x07Link\x1b]8;;\x07";
    let plain = strip_ansi(raw);
    assert_eq!(plain, "Green Text Link");
}

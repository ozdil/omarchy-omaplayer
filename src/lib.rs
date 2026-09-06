use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const MAX_BYTE_LIMIT: usize = 65536;

pub struct RadioStream {
    pub name: &'static str,
    pub url: &'static str,
    pub genre: &'static str,
}

pub fn get_radio_streams() -> HashMap<&'static str, RadioStream> {
    let mut map = HashMap::new();
    map.insert(
        "lofi",
        RadioStream {
            name: "Lofi Beats (Sakin / Kodlama)",
            url: "https://stream.zeno.fm/f3wvbbqmdg8uv",
            genre: "Lofi & Chill",
        },
    );
    map.insert(
        "jazz",
        RadioStream {
            name: "Jazz Radio Classics",
            url: "https://jazz-wr01.ice.infomaniak.ch/jazz-wr01-128.mp3",
            genre: "Jazz & Soul",
        },
    );
    map.insert(
        "synthwave",
        RadioStream {
            name: "Nightwave Plaza (Synthwave)",
            url: "https://plaza.one/mp3",
            genre: "Synthwave & Retrowave",
        },
    );
    map.insert(
        "rock",
        RadioStream {
            name: "Classic Rock Radio",
            url: "https://ais-sa2.cdnstream1.com/1988_128.mp3",
            genre: "Rock & Metal",
        },
    );
    map.insert(
        "trt",
        RadioStream {
            name: "TRT Radyo 3 (Klasik & Senfoni)",
            url: "https://radio-trtradyo3.live.trt.com.tr/master.m3u8",
            genre: "Classical & Culture",
        },
    );
    map
}

// ==============================================================================
// 1. BULLETPROOF TERMINAL SANITIZATION & STRIP ANSI
// ==============================================================================

/// Strips all ANSI / VT escape sequences (CSI, OSC, DCS, APC, PM, SOS, 2-char escapes).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    let n = bytes.len();

    while i < n {
        if bytes[i] == 0x1B {
            i += 1;
            if i >= n {
                break;
            }
            match bytes[i] {
                b'[' => {
                    // CSI sequence: ESC [ [0-?]* [ -/]* [@-~]
                    i += 1;
                    while i < n && (0x30..=0x3F).contains(&bytes[i]) {
                        i += 1;
                    }
                    while i < n && (0x20..=0x2F).contains(&bytes[i]) {
                        i += 1;
                    }
                    if i < n && (0x40..=0x7E).contains(&bytes[i]) {
                        i += 1;
                    }
                }
                b']' => {
                    // OSC sequence: ESC ] ... (BEL (0x07) or ST (ESC \))
                    i += 1;
                    while i < n {
                        if bytes[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1B && i + 1 < n && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                b'P' | b'X' | b'^' | b'_' => {
                    // DCS, SOS, PM, APC: terminated by ST (ESC \)
                    i += 1;
                    while i < n {
                        if bytes[i] == 0x1B && i + 1 < n && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    // 2-character escape sequence (e.g. ESC N, ESC O, ESC M)
                    i += 1;
                }
            }
        } else {
            // Valid UTF-8 character boundary copy
            let ch_len = match bytes[i] {
                0..=0x7F => 1,
                0xC0..=0xDF => 2,
                0xE0..=0xEF => 3,
                0xF0..=0xF7 => 4,
                _ => 1,
            };
            if i + ch_len <= n {
                if let Ok(valid_str) = std::str::from_utf8(&bytes[i..i + ch_len]) {
                    out.push_str(valid_str);
                }
            }
            i += ch_len;
        }
    }
    out
}

/// Strict terminal string sanitizer:
/// 1. Bounds raw input bytes to `max_bytes` without tearing UTF-8 characters.
/// 2. Strips all ANSI/VT escape sequences (OSC clipboard/hyperlink, CSI cursor, etc.).
/// 3. Removes HTML/XML tags.
/// 4. Removes C0 controls (0x00..=0x1F), C1 controls (0x80..=0x9F), DEL (0x7F), and ESC (0x1B).
/// 5. Removes Unicode Bidirectional controls (U+061C, U+200E, U+200F, U+202A..=U+202E, U+2066..=U+2069).
/// 6. Removes markup delimiters (`<`, `>`, `&`, `'`, `"`, '`', `\`).
/// 7. Collapses excessive whitespace and enforces character count `max_len`.
pub fn sanitize_terminal_str(s: &str, max_len: usize, max_bytes: usize) -> String {
    if s.is_empty() {
        return String::new();
    }

    // 1. Bound byte length safely at a valid UTF-8 boundary
    let mut bounded_str = s;
    if bounded_str.len() > max_bytes {
        let mut end = max_bytes;
        while end > 0 && !bounded_str.is_char_boundary(end) {
            end -= 1;
        }
        bounded_str = &bounded_str[..end];
    }

    // 2. Strip ANSI escape sequences
    let without_ansi = strip_ansi(bounded_str);

    // 3. Strip HTML/XML tags
    let mut without_tags = String::with_capacity(without_ansi.len());
    let mut in_tag = false;
    for ch in without_ansi.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' && in_tag {
            in_tag = false;
        } else if !in_tag {
            without_tags.push(ch);
        }
    }

    // 4, 5, 6. Filter control codes, bidi overrides, delimiters
    let mut cleaned = String::with_capacity(without_tags.len());
    let mut last_was_space = true; // trims leading spaces automatically

    for ch in without_tags.chars() {
        // Filter C0/C1 controls, DEL, ESC
        let cp = ch as u32;
        if cp <= 0x1F || cp == 0x7F || (0x80..=0x9F).contains(&cp) {
            continue;
        }

        // Filter Unicode Bidi controls
        if cp == 0x061C
            || cp == 0x200E
            || cp == 0x200F
            || (0x202A..=0x202E).contains(&cp)
            || (0x2066..=0x2069).contains(&cp)
        {
            continue;
        }

        // Filter markup delimiters: < > & ' " ` \
        if matches!(ch, '<' | '>' | '&' | '\'' | '"' | '`' | '\\') {
            continue;
        }

        if ch.is_whitespace() {
            if !last_was_space {
                cleaned.push(' ');
                last_was_space = true;
            }
        } else {
            cleaned.push(ch);
            last_was_space = false;
        }
    }

    // Trim trailing whitespace
    if cleaned.ends_with(' ') {
        cleaned.pop();
    }

    // 7. Enforce max_len character truncation
    cleaned.chars().take(max_len).collect()
}

// ==============================================================================
// 2. PATHS, STATE, & AUTH MANAGEMENT
// ==============================================================================

pub fn get_runtime_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        let p = PathBuf::from(xdg);
        if p.is_dir() {
            return p;
        }
    }
    let uid = unsafe { libc::getuid() };
    let run_user = PathBuf::from(format!("/run/user/{}", uid));
    if run_user.is_dir() {
        return run_user;
    }
    if let Some(home) = dirs_home() {
        let fallback = home.join(".local/state/omaplayer");
        let _ = fs::create_dir_all(&fallback);
        if let Ok(meta) = fs::metadata(&fallback) {
            let mut perms = meta.permissions();
            perms.set_mode(0o700);
            let _ = fs::set_permissions(&fallback, perms);
        }
        return fallback;
    }
    PathBuf::from("/tmp")
}

pub fn get_socket_path() -> PathBuf {
    get_runtime_dir().join("omaplayer_mpv.sock")
}

pub fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub fn get_state_dir() -> PathBuf {
    let p = if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(xdg).join("omarchy/omaplayer")
    } else {
        dirs_home()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local/share/omarchy/omaplayer")
    };
    let _ = fs::create_dir_all(&p);
    if let Ok(meta) = fs::metadata(&p) {
        let mut perms = meta.permissions();
        perms.set_mode(0o700);
        let _ = fs::set_permissions(&p, perms);
    }
    p
}

pub fn get_state_file() -> PathBuf {
    get_state_dir().join("state.json")
}

pub fn get_auth_file() -> PathBuf {
    get_state_dir().join("auth.json")
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedState {
    pub active_source: String,
    pub current_title: String,
    pub current_artist: String,
    pub current_genre: String,
}

impl Default for SavedState {
    fn default() -> Self {
        Self {
            active_source: "spotify".to_string(),
            current_title: String::new(),
            current_artist: String::new(),
            current_genre: "Müzik".to_string(),
        }
    }
}

pub fn get_saved_state() -> SavedState {
    let path = get_state_file();
    if path.is_file() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(state) = serde_json::from_str::<SavedState>(&content) {
                return state;
            }
        }
    }
    SavedState::default()
}

pub fn save_state(state: &SavedState) {
    let path = get_state_file();
    if let Ok(json_str) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, json_str);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AuthData {
    pub spotify_user: String,
    pub spotify_premium: bool,
    #[serde(default)]
    pub spotify_client_id: String,
    #[serde(default)]
    pub spotify_access_token: String,
    #[serde(default)]
    pub spotify_refresh_token: String,
    #[serde(default)]
    pub spotify_token_expires_at: u64,
    pub yt_cookies: bool,
    pub qobuz_user: String,
}

impl Default for AuthData {
    fn default() -> Self {
        let cur_user = std::env::var("USER").unwrap_or_else(|_| "ozdil".to_string());
        Self {
            spotify_user: cur_user,
            spotify_premium: true,
            spotify_client_id: String::new(),
            spotify_access_token: String::new(),
            spotify_refresh_token: String::new(),
            spotify_token_expires_at: 0,
            yt_cookies: false,
            qobuz_user: String::new(),
        }
    }
}

pub fn get_auth_data() -> AuthData {
    let path = get_auth_file();
    if path.is_file() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(auth) = serde_json::from_str::<AuthData>(&content) {
                return auth;
            }
        }
    }
    AuthData::default()
}

pub fn save_auth_data(auth: &AuthData) {
    let path = get_auth_file();
    if let Ok(json_str) = serde_json::to_string_pretty(auth) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
        {
            let _ = file.write_all(json_str.as_bytes());
        }
    }
}

// ==============================================================================
// 2.1 SPOTIFY OAUTH 2.0 PKCE & CRYPTO UTILITIES (PURE RUST)
// ==============================================================================

pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let mut h0: u32 = 0x6a09e667;
    let mut h1: u32 = 0xbb67ae85;
    let mut h2: u32 = 0x3c6ef372;
    let mut h3: u32 = 0xa54ff53a;
    let mut h4: u32 = 0x510e527f;
    let mut h5: u32 = 0x9b05688c;
    let mut h6: u32 = 0x1f83d9ab;
    let mut h7: u32 = 0x5be0cd19;

    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;
        let mut f = h5;
        let mut g = h6;
        let mut h = h7;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
        h5 = h5.wrapping_add(f);
        h6 = h6.wrapping_add(g);
        h7 = h7.wrapping_add(h);
    }

    let mut out = [0u8; 32];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out[20..24].copy_from_slice(&h5.to_be_bytes());
    out[24..28].copy_from_slice(&h6.to_be_bytes());
    out[28..32].copy_from_slice(&h7.to_be_bytes());
    out
}

pub fn base64url_encode(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut res = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() { data[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        res.push(CHARSET[((triple >> 18) & 0x3F) as usize] as char);
        res.push(CHARSET[((triple >> 12) & 0x3F) as usize] as char);
        if i + 1 < data.len() {
            res.push(CHARSET[((triple >> 6) & 0x3F) as usize] as char);
        }
        if i + 2 < data.len() {
            res.push(CHARSET[(triple & 0x3F) as usize] as char);
        }
        i += 3;
    }
    res
}

pub fn generate_code_verifier() -> String {
    let mut buf = [0u8; 48];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut buf);
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(12345);
        let pid = std::process::id();
        let seed = format!("{}:{}", now, pid);
        let h = sha256(seed.as_bytes());
        buf[..32].copy_from_slice(&h);
    }
    base64url_encode(&buf)
}

pub fn generate_code_challenge(verifier: &str) -> String {
    let hash = sha256(verifier.as_bytes());
    base64url_encode(&hash)
}

pub fn percent_decode(input: &str) -> String {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub fn extract_query_param(req_line: &str, param: &str) -> Option<String> {
    let path_and_query = req_line.split_whitespace().nth(1)?;
    let query_str = path_and_query.split('?').nth(1)?;
    for pair in query_str.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        let val = parts.next().unwrap_or("");
        if key == param {
            return Some(percent_decode(val));
        }
    }
    None
}

pub const DEFAULT_SPOTIFY_CLIENT_ID: &str = "d420a117a32841c2b3474932e49fb54b";

pub fn start_spotify_oauth(client_id: &str) -> Result<AuthData, String> {
    let raw = client_id.trim();
    let effective_client_id = if raw.is_empty() || raw.contains('@') {
        DEFAULT_SPOTIFY_CLIENT_ID
    } else {
        raw
    };

    let is_default = effective_client_id == DEFAULT_SPOTIFY_CLIENT_ID;
    let port = if is_default { 8989 } else { 8888 };
    let redirect_uri = if is_default {
        "http://127.0.0.1:8989/login"
    } else {
        "http://127.0.0.1:8888/callback"
    };
    let redirect_encoded = if is_default {
        "http%3A%2F%2F127.0.0.1%3A8989%2Flogin"
    } else {
        "http%3A%2F%2F127.0.0.1%3A8888%2Fcallback"
    };

    let verifier = generate_code_verifier();
    let challenge = generate_code_challenge(&verifier);
    let state = generate_code_verifier();
    let state_slice = if state.len() >= 16 { &state[..16] } else { &state };

    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .map_err(|e| format!("127.0.0.1:{} portu açılamadı: {}", port, e))?;
    
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("Sunucu modu ayarlanamadı: {}", e))?;

    let scopes = "user-read-playback-state user-modify-playback-state user-read-currently-playing playlist-read-private playlist-read-collaborative playlist-modify-private playlist-modify-public user-follow-modify user-follow-read user-read-playback-position user-top-read user-read-recently-played user-library-modify user-library-read user-read-email user-read-private";
    let scopes_encoded = scopes.replace(' ', "%20");

    let auth_url = format!(
        "https://accounts.spotify.com/authorize?response_type=code&client_id={}&scope={}&redirect_uri={}&code_challenge_method=S256&code_challenge={}&state={}",
        effective_client_id, scopes_encoded, redirect_encoded, challenge, state_slice
    );

    println!("\n  \x1b[1;36m[1/3]\x1b[0m Yerel yetkilendirme dinleyicisi hazır: {}", redirect_uri);
    println!("  \x1b[1;36m[2/3]\x1b[0m Web tarayıcısında Spotify yetkilendirme sayfası açılıyor...");

    let _ = Command::new("xdg-open").arg(&auth_url).spawn();

    println!("  \x1b[1;33m[3/3]\x1b[0m Tarayıcıdan Spotify onayı bekleniyor (Zaman aşımı: 10 dk)...");
    println!("  \x1b[2m      (İptal etmek için terminalde Ctrl+C yapabilirsiniz)\x1b[0m");
    let _ = std::io::stdout().flush();

    let start_time = Instant::now();
    let timeout = Duration::from_secs(600);
    let mut auth_code = None;

    while start_time.elapsed() < timeout {
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let mut buf = [0u8; 4096];
                let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                let n = stream.read(&mut buf).unwrap_or(0);
                let req_text = String::from_utf8_lossy(&buf[..n]);

                if let Some(first_line) = req_text.lines().next() {
                    if first_line.contains("/favicon.ico") {
                        let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.flush();
                        continue;
                    }

                    if first_line.contains("/login") || first_line.contains("/callback") {
                        if let Some(err_val) = extract_query_param(first_line, "error") {
                            let resp_body = format!(
                                "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>OmaPlayer • Hata</title><style>body{{background:#0d1117;color:#f85149;font-family:sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;}}.box{{background:#161b22;border:1px solid #30363d;border-radius:12px;padding:32px;text-align:center;max-width:440px;}}h1{{color:#f85149;}}</style></head><body><div class=\"box\"><h1>Yetkilendirme Reddedildi</h1><p>Hata: {}</p></div></body></html>",
                                err_val
                            );
                            let resp = format!(
                                "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{}",
                                resp_body.as_bytes().len(),
                                resp_body
                            );
                            let _ = stream.write_all(resp.as_bytes());
                            let _ = stream.flush();
                            let _ = stream.shutdown(std::net::Shutdown::Both);
                            std::thread::sleep(Duration::from_millis(150));
                            return Err(format!("Spotify yetkilendirmesi iptal edildi / reddedildi: {}", err_val));
                        }

                        if let Some(code) = extract_query_param(first_line, "code") {
                            auth_code = Some(code);
                            let resp_body = "<!DOCTYPE html><html lang=\"tr\"><head><meta charset=\"utf-8\"><title>OmaPlayer • Başarılı</title><style>body{background:#0d1117;color:#c9d1d9;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;}.box{background:#161b22;border:1px solid #30363d;border-radius:12px;padding:36px 32px;max-width:440px;text-align:center;box-shadow:0 10px 30px rgba(0,0,0,0.6);}.badge{display:inline-block;background:#238636;color:#fff;padding:6px 14px;border-radius:20px;font-size:13px;font-weight:600;margin-bottom:18px;}h1{color:#58a6ff;margin:0 0 12px 0;font-size:22px;}p{color:#8b949e;font-size:14px;line-height:1.6;margin:0 0 20px 0;}.btn{display:inline-block;background:#21262d;color:#58a6ff;border:1px solid #30363d;padding:8px 18px;border-radius:6px;font-size:13px;text-decoration:none;cursor:pointer;}</style></head><body><div class=\"box\"><div class=\"badge\">✓ Spotify Bağlandı</div><h1>Yetkilendirme Başarılı</h1><p>Spotify hesabınız OmaPlayer terminal istasyonuna başarıyla bağlandı.<br>Bu sekmeyi güvenle kapatabilirsiniz.</p><button class=\"btn\" onclick=\"window.close()\">Sekmeyi Kapat</button></div></body></html>";
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{}",
                                resp_body.as_bytes().len(),
                                resp_body
                            );
                            let _ = stream.write_all(resp.as_bytes());
                            let _ = stream.flush();
                            let _ = stream.shutdown(std::net::Shutdown::Both);
                            std::thread::sleep(Duration::from_millis(150));
                            break;
                        }
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(e) => {
                return Err(format!("Ağ dinleme hatası: {}", e));
            }
        }
    }

    let code = auth_code.ok_or_else(|| {
        "Yetkilendirme zaman aşımına uğradı (tarayıcıdan beklenen süre içinde onay alınamadı).".to_string()
    })?;

    println!("\n  \x1b[1;32m✓\x1b[0m Onay kodu alındı. Güvenli erişim anahtarları talep ediliyor...");

    let token_output = Command::new("/usr/bin/curl")
        .args([
            "-s",
            "-X",
            "POST",
            "https://accounts.spotify.com/api/token",
            "--data-urlencode",
            "grant_type=authorization_code",
            "--data-urlencode",
            &format!("client_id={}", effective_client_id),
            "--data-urlencode",
            &format!("code={}", code),
            "--data-urlencode",
            &format!("redirect_uri={}", redirect_uri),
            "--data-urlencode",
            &format!("code_verifier={}", verifier),
        ])
        .output()
        .map_err(|e| format!("Curl token isteği hatası: {}", e))?;

    let token_json_str = String::from_utf8_lossy(&token_output.stdout);
    let token_val: serde_json::Value = serde_json::from_str(&token_json_str)
        .map_err(|_| format!("Geçersiz Spotify token yanıtı: {}", token_json_str))?;

    if let Some(err_desc) = token_val.get("error_description").and_then(|v| v.as_str()) {
        return Err(format!("Spotify Token Reddi: {}", err_desc));
    }
    if let Some(err) = token_val.get("error").and_then(|v| v.as_str()) {
        return Err(format!("Spotify Token Hatası: {}", err));
    }

    let access_token = token_val
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Yanıtta access_token bulunamadı.".to_string())?
        .to_string();

    let refresh_token = token_val
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let expires_in = token_val
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(3600);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    println!("  \x1b[1;32m✓\x1b[0m Spotify profil bilgileri sorgulanıyor (/v1/me)...");
    let prof_output = Command::new("/usr/bin/curl")
        .args([
            "-s",
            "-H",
            &format!("Authorization: Bearer {}", access_token),
            "https://api.spotify.com/v1/me",
        ])
        .output();

    let mut user_display = "Spotify User".to_string();
    let mut is_premium = true;

    if let Ok(out) = prof_output {
        let prof_str = String::from_utf8_lossy(&out.stdout);
        if let Ok(p_json) = serde_json::from_str::<serde_json::Value>(&prof_str) {
            if let Some(dn) = p_json.get("display_name").and_then(|v| v.as_str()) {
                user_display = dn.to_string();
            } else if let Some(id) = p_json.get("id").and_then(|v| v.as_str()) {
                user_display = id.to_string();
            }
            if let Some(prod) = p_json.get("product").and_then(|v| v.as_str()) {
                is_premium = prod == "premium";
            }
        }
    }

    let mut auth = get_auth_data();
    auth.spotify_user = sanitize_terminal_str(&user_display, 40, 128);
    auth.spotify_premium = is_premium;
    auth.spotify_client_id = effective_client_id.to_string();
    auth.spotify_access_token = access_token;
    auth.spotify_refresh_token = refresh_token;
    auth.spotify_token_expires_at = now + expires_in;

    save_auth_data(&auth);
    Ok(auth)
}

pub fn refresh_spotify_token(auth: &mut AuthData) -> Result<(), String> {
    if auth.spotify_refresh_token.is_empty() || auth.spotify_client_id.is_empty() {
        return Err("Yenilenecek Spotify refresh_token veya client_id yok.".to_string());
    }

    let output = Command::new("/usr/bin/curl")
        .args([
            "-s",
            "-X",
            "POST",
            "https://accounts.spotify.com/api/token",
            "--data-urlencode",
            "grant_type=refresh_token",
            "--data-urlencode",
            &format!("refresh_token={}", auth.spotify_refresh_token),
            "--data-urlencode",
            &format!("client_id={}", auth.spotify_client_id),
        ])
        .output()
        .map_err(|e| format!("Token yenileme isteği başarısız: {}", e))?;

    let json_str = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|_| format!("Geçersiz Spotify token yanıtı: {}", json_str))?;

    if let Some(at) = val.get("access_token").and_then(|v| v.as_str()) {
        auth.spotify_access_token = at.to_string();
        let exp = val.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(3600);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        auth.spotify_token_expires_at = now + exp;
        if let Some(rt) = val.get("refresh_token").and_then(|v| v.as_str()) {
            auth.spotify_refresh_token = rt.to_string();
        }
        save_auth_data(auth);
        Ok(())
    } else {
        Err(format!("Token yenilenemedi: {}", json_str))
    }
}

// ==============================================================================
// 3. MPV UNIX SOCKET IPC
// ==============================================================================

pub fn send_mpv_command(cmd_list: &[serde_json::Value]) -> Option<serde_json::Value> {
    let sock_path = get_socket_path();
    if !sock_path.exists() {
        return None;
    }

    let mut stream = UnixStream::connect(&sock_path).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));

    let payload = serde_json::json!({ "command": cmd_list });
    let mut req_bytes = serde_json::to_vec(&payload).ok()?;
    req_bytes.push(b'\n');

    stream.write_all(&req_bytes).ok()?;

    let reader = BufReader::new(stream);
    for line in reader.lines() {
        if let Ok(l) = line {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&l) {
                if val.get("data").is_some() || val.get("error").is_some() {
                    return val.get("data").cloned();
                }
            }
        } else {
            break;
        }
    }
    None
}

pub fn is_mpv_active() -> bool {
    let sock = get_socket_path();
    if !sock.exists() {
        return false;
    }
    matches!(
        send_mpv_command(&[serde_json::Value::from("get_property"), serde_json::Value::from("idle-active")]),
        Some(_)
    )
}

pub fn ensure_mpv_running() -> bool {
    let sock = get_socket_path();
    if sock.exists() {
        if is_mpv_active() {
            return true;
        }
        let _ = fs::remove_file(&sock);
    }

    let sock_str = sock.to_string_lossy().to_string();
    let _ = Command::new("/usr/bin/mpv")
        .args([
            "--no-video",
            "--idle",
            &format!("--input-ipc-server={}", sock_str),
            "--ytdl-format=bestaudio[ext=m4a]/bestaudio/best",
            "--volume=85",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(600) {
        if sock.exists() && is_mpv_active() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    sock.exists()
}

// ==============================================================================
// 4. PLAYBACK CONTROLS & STREAMING
// ==============================================================================

pub fn play_stream(url: &str, title: &str, artist: &str, source: &str) -> bool {
    if !ensure_mpv_running() {
        return false;
    }
    let _ = send_mpv_command(&[
        serde_json::Value::from("loadfile"),
        serde_json::Value::from(url),
        serde_json::Value::from("replace"),
    ]);

    let mut state = get_saved_state();
    state.active_source = source.to_string();
    state.current_title = sanitize_terminal_str(title, 50, 256);
    state.current_artist = sanitize_terminal_str(artist, 40, 256);
    save_state(&state);
    true
}

pub fn play_query_stream(query: &str, source: &str) -> bool {
    let clean_q = sanitize_terminal_str(query, 50, 256);
    if clean_q.is_empty() {
        return false;
    }
    let ytdl_url = format!("ytdl://ytsearch1:{}", clean_q);
    let artist = if source == "spotify" {
        "Spotify Premium CLI"
    } else {
        "YouTube Music"
    };
    play_stream(&ytdl_url, &clean_q, artist, source)
}

pub fn control_playback(action: &str) -> bool {
    if is_mpv_active() {
        match action {
            "play_pause" => {
                send_mpv_command(&[
                    serde_json::Value::from("cycle"),
                    serde_json::Value::from("pause"),
                ]);
                return true;
            }
            "vol_up" => {
                send_mpv_command(&[
                    serde_json::Value::from("add"),
                    serde_json::Value::from("volume"),
                    serde_json::Value::from(5),
                ]);
                return true;
            }
            "vol_down" => {
                send_mpv_command(&[
                    serde_json::Value::from("add"),
                    serde_json::Value::from("volume"),
                    serde_json::Value::from(-5),
                ]);
                return true;
            }
            "stop" => {
                send_mpv_command(&[serde_json::Value::from("stop")]);
                return true;
            }
            _ => {}
        }
    }

    // MPRIS fallback via busctl
    if let Some(mpris) = get_spotify_mpris() {
        if let Some(service) = mpris.service {
            let mpris_action = match action {
                "play_pause" => Some("PlayPause"),
                "next" => Some("Next"),
                "prev" => Some("Previous"),
                _ => None,
            };
            if let Some(act) = mpris_action {
                let _ = Command::new("/usr/bin/busctl")
                    .args([
                        "--user",
                        "call",
                        &service,
                        "/org/mpris/MediaPlayer2",
                        "org.mpris.MediaPlayer2.Player",
                        act,
                    ])
                    .env("PATH", "/usr/bin:/bin")
                    .env("LC_ALL", "C")
                    .output();
                return true;
            }
        }
    }
    false
}

// ==============================================================================
// 5. METRICS & STATUS MODELS
// ==============================================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlaybackInfo {
    pub status: String,
    pub is_running: bool,
    pub source: String,
    pub source_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub codec: String,
    pub bitrate_kbps: u32,
    pub sample_rate: String,
    pub is_lossless: bool,
    pub quality_label: String,
    pub position_sec: u64,
    pub length_sec: u64,
    pub volume_pct: u32,
    pub service: Option<String>,
}

pub fn get_spotify_mpris() -> Option<PlaybackInfo> {
    let out = Command::new("/usr/bin/busctl")
        .args(["--user", "list"])
        .env("PATH", "/usr/bin:/bin")
        .env("LC_ALL", "C")
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if line.contains("org.mpris.MediaPlayer2")
            && (line.to_lowercase().contains("spotify") || line.to_lowercase().contains("ncspot"))
        {
            if let Some(service) = line.split_whitespace().find(|w| w.starts_with("org.mpris.MediaPlayer2")) {
                let meta_out = Command::new("/usr/bin/busctl")
                    .args([
                        "--user",
                        "get-property",
                        service,
                        "/org/mpris/MediaPlayer2",
                        "org.mpris.MediaPlayer2.Player",
                        "Metadata",
                    ])
                    .env("PATH", "/usr/bin:/bin")
                    .env("LC_ALL", "C")
                    .output()
                    .ok()?;
                let meta_str = String::from_utf8_lossy(&meta_out.stdout);

                let mut title = "Bilinmeyen Şarkı".to_string();
                let mut artist = "Spotify".to_string();
                let mut album = String::new();
                let mut length_sec = 0;

                for m_line in meta_str.lines() {
                    if m_line.contains("\"xesam:title\"") {
                        if let Some(start) = m_line.find('"') {
                            if let Some(end) = m_line.rfind('"') {
                                if start != end {
                                    let parts: Vec<&str> = m_line.split('"').collect();
                                    if parts.len() >= 4 {
                                        title = parts[3].to_string();
                                    }
                                }
                            }
                        }
                    } else if m_line.contains("\"xesam:artist\"") {
                        let parts: Vec<&str> = m_line.split('"').collect();
                        if parts.len() >= 4 {
                            artist = parts[3].to_string();
                        }
                    } else if m_line.contains("\"xesam:album\"") {
                        let parts: Vec<&str> = m_line.split('"').collect();
                        if parts.len() >= 4 {
                            album = parts[3].to_string();
                        }
                    } else if m_line.contains("\"mpris:length\"") {
                        if let Some(num_str) = m_line.split_whitespace().last() {
                            if let Ok(us) = num_str.parse::<u64>() {
                                length_sec = us / 1_000_000;
                            }
                        }
                    }
                }

                let st_out = Command::new("/usr/bin/busctl")
                    .args([
                        "--user",
                        "get-property",
                        service,
                        "/org/mpris/MediaPlayer2",
                        "org.mpris.MediaPlayer2.Player",
                        "PlaybackStatus",
                    ])
                    .env("PATH", "/usr/bin:/bin")
                    .env("LC_ALL", "C")
                    .output()
                    .ok()?;
                let st_str = String::from_utf8_lossy(&st_out.stdout);
                let status = if st_str.contains("Playing") {
                    "PLAYING"
                } else {
                    "PAUSED"
                };

                return Some(PlaybackInfo {
                    status: status.to_string(),
                    is_running: true,
                    source: "spotify".to_string(),
                    source_name: "Spotify Premium".to_string(),
                    title: sanitize_terminal_str(&title, 40, 256),
                    artist: sanitize_terminal_str(&artist, 35, 256),
                    album: sanitize_terminal_str(&album, 35, 256),
                    codec: "OGG VORBIS".to_string(),
                    bitrate_kbps: 320,
                    sample_rate: "44.1 kHz".to_string(),
                    is_lossless: false,
                    quality_label: "SPOTIFY PREMIUM: 320 kbps (44.1 kHz)".to_string(),
                    position_sec: 0,
                    length_sec,
                    volume_pct: 85,
                    service: Some(service.to_string()),
                });
            }
        }
    }
    None
}

pub fn get_playback_info() -> PlaybackInfo {
    if let Some(sp) = get_spotify_mpris() {
        if sp.status == "PLAYING" {
            return sp;
        }
    }

    if is_mpv_active() {
        let idle_val = send_mpv_command(&[
            serde_json::Value::from("get_property"),
            serde_json::Value::from("idle-active"),
        ]);
        let is_idle = idle_val.and_then(|v| v.as_bool()).unwrap_or(true);

        if !is_idle {
            let paused = send_mpv_command(&[
                serde_json::Value::from("get_property"),
                serde_json::Value::from("pause"),
            ])
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

            let raw_media_title = send_mpv_command(&[
                serde_json::Value::from("get_property"),
                serde_json::Value::from("media-title"),
            ])
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "Terminal Audio".to_string());
            let media_title = sanitize_terminal_str(&raw_media_title, 64, 256);

            let pos = send_mpv_command(&[
                serde_json::Value::from("get_property"),
                serde_json::Value::from("time-pos"),
            ])
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u64;

            let dur = send_mpv_command(&[
                serde_json::Value::from("get_property"),
                serde_json::Value::from("duration"),
            ])
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u64;

            let vol = send_mpv_command(&[
                serde_json::Value::from("get_property"),
                serde_json::Value::from("volume"),
            ])
            .and_then(|v| v.as_f64())
            .unwrap_or(85.0) as u32;

            let raw_codec = send_mpv_command(&[
                serde_json::Value::from("get_property"),
                serde_json::Value::from("audio-codec-name"),
            ])
            .and_then(|v| v.as_str().map(|s| s.to_uppercase()))
            .unwrap_or_else(|| "OPUS".to_string());
            let clean_codec = sanitize_terminal_str(&raw_codec, 16, 64);

            let raw_bitrate = send_mpv_command(&[
                serde_json::Value::from("get_property"),
                serde_json::Value::from("audio-bitrate"),
            ])
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

            let raw_sr = send_mpv_command(&[
                serde_json::Value::from("get_property"),
                serde_json::Value::from("audio-params/samplerate"),
            ])
            .and_then(|v| v.as_f64())
            .unwrap_or(48000.0);

            let bitrate_kbps = if raw_bitrate > 0.0 {
                (raw_bitrate / 1000.0) as u32
            } else {
                320
            };
            let sr_khz = format!("{:.1} kHz", raw_sr / 1000.0);
            let is_lossless = matches!(
                clean_codec.as_str(),
                "FLAC" | "WAV" | "ALAC" | "PCM_S16LE" | "PCM_S24LE" | "PCM_S32LE"
            );

            let state = get_saved_state();
            let source = sanitize_terminal_str(&state.active_source, 20, 64);
            let auth = get_auth_data();

            let quality_label = if is_lossless {
                format!("LOSSLESS ({} 24-bit • {})", clean_codec, sr_khz)
            } else if source == "spotify" || auth.spotify_premium {
                format!("SPOTIFY PREMIUM: 320 kbps ({})", sr_khz)
            } else {
                format!("{} kbps • {} ({})", bitrate_kbps, clean_codec, sr_khz)
            };

            let src_names = [
                ("spotify", "Spotify Premium"),
                ("youtube", "YouTube Music"),
                ("qobuz", "Qobuz Hi-Res"),
                ("radio", "Canlı Radyo"),
                ("local", "Yerel Müzik"),
            ];
            let source_name = src_names
                .iter()
                .find(|(k, _)| *k == source.as_str())
                .map(|(_, v)| *v)
                .unwrap_or("OmaPlayer");

            let clean_title = if !state.current_title.is_empty() {
                sanitize_terminal_str(&state.current_title, 50, 256)
            } else {
                media_title
            };

            let clean_artist = if !state.current_artist.is_empty() {
                sanitize_terminal_str(&state.current_artist, 40, 256)
            } else {
                "Spotify Premium CLI".to_string()
            };

            return PlaybackInfo {
                status: if paused { "PAUSED" } else { "PLAYING" }.to_string(),
                is_running: true,
                source,
                source_name: source_name.to_string(),
                title: clean_title,
                artist: clean_artist,
                album: "OmaPlayer Native Engine".to_string(),
                codec: clean_codec,
                bitrate_kbps,
                sample_rate: sr_khz,
                is_lossless,
                quality_label: sanitize_terminal_str(&quality_label, 64, 256),
                position_sec: pos,
                length_sec: dur,
                volume_pct: vol,
                service: Some("OmaPlayer MPV Daemon".to_string()),
            };
        }
    }

    if let Some(sp) = get_spotify_mpris() {
        return sp;
    }

    let state = get_saved_state();
    let auth = get_auth_data();
    let user_str = sanitize_terminal_str(&auth.spotify_user, 20, 64);

    let clean_title = if !state.current_title.is_empty() {
        sanitize_terminal_str(&state.current_title, 50, 256)
    } else {
        "Müzik Çalmıyor".to_string()
    };

    let clean_artist = if !state.current_artist.is_empty() {
        sanitize_terminal_str(&state.current_artist, 40, 256)
    } else if !user_str.is_empty() {
        format!("{} • OmaPlayer", user_str)
    } else {
        "OmaPlayer • Müzik & Radyo".to_string()
    };

    let quality_label = if auth.spotify_premium {
        "SPOTIFY PREMIUM: 320 kbps (Hazır)".to_string()
    } else {
        "OMAPLAYER HI-FI (Hazır)".to_string()
    };

    PlaybackInfo {
        status: "STOPPED".to_string(),
        is_running: false,
        source: sanitize_terminal_str(&state.active_source, 20, 64),
        source_name: "OmaPlayer".to_string(),
        title: clean_title,
        artist: clean_artist,
        album: String::new(),
        codec: "None".to_string(),
        bitrate_kbps: 320,
        sample_rate: "48.0 kHz".to_string(),
        is_lossless: false,
        quality_label,
        position_sec: 0,
        length_sec: 0,
        volume_pct: 0,
        service: None,
    }
}

pub fn list_local_music() -> Vec<serde_json::Value> {
    let mut tracks = Vec::new();
    let home = dirs_home().unwrap_or_else(|| PathBuf::from("."));
    let music_dir = home.join("Music");
    if !music_dir.is_dir() {
        return tracks;
    }

    fn visit_dir(dir: &Path, tracks: &mut Vec<serde_json::Value>) {
        if tracks.len() >= 50 {
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    visit_dir(&p, tracks);
                } else if p.is_file() {
                    let ext = p
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    if matches!(ext.as_str(), "mp3" | "flac" | "wav" | "m4a" | "ogg" | "opus") {
                        let name = p
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or("track");
                        tracks.push(serde_json::json!({
                            "name": sanitize_terminal_str(name, 40, 128),
                            "path": p.to_string_lossy().to_string()
                        }));
                        if tracks.len() >= 50 {
                            return;
                        }
                    }
                }
            }
        }
    }

    visit_dir(&music_dir, &mut tracks);
    tracks
}

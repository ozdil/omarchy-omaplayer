use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
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
    let p = dirs_home()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/share/omarchy/omaplayer");
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
    pub yt_cookies: bool,
    pub qobuz_user: String,
}

impl Default for AuthData {
    fn default() -> Self {
        let cur_user = std::env::var("USER").unwrap_or_else(|_| "ozdil".to_string());
        Self {
            spotify_user: cur_user,
            spotify_premium: true,
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

    PlaybackInfo {
        status: "STOPPED".to_string(),
        is_running: false,
        source: sanitize_terminal_str(&state.active_source, 20, 64),
        source_name: "OmaPlayer".to_string(),
        title: if !state.current_title.is_empty() {
            sanitize_terminal_str(&state.current_title, 50, 256)
        } else {
            "Müzik Çalmıyor".to_string()
        },
        artist: if !state.current_artist.is_empty() {
            sanitize_terminal_str(&state.current_artist, 40, 256)
        } else {
            format!("{} (Spotify Premium)", user_str)
        },
        album: String::new(),
        codec: "None".to_string(),
        bitrate_kbps: 320,
        sample_rate: "48.0 kHz".to_string(),
        is_lossless: false,
        quality_label: "SPOTIFY PREMIUM: 320 kbps (Hazır)".to_string(),
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

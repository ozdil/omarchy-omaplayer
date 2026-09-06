use omaplayer::{
    control_playback, get_auth_data, get_playback_info, get_radio_streams,
    play_query_stream, play_stream, sanitize_terminal_str, save_auth_data, start_spotify_oauth,
    strip_ansi, AuthData, PlaybackInfo, DEFAULT_SPOTIFY_CLIENT_ID,
};
use std::io::{self, Write};
use std::mem::MaybeUninit;
use std::time::Duration;

// ==============================================================================
// 1. TERMINAL RAW MODE RAII GUARD (STAIR-STEPPING & CURSOR PROTECTED)
// ==============================================================================

struct RawModeGuard {
    orig_termios: libc::termios,
    active: bool,
}

impl RawModeGuard {
    fn new() -> Option<Self> {
        unsafe {
            if libc::isatty(libc::STDIN_FILENO) == 0 {
                return None;
            }
            let mut orig: MaybeUninit<libc::termios> = MaybeUninit::uninit();
            if libc::tcgetattr(libc::STDIN_FILENO, orig.as_mut_ptr()) != 0 {
                return None;
            }
            let orig_termios = orig.assume_init();
            let mut raw = orig_termios;
            libc::cfmakeraw(&mut raw);
            // CRITICAL: Re-enable OPOST and ONLCR in output flags.
            // Without ONLCR, \n does NOT perform a Carriage Return (\r),
            // which causes every subsequent line to start at the column of the previous line (stair-stepping).
            raw.c_oflag |= libc::OPOST | libc::ONLCR;
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) != 0 {
                return None;
            }
            // Hide terminal cursor while in TUI dashboard mode
            let _ = io::stdout().write_all(b"\x1b[?25l");
            let _ = io::stdout().flush();
            Some(Self {
                orig_termios,
                active: true,
            })
        }
    }

    fn pause(&mut self) {
        if self.active {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.orig_termios);
            }
            // Show cursor for interactive prompts
            let _ = io::stdout().write_all(b"\x1b[?25h");
            let _ = io::stdout().flush();
            self.active = false;
        }
    }

    fn resume(&mut self) {
        if !self.active {
            unsafe {
                let mut raw = self.orig_termios;
                libc::cfmakeraw(&mut raw);
                raw.c_oflag |= libc::OPOST | libc::ONLCR;
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw);
            }
            // Re-hide cursor
            let _ = io::stdout().write_all(b"\x1b[?25l");
            let _ = io::stdout().flush();
            self.active = true;
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.orig_termios);
            }
        }
        // Restore cursor and move to newline
        let _ = io::stdout().write_all(b"\x1b[?25h\r\n");
        let _ = io::stdout().flush();
    }
}

fn get_terminal_size() -> (u16, u16) {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0
            && ws.ws_col > 0
            && ws.ws_row > 0
        {
            (ws.ws_col, ws.ws_row)
        } else {
            (80, 24)
        }
    }
}

// ==============================================================================
// 2. FORMATTING HELPERS
// ==============================================================================

fn format_time(seconds: u64) -> String {
    let m = seconds / 60;
    let s = seconds % 60;
    format!("{:02}:{:02}", m, s)
}

fn render_progress_bar(pos: u64, length: u64, width: usize) -> String {
    let w = width.clamp(6, 26);
    let ratio = if length > 0 {
        (pos as f64 / length as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = (ratio * w as f64) as usize;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(w.saturating_sub(filled)));
    format!("[{}] {} / {}", bar, format_time(pos), format_time(length))
}

fn render_volume_bar(vol: u32, width: usize) -> String {
    let w = width.max(6);
    let ratio = (vol as f64 / 100.0).clamp(0.0, 1.0);
    let filled = (ratio * w as f64) as usize;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(w.saturating_sub(filled)));
    format!("Ses: %{:<3} [{}]", vol, bar)
}

fn make_row(content: &str, inner_w: usize) -> String {
    let clean = strip_ansi(content);
    let vis_len = clean.chars().count();
    if vis_len > inner_w {
        let truncated: String = clean.chars().take(inner_w).collect();
        format!("│ {} │", truncated)
    } else {
        let padding = " ".repeat(inner_w - vis_len);
        format!("│ {}{} │", content, padding)
    }
}

fn pad_cell(content: &str, width: usize) -> String {
    let clean = strip_ansi(content);
    let vis_len = clean.chars().count();
    if vis_len > width {
        clean.chars().take(width).collect()
    } else {
        format!("{}{}", content, " ".repeat(width - vis_len))
    }
}

// ==============================================================================
// 3. COMPACT & FULLSCREEN RENDERERS
// ==============================================================================

fn render_compact_view(
    data: &PlaybackInfo,
    _auth: &AuthData,
    wave_frames: &[&str],
    frame_idx: usize,
    cols: u16,
) -> String {
    let w = (cols as usize).saturating_sub(2).clamp(46, 68);
    let inner_w = w.saturating_sub(4);

    let is_playing = data.status == "PLAYING";
    let title = sanitize_terminal_str(&data.title, inner_w.saturating_sub(6), 256);
    let artist = sanitize_terminal_str(&data.artist, inner_w.saturating_sub(10), 256);
    let src_label = sanitize_terminal_str(&data.source_name, 14, 64);
    let quality = sanitize_terminal_str(&data.quality_label, inner_w.saturating_sub(4), 128);

    let track_full = if !artist.is_empty() && artist != "OmaPlayer" {
        format!("{} • {}", title, artist)
    } else {
        title
    };
    let track_bounded: String = track_full.chars().take(inner_w.saturating_sub(4)).collect();

    let (st_badge, raw_wave) = if is_playing {
        (
            "\x1b[1;32m▶ CALIYOR\x1b[0m",
            wave_frames[frame_idx % wave_frames.len()],
        )
    } else if data.status == "PAUSED" {
        (
            "\x1b[1;33m⏸ DURDU\x1b[0m  ",
            "─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─",
        )
    } else {
        (
            "\x1b[0;90m■ KAPALI\x1b[0m ",
            "─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─",
        )
    };

    let wave_slice: String = raw_wave.chars().take(inner_w.saturating_sub(4)).collect();

    let mut buf = Vec::new();

    // Top border - starts with cursor home \x1b[H so no empty top line is created
    buf.push(format!("\x1b[H\x1b[1;36m┌{}┐\x1b[0m", "─".repeat(inner_w + 2)));

    // Header
    let h_left = "  \x1b[1;37mOMAPLAYER\x1b[0m";
    let h_right = format!("[{}]", src_label.chars().take(12).collect::<String>());
    let left_clean = strip_ansi(h_left);
    let spaces = inner_w
        .saturating_sub(left_clean.chars().count())
        .saturating_sub(h_right.chars().count())
        .max(1);
    buf.push(make_row(
        &format!("{}{}\x1b[0;90m{}\x1b[0m", h_left, " ".repeat(spaces), h_right),
        inner_w,
    ));
    buf.push(format!("\x1b[1;36m├{}┤\x1b[0m", "─".repeat(inner_w + 2)));

    // Track Info
    buf.push(make_row("", inner_w));
    buf.push(make_row(&format!("  \x1b[1;32m{}\x1b[0m", track_bounded), inner_w));

    // Quality
    let q_color = if data.is_lossless { "\x1b[1;35m" } else { "\x1b[1;33m" };
    buf.push(make_row(&format!("  {}{}\x1b[0m", q_color, quality), inner_w));

    // Progress
    let p_bar = render_progress_bar(data.position_sec, data.length_sec, inner_w.saturating_sub(28));
    buf.push(make_row(&format!("  {} {}", st_badge, p_bar), inner_w));

    // Wave / Spectrum
    buf.push(make_row("", inner_w));
    buf.push(make_row(&format!("  \x1b[1;36m{}\x1b[0m", wave_slice), inner_w));
    buf.push(make_row("", inner_w));

    // Divider
    buf.push(format!("\x1b[1;36m├{}┤\x1b[0m", "─".repeat(inner_w + 2)));

    // Footer
    buf.push(make_row("  \x1b[1;32m1\x1b[0m:Spotify \x1b[1;31m2\x1b[0m:YT \x1b[1;33m3\x1b[0m:Radyo \x1b[1;35mp\x1b[0m:Listeler \x1b[1;36m5\x1b[0m:Hesap \x1b[1;37mf\x1b[0m:Ara", inner_w));
    buf.push(make_row("  \x1b[1;37mSpace\x1b[0m:Oynat/Dur   \x1b[1;37m+/-\x1b[0m:Ses   \x1b[1;31mx\x1b[0m:Durdur   \x1b[1;31mq\x1b[0m:Cikis", inner_w));
    buf.push(format!("\x1b[1;36m└{}┘\x1b[0m", "─".repeat(inner_w + 2)));

    buf.join("\r\n")
}

fn render_fullscreen_view(
    data: &PlaybackInfo,
    auth: &AuthData,
    wave_frames: &[&str],
    frame_idx: usize,
    cols: u16,
) -> String {
    let w_left = 28;
    let w_right = 30;
    let w_total = (cols as usize).saturating_sub(2).clamp(88, 120);
    let w_mid = w_total.saturating_sub(w_left + w_right + 4);

    let is_playing = data.status == "PLAYING";
    let title = sanitize_terminal_str(&data.title, w_mid.saturating_sub(6), 256);
    let artist = sanitize_terminal_str(&data.artist, w_mid.saturating_sub(8), 256);
    let src_label = sanitize_terminal_str(&data.source_name, 22, 64);
    let codec = sanitize_terminal_str(&data.codec, 14, 64);
    let sr = sanitize_terminal_str(&data.sample_rate, 14, 64);
    let auth_user = sanitize_terminal_str(&auth.spotify_user, 18, 64);

    let track_full = if !artist.is_empty() && artist != "OmaPlayer" {
        format!("{} • {}", title, artist)
    } else {
        title
    };
    let track_bounded: String = track_full.chars().take(w_mid.saturating_sub(4)).collect();

    let st_text = if is_playing {
        "\x1b[1;32m▶ CALIYOR (Hi-Res Engine)\x1b[0m"
    } else if data.status == "PAUSED" {
        "\x1b[1;33m⏸ DURDURULDU\x1b[0m"
    } else {
        "\x1b[0;90m■ BEKLEMEDE\x1b[0m"
    };

    let raw_wave1 = wave_frames[frame_idx % wave_frames.len()];
    let raw_wave2 = wave_frames[(frame_idx + 2) % wave_frames.len()];
    let wave1: String = raw_wave1.chars().take(w_mid.saturating_sub(4)).collect();
    let wave2: String = raw_wave2.chars().take(w_mid.saturating_sub(4)).collect();

    let mut buf = Vec::new();

    // Top Frame
    buf.push(format!("\x1b[H\x1b[1;35m┌{}┐\x1b[0m", "─".repeat(w_total.saturating_sub(2))));
    let title_bar = "OMAPLAYER PRO HI-FI STUDIO • HIGH PERFORMANCE AUDIO WORKSTATION";
    buf.push(format!(
        "│ {} │",
        pad_cell(&format!("\x1b[1;37m{}\x1b[0m", title_bar), w_total.saturating_sub(4))
    ));
    buf.push(format!(
        "\x1b[1;35m├{}┬{}┬{}┤\x1b[0m",
        "─".repeat(w_left),
        "─".repeat(w_mid),
        "─".repeat(w_right)
    ));

    // Headers
    let c1_h = " \x1b[1;33mPLATFORMLAR & KONTROL\x1b[0m";
    let c2_h = " \x1b[1;37mCALAN PARCA & SPEKTRUM\x1b[0m";
    let c3_h = " \x1b[1;36mDONANIM & SES DETAYI\x1b[0m";
    buf.push(format!(
        "│{}│{}│{}│",
        pad_cell(c1_h, w_left),
        pad_cell(c2_h, w_mid),
        pad_cell(c3_h, w_right)
    ));
    buf.push(format!(
        "\x1b[1;35m├{}┼{}┼{}┤\x1b[0m",
        "─".repeat(w_left),
        "─".repeat(w_mid),
        "─".repeat(w_right)
    ));

    // Row 1
    let r1_1 = " \x1b[1;32m[1]\x1b[0m Spotify Premium Stüdyo";
    let r1_2 = format!(" \x1b[1;32m{}\x1b[0m", track_bounded);
    let r1_3 = " Cikis: PipeWire / ALSA";
    buf.push(format!(
        "│{}│{}│{}│",
        pad_cell(r1_1, w_left),
        pad_cell(&r1_2, w_mid),
        pad_cell(r1_3, w_right)
    ));

    // Row 2
    let r2_1 = " \x1b[1;31m[2]\x1b[0m YouTube Music CLI";
    let r2_2 = format!(" Durum  : {}", st_text);
    let r2_3 = format!(" Bitrate: {} kbps (Max)", data.bitrate_kbps);
    buf.push(format!(
        "│{}│{}│{}│",
        pad_cell(r2_1, w_left),
        pad_cell(&r2_2, w_mid),
        pad_cell(&r2_3, w_right)
    ));

    // Row 3
    let r3_1 = " \x1b[1;33m[3]\x1b[0m Canlı Radyo İstasyonları";
    let r3_2 = format!(" Kaynak : \x1b[1;35m{}\x1b[0m", src_label);
    let r3_3 = format!(" Format : {} • {}", codec, sr);
    buf.push(format!(
        "│{}│{}│{}│",
        pad_cell(r3_1, w_left),
        pad_cell(&r3_2, w_mid),
        pad_cell(&r3_3, w_right)
    ));

    // Row 4
    let r4_1 = " \x1b[1;35m[p]\x1b[0m Kişisel Çalma Listeleri";
    let p_bar = render_progress_bar(data.position_sec, data.length_sec, w_mid.saturating_sub(24));
    let r4_2 = format!(" {}", p_bar);
    let r4_3 = format!(
        " Kalite : {}",
        if data.is_lossless {
            "LOSSLESS FLAC"
        } else {
            "SPOTIFY PREMIUM"
        }
    );
    buf.push(format!(
        "│{}│{}│{}│",
        pad_cell(r4_1, w_left),
        pad_cell(&r4_2, w_mid),
        pad_cell(&r4_3, w_right)
    ));

    // Row 5
    let r5_1 = " \x1b[1;36m[5]\x1b[0m Hesap & Üyelik Girişi";
    let r5_2 = format!(" \x1b[1;36m{}\x1b[0m", wave1);
    let r5_3 = format!(" Oturum : {} (Premium)", auth_user);
    buf.push(format!(
        "│{}│{}│{}│",
        pad_cell(r5_1, w_left),
        pad_cell(&r5_2, w_mid),
        pad_cell(&r5_3, w_right)
    ));

    // Row 6
    let r6_1 = " \x1b[1;37m[f]\x1b[0m Şarkı / Sanatçı Ara";
    let r6_2 = format!(" \x1b[1;35m{}\x1b[0m", wave2);
    let r6_3 = format!(" {}", render_volume_bar(data.volume_pct, 10));
    buf.push(format!(
        "│{}│{}│{}│",
        pad_cell(r6_1, w_left),
        pad_cell(&r6_2, w_mid),
        pad_cell(&r6_3, w_right)
    ));

    // Divider
    buf.push(format!(
        "\x1b[1;35m├{}┼{}┼{}┤\x1b[0m",
        "─".repeat(w_left),
        "─".repeat(w_mid),
        "─".repeat(w_right)
    ));

    // Footer
    let c1_f = " [Space] Oynat/Dur  [q] Cikis";
    let c2_f = " [+/-] Ses Ayarla (%5)   [x] Durdur";
    let c3_f = " • Equalizer: Flat / Hi-Res";
    buf.push(format!(
        "│{}│{}│{}│",
        pad_cell(c1_f, w_left),
        pad_cell(c2_f, w_mid),
        pad_cell(c3_f, w_right)
    ));
    buf.push(format!(
        "\x1b[1;35m└{}┴{}┴{}┘\x1b[0m",
        "─".repeat(w_left),
        "─".repeat(w_mid),
        "─".repeat(w_right)
    ));

    buf.join("\r\n")
}

// ==============================================================================
// 4. INTERACTIVE DIALOGS
// ==============================================================================

fn search_dialog(guard: &mut RawModeGuard) {
    guard.pause();
    print!("\x1b[2J\x1b[H");
    println!("\x1b[1;36m┌──────────────────────────────────────────────────────────┐\x1b[0m");
    println!("│  \x1b[1;37mOMAPLAYER ARAMA • Şarkı veya Sanatçı Yazın\x1b[0m              │");
    println!("\x1b[1;36m└──────────────────────────────────────────────────────────┘\x1b[0m\n");
    print!("  Arama: ");
    let _ = io::stdout().flush();

    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_ok() {
        let q = sanitize_terminal_str(line.trim(), 60, 256);
        if !q.is_empty() {
            println!("\n  \x1b[1;32m'{}' aranıyor ve başlatılıyor...\x1b[0m", q);
            play_query_stream(&q, "spotify");
            std::thread::sleep(Duration::from_millis(800));
        }
    }
    guard.resume();
}

fn playlists_dialog(guard: &mut RawModeGuard) {
    guard.pause();
    print!("\x1b[2J\x1b[H");
    println!("\x1b[1;35m┌──────────────────────────────────────────────────────────┐\x1b[0m");
    println!("│  📂 \x1b[1;37mKİŞİSEL ÇALMA LİSTELERİ & ARŞİV\x1b[0m                      │");
    println!("\x1b[1;35m├──────────────────────────────────────────────────────────┤\x1b[0m");
    println!("│  \x1b[1;32m[1]\x1b[0m ★ Beğenilen Şarkılarım (Liked Songs)               │");
    println!("│  \x1b[1;36m[2]\x1b[0m ★ Haftalık Keşif Listesi (Discover Weekly)         │");
    println!("│  \x1b[1;33m[3]\x1b[0m ★ Türkçe Pop, Rap & Alternatif Hitleri              │");
    println!("│  \x1b[1;34m[4]\x1b[0m ★ Lofi & Kodlama Odaklanma Miksi                     │");
    println!("│  \x1b[1;35m[5]\x1b[0m ★ Synthwave / Retrowave Cyberpunk Arşivi           │");
    println!("│  \x1b[1;31m[6]\x1b[0m ★ Efsane Classic Rock Klasikleri                     │");
    println!("│  \x1b[1;37m[7]\x1b[0m ★ Klasik Müzik & Piyano Konsantrasyon              │");
    println!("\x1b[1;35m└──────────────────────────────────────────────────────────┘\x1b[0m\n");
    print!("  Çalmak İstediğiniz Liste [1-7]: ");
    let _ = io::stdout().flush();

    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_ok() {
        let p_map = [
            ("1", "Liked Songs Top Hits"),
            ("2", "Discover Weekly Mix"),
            ("3", "Turkce Pop Rap Hitleri"),
            ("4", "Lofi Beats Coding Mix"),
            ("5", "Synthwave Cyberpunk Mix"),
            ("6", "Classic Rock Greatest Hits"),
            ("7", "Chopin & Classical Piano Masterpieces"),
        ];
        let c = line.trim();
        if let Some((_, query)) = p_map.iter().find(|(k, _)| *k == c) {
            println!("\n  \x1b[1;32m'{}' çalma listesi başlatılıyor...\x1b[0m", query);
            play_query_stream(query, "spotify");
            std::thread::sleep(Duration::from_millis(800));
        }
    }
    guard.resume();
}

fn radio_dialog(guard: &mut RawModeGuard) {
    guard.pause();
    print!("\x1b[2J\x1b[H");
    println!("\x1b[1;33m┌──────────────────────────────────────────────────────────┐\x1b[0m");
    println!("│  \x1b[1;37mCANLI İNTERNET RADYOLARI • 7/24 Kesintisiz Yayın\x1b[0m        │");
    println!("\x1b[1;33m├──────────────────────────────────────────────────────────┤\x1b[0m");
    println!("│  [1] Lofi Beats (Sakin / Chill)                          │");
    println!("│  [2] Jazz Radio Classics (Klasik Caz)                    │");
    println!("│  [3] Nightwave Plaza (Synthwave / Cyberpunk)             │");
    println!("│  [4] Classic Rock Radio (Efsane Rock)                    │");
    println!("│  [5] TRT Radyo 3 (Klasik Müzik & Kültür)                 │");
    println!("\x1b[1;33m└──────────────────────────────────────────────────────────┘\x1b[0m\n");
    print!("  İstasyon Seçiniz [1-5]: ");
    let _ = io::stdout().flush();

    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_ok() {
        let st_map = [
            ("1", "lofi"),
            ("2", "jazz"),
            ("3", "synthwave"),
            ("4", "rock"),
            ("5", "trt"),
        ];
        let c = line.trim();
        if let Some((_, st_id)) = st_map.iter().find(|(k, _)| *k == c) {
            let radios = get_radio_streams();
            let (url, name, genre) = if let Some(st) = radios.get(st_id) {
                (st.url, st.name, st.genre)
            } else {
                let lofi = &radios["lofi"];
                (lofi.url, lofi.name, lofi.genre)
            };
            println!("\n  \x1b[1;32m'{}' başlatılıyor...\x1b[0m", name);
            play_stream(url, name, genre, "radio");
            std::thread::sleep(Duration::from_millis(800));
        }
    }
    guard.resume();
}

fn account_dialog(guard: &mut RawModeGuard) {
    guard.pause();
    let mut auth = get_auth_data();
    print!("\x1b[2J\x1b[H");
    println!("\x1b[1;36m┌──────────────────────────────────────────────────────────┐\x1b[0m");
    println!("│  \x1b[1;37mHESAP & ÜYELİK AYARLARI\x1b[0m                                 │");
    println!("\x1b[1;36m├──────────────────────────────────────────────────────────┤\x1b[0m");
    let u_str = sanitize_terminal_str(&auth.spotify_user, 18, 64);
    let p_str = if auth.spotify_premium {
        "Aktif (320k)"
    } else {
        "Kapalı"
    };
    println!("│  Spotify: {:<18}  Premium: {:<16}  │", u_str, p_str);
    let oauth_status = if !auth.spotify_access_token.is_empty() {
        "Bağlı (OAuth Token)"
    } else {
        "Bağlı Değil"
    };
    let cid_preview = if !auth.spotify_client_id.is_empty() && !auth.spotify_client_id.contains('@') {
        let len = auth.spotify_client_id.len();
        if len > 8 {
            format!("{}...{}", &auth.spotify_client_id[..4], &auth.spotify_client_id[len - 4..])
        } else {
            auth.spotify_client_id.clone()
        }
    } else {
        "Varsayılan (Oma)".to_string()
    };
    println!("│  OAuth:   {:<18}  Client:  {:<16}  │", oauth_status, cid_preview);
    println!("\x1b[1;36m├──────────────────────────────────────────────────────────┤\x1b[0m");
    println!("│  [1] 🌐 Spotify Web Hesabını Bağla (Tek Tıkla Tarayıcı)  │");
    println!("│  [2] 🎧 Yerel Spotify Desktop Uygulamasını Başlat        │");
    println!("│  [3] 🔑 Özel Developer Client ID Tanımla                 │");
    println!("│  [4] 🍪 YouTube Music Premium Köprüsü                    │");
    println!("│  [5] ✕ Oturumları ve Token'ları Sıfırla                  │");
    println!("│  [0] ← Geri Dön                                          │");
    println!("\x1b[1;36m└──────────────────────────────────────────────────────────┘\x1b[0m\n");
    print!("  Seçiminiz [0-5]: ");
    let _ = io::stdout().flush();

    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_ok() {
        match line.trim() {
            "1" => {
                println!("\n  \x1b[1;36m--- SPOTIFY WEB HESABINI BAĞLA (OAUTH 2.0 PKCE) ---\x1b[0m");
                let client_id = if !auth.spotify_client_id.is_empty() && !auth.spotify_client_id.contains('@') {
                    auth.spotify_client_id.clone()
                } else {
                    DEFAULT_SPOTIFY_CLIENT_ID.to_string()
                };

                println!("  \x1b[1;32m✓\x1b[0m OmaPlayer resmi entegrasyonu hazırlandı.");
                match start_spotify_oauth(&client_id) {
                    Ok(new_auth) => {
                        println!("\n  \x1b[1;32m✓ SPOTIFY BAŞARIYLA BAĞLANDI!\x1b[0m");
                        println!("  Kullanıcı: \x1b[1;37m{}\x1b[0m", new_auth.spotify_user);
                        println!(
                            "  Üyelik:    \x1b[1;32m{}\x1b[0m",
                            if new_auth.spotify_premium {
                                "Premium (320 kbps)"
                            } else {
                                "Standart / Free"
                            }
                        );
                        std::thread::sleep(Duration::from_secs(2));
                    }
                    Err(e) => {
                        println!("\n  \x1b[1;31m✕ Yetkilendirme Başarısız:\x1b[0m {}", e);
                        std::thread::sleep(Duration::from_secs(2));
                    }
                }
            }
            "2" => {
                println!("\n  \x1b[1;36mYerel Spotify Desktop başlatılıyor...\x1b[0m");
                let _ = std::process::Command::new("/usr/bin/spotify").spawn();
                println!("  \x1b[1;32m✓ Spotify masaüstü uygulaması açıldı. OmaPlayer MPRIS ile otomatik bağlanacak.\x1b[0m");
                std::thread::sleep(Duration::from_millis(1500));
            }
            "3" => {
                println!("\n  \x1b[1;36m--- ÖZEL DEVELOPER CLIENT ID TANIMLA ---\x1b[0m");
                println!("  \x1b[1;33mNot: Normal kullanıcıların Client ID girmesine gerek yoktur (OmaPlayer varsayılan olarak hazırdır).\x1b[0m");
                println!("  \x1b[1;37mKendi Spotify Developer uygulamanızın 32 karakterlik Client ID'sini girin\x1b[0m");
                println!("  (Varsayılana dönmek için boş bırakıp Enter'a basın):\n");
                print!("  Client ID: ");
                let _ = io::stdout().flush();
                let mut input_cid = String::new();
                let _ = io::stdin().read_line(&mut input_cid);
                let trimmed = input_cid.trim();
                if trimmed.is_empty() {
                    auth.spotify_client_id.clear();
                    save_auth_data(&auth);
                    println!("\n  \x1b[1;32m✓ Varsayılan OmaPlayer Client ID'sine dönüldü.\x1b[0m");
                    std::thread::sleep(Duration::from_millis(1200));
                } else if trimmed.contains('@') {
                    println!("\n  \x1b[1;31m✕ HATA: E-posta adresi girdiniz!\x1b[0m");
                    println!("  Client ID e-posta adresi değildir. Spotify Developer Dashboard'dan alınan 32 haneli API anahtarıdır.");
                    println!("  E-posta yazmanıza gerek yoktur, [1]'e basarak doğrudan tarayıcı ile bağlanabilirsiniz.");
                    std::thread::sleep(Duration::from_secs(3));
                } else {
                    auth.spotify_client_id = trimmed.to_string();
                    save_auth_data(&auth);
                    println!("\n  \x1b[1;32m✓ Özel Client ID kaydedildi: {}\x1b[0m", trimmed);
                    std::thread::sleep(Duration::from_millis(1200));
                }
            }
            "4" => {
                auth.yt_cookies = true;
                save_auth_data(&auth);
                println!("\n  \x1b[1;32m✓ YouTube Music Premium köprüsü kaydedildi!\x1b[0m");
                std::thread::sleep(Duration::from_millis(800));
            }
            "5" => {
                auth.spotify_user = String::new();
                auth.spotify_premium = false;
                auth.spotify_client_id = String::new();
                auth.spotify_access_token = String::new();
                auth.spotify_refresh_token = String::new();
                auth.spotify_token_expires_at = 0;
                auth.yt_cookies = false;
                auth.qobuz_user = String::new();
                save_auth_data(&auth);
                println!("\n  \x1b[1;33m✓ Oturumlar ve erişim anahtarları sıfırlandı.\x1b[0m");
                std::thread::sleep(Duration::from_millis(800));
            }
            _ => {}
        }
    }
    guard.resume();
}

// ==============================================================================
// 5. MAIN EVENT LOOP
// ==============================================================================

fn main() {
    let mut guard = RawModeGuard::new().expect("Failed to initialize terminal raw mode");

    let wave_frames = [
        " ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ",
        "▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂",
        "▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃",
        "▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄",
        "▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅",
        "▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆",
        "▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇",
        "█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅▆▇█",
    ];

    let mut frame_idx = 0;
    let mut last_size = (0u16, 0u16);

    // Initial clear
    print!("\x1b[2J\x1b[H");
    let _ = io::stdout().flush();

    loop {
        let (cols, rows) = get_terminal_size();
        if (cols, rows) != last_size {
            print!("\x1b[2J\x1b[H");
            let _ = io::stdout().flush();
            last_size = (cols, rows);
        }

        let data = get_playback_info();
        let auth = get_auth_data();

        let output = if cols >= 92 && rows >= 18 {
            render_fullscreen_view(&data, &auth, &wave_frames, frame_idx, cols)
        } else {
            render_compact_view(&data, &auth, &wave_frames, frame_idx, cols)
        };

        print!("{}", output);
        let _ = io::stdout().flush();

        // Non-blocking key poll (250ms timeout)
        unsafe {
            let mut read_fds: libc::fd_set = std::mem::zeroed();
            libc::FD_ZERO(&mut read_fds);
            libc::FD_SET(libc::STDIN_FILENO, &mut read_fds);

            let mut tv = libc::timeval {
                tv_sec: 0,
                tv_usec: 250_000,
            };

            let ret = libc::select(
                libc::STDIN_FILENO + 1,
                &mut read_fds,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut tv,
            );

            if ret > 0 {
                let mut buf = [0u8; 16];
                let n = libc::read(libc::STDIN_FILENO, buf.as_mut_ptr() as *mut libc::c_void, 16);
                if n > 0 {
                    let key = buf[0];
                    match key {
                        b'q' | b'Q' | 0x03 | 0x1B => {
                            // Quit
                            break;
                        }
                        b' ' => {
                            control_playback("play_pause");
                        }
                        b'+' | b'=' => {
                            control_playback("vol_up");
                        }
                        b'-' | b'_' => {
                            control_playback("vol_down");
                        }
                        b'x' | b'X' => {
                            control_playback("stop");
                        }
                        b'1' => {
                            play_query_stream("Liked Songs Top Hits", "spotify");
                        }
                        b'2' => {
                            play_query_stream("YouTube Music Trending Top", "youtube");
                        }
                        b'3' => {
                            radio_dialog(&mut guard);
                        }
                        b'p' | b'P' => {
                            playlists_dialog(&mut guard);
                        }
                        b'5' | b'a' | b'A' => {
                            account_dialog(&mut guard);
                        }
                        b'f' | b'F' => {
                            search_dialog(&mut guard);
                        }
                        _ => {}
                    }
                }
            }
        }

        frame_idx = (frame_idx + 1) % wave_frames.len();
    }

    // Clean exit
    print!("\x1b[2J\x1b[H");
    let _ = io::stdout().flush();
}

use omaplayer::get_playback_info;

fn main() {
    let data = get_playback_info();

    let (text, css, tooltip) = if !data.is_running {
        (
            format!("󰎆 {}", data.source_name),
            "normal".to_string(),
            format!(
                "🎵 OmaPlayer • Evrensel Müzik Stüdyosu\n• Aktif Kaynak: {}\n• Durum: Çalmıyor\n\n[Sol Tık] Oynatıcıyı / Radyoyu Aç",
                data.source_name
            ),
        )
    } else if data.status == "PLAYING" {
        let track_str = if !data.artist.is_empty() {
            format!("{} • {}", data.title, data.artist)
        } else {
            data.title.clone()
        };
        let short_track: String = track_str.chars().take(26).collect();
        (
            format!("󰎆 {}", short_track),
            "active".to_string(),
            format!(
                "🎵 OmaPlayer • Çalıyor ({})\n• Parça: {}\n• Sanatçı: {}\n• Albüm: {}\n• Ses: %{}\n\n[Sol Tık] CLI Müzik Merkezini Aç",
                data.source_name, data.title, data.artist, data.album, data.volume_pct
            ),
        )
    } else if data.status == "PAUSED" {
        let short_title: String = data.title.chars().take(18).collect();
        (
            format!("󰎆 Duraklatıldı: {}", short_title),
            "normal".to_string(),
            format!(
                "🎵 OmaPlayer • Duraklatıldı ({})\n• Parça: {}\n• Sanatçı: {}\n\n[Sol Tık] CLI Müzik Merkezini Aç",
                data.source_name, data.title, data.artist
            ),
        )
    } else {
        (
            "󰎆 OmaPlayer".to_string(),
            "normal".to_string(),
            "🎵 OmaPlayer • Evrensel Müzik Çalar".to_string(),
        )
    };

    let res = serde_json::json!({
        "text": text,
        "tooltip": tooltip,
        "class": css
    });
    println!("{}", res);
}

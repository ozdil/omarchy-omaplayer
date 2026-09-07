use omaplayer::get_playback_info;

fn main() {
    let data = get_playback_info();

    let (text, css, tooltip) = if !data.is_running {
        (
            "MUSIC: IDLE".to_string(),
            "normal".to_string(),
            format!(
                "OmaPlayer Audio Studio\nSource: {}\nStatus: STOPPED\nEngine: Native Rust",
                data.source_name
            ),
        )
    } else if data.status == "PLAYING" {
        let track_str = if !data.artist.is_empty() {
            format!("{} - {}", data.title, data.artist)
        } else {
            data.title.clone()
        };
        let short_track: String = track_str.chars().take(22).collect();
        (
            format!("MUSIC: {}", short_track.to_uppercase()),
            "active".to_string(),
            format!(
                "OmaPlayer Audio Studio\nStatus: PLAYING ({})\nTrack: {}\nArtist: {}\nAlbum: {}\nVolume: {}%\nEngine: Native Rust",
                data.source_name, data.title, data.artist, data.album, data.volume_pct
            ),
        )
    } else if data.status == "PAUSED" {
        let short_title: String = data.title.chars().take(18).collect();
        (
            format!("MUSIC: PAUSED ({})", short_title.to_uppercase()),
            "normal".to_string(),
            format!(
                "OmaPlayer Audio Studio\nStatus: PAUSED ({})\nTrack: {}\nArtist: {}\nEngine: Native Rust",
                data.source_name, data.title, data.artist
            ),
        )
    } else {
        (
            "MUSIC: IDLE".to_string(),
            "normal".to_string(),
            "OmaPlayer Audio Studio\nStatus: IDLE\nEngine: Native Rust".to_string(),
        )
    };

    let res = serde_json::json!({
        "text": text,
        "tooltip": tooltip,
        "class": css
    });
    println!("{}", res);
}

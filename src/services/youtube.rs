use super::TrackItem;

pub fn search_youtube(query: &str) -> Vec<TrackItem> {
    let clean = query.trim();
    if clean.is_empty() {
        return Vec::new();
    }

    // High performance local query track builder
    vec![
        TrackItem {
            id: format!("yt_{}", clean),
            title: clean.to_string(),
            artist: "YouTube Music".to_string(),
            album: "Online Stream".to_string(),
            duration_sec: 210,
            cover_url: String::new(),
            uri: format!("ytdl://ytsearch1:{}", clean),
        }
    ]
}

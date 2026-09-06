use omaplayer::{
    control_playback, get_playback_info, get_radio_streams, list_local_music, play_query_stream,
    play_stream, MAX_BYTE_LIMIT,
};
use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                i += 1;
            }
            "--play-pause" => {
                control_playback("play_pause");
                i += 1;
            }
            "--stop" => {
                control_playback("stop");
                i += 1;
            }
            "--vol-up" => {
                control_playback("vol_up");
                i += 1;
            }
            "--vol-down" => {
                control_playback("vol_down");
                i += 1;
            }
            "--next" => {
                control_playback("next");
                i += 1;
            }
            "--prev" => {
                control_playback("prev");
                i += 1;
            }
            "--search" => {
                if i + 1 < args.len() {
                    play_query_stream(&args[i + 1], "spotify");
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--play-radio" => {
                if i + 1 < args.len() {
                    let st_id = &args[i + 1];
                    let radios = get_radio_streams();
                    let (url, name, genre) = if let Some(st) = radios.get(st_id.as_str()) {
                        (st.url, st.name, st.genre)
                    } else {
                        let lofi = &radios["lofi"];
                        (lofi.url, lofi.name, lofi.genre)
                    };
                    play_stream(url, name, genre, "radio");
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--play-file" => {
                if i + 1 < args.len() {
                    let f_path = &args[i + 1];
                    let p = Path::new(f_path);
                    if p.is_file() {
                        let f_name = p
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or("Yerel Dosya");
                        play_stream(f_path, f_name, "Yerel Müzik", "local");
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--list-local" => {
                let tracks = list_local_music();
                if let Ok(s) = serde_json::to_string(&tracks) {
                    println!("{}", s);
                }
                return;
            }
            _ => {
                i += 1;
            }
        }
    }

    let info = get_playback_info();
    let mut out_str = serde_json::to_string(&info).unwrap_or_else(|_| "{}".to_string());
    if out_str.len() > MAX_BYTE_LIMIT {
        out_str = serde_json::json!({
            "status": "STOPPED",
            "is_running": false,
            "source": "None",
            "source_name": "OmaPlayer",
            "title": "Müzik Çalmıyor",
            "artist": "OmaPlayer",
            "album": "",
            "volume_pct": 0,
            "service": null
        })
        .to_string();
    }
    println!("{}", out_str);
}

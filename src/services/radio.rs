use super::RadioStation;

pub fn get_curated_stations() -> Vec<RadioStation> {
    vec![
        RadioStation {
            id: "lofi".to_string(),
            name: "Lofi Beats Radio".to_string(),
            genre: "Chill / Study / Focus".to_string(),
            url: "https://stream.zeno.fm/f3wvbbqmdg8uv".to_string(),
        },
        RadioStation {
            id: "jazz".to_string(),
            name: "Jazz Radio Classics".to_string(),
            genre: "Classic & Vocal Jazz".to_string(),
            url: "https://jazz-wr01.ice.infomaniak.ch/jazz-wr01-128.mp3".to_string(),
        },
        RadioStation {
            id: "synthwave".to_string(),
            name: "Nightwave Plaza".to_string(),
            genre: "Synthwave / Cyberpunk / Vaporwave".to_string(),
            url: "https://plaza.one/mp3".to_string(),
        },
        RadioStation {
            id: "rock".to_string(),
            name: "Classic Rock Radio".to_string(),
            genre: "70s & 80s Rock Hits".to_string(),
            url: "https://stream.rockantenne.de/classic-perlen/stream/mp3".to_string(),
        },
        RadioStation {
            id: "trt".to_string(),
            name: "TRT Radyo 3".to_string(),
            genre: "Klasik Müzik, Caz & Kültür".to_string(),
            url: "https://radyo-trt.medya.trt.com.tr/live/trt_radyo_3.mp3".to_string(),
        },
    ]
}

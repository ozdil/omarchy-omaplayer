pub mod radio;
pub mod spotify;
pub mod youtube;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceKind {
    Spotify,
    YouTubeMusic,
    WebRadio,
    LocalLibrary,
}

impl ServiceKind {
    pub fn label(&self) -> &'static str {
        match self {
            ServiceKind::Spotify => "Spotify",
            ServiceKind::YouTubeMusic => "YouTube Music",
            ServiceKind::WebRadio => "Canlı Radyolar",
            ServiceKind::LocalLibrary => "Yerel Müzikler",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            ServiceKind::Spotify => "",
            ServiceKind::YouTubeMusic => "",
            ServiceKind::WebRadio => "",
            ServiceKind::LocalLibrary => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountTier {
    Premium,
    Free,
}

impl AccountTier {
    pub fn label(&self) -> &'static str {
        match self {
            AccountTier::Premium => "Premium (320 kbps Hi-Fi)",
            AccountTier::Free => "Free / Standart",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackItem {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_sec: u32,
    pub cover_url: String,
    pub uri: String,
}

impl TrackItem {
    pub fn duration_formatted(&self) -> String {
        let m = self.duration_sec / 60;
        let s = self.duration_sec % 60;
        format!("{}:{:02}", m, s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub track_count: u32,
    pub cover_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadioStation {
    pub id: String,
    pub name: String,
    pub genre: String,
    pub url: String,
}

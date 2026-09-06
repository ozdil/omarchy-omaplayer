use std::process::Command;
use std::thread;
use std::time::Duration;
use serde::{Deserialize, Serialize};

use super::{PlaylistItem, TrackItem};
use crate::{get_auth_data, refresh_spotify_token};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub display_name: String,
    pub email: String,
    pub product: String,
    pub id: String,
    pub avatar_url: String,
}

fn get_playlists_cache_path() -> std::path::PathBuf {
    crate::get_state_dir().join("playlists_cache.json")
}

fn curl_get(url: &str, access_token: &str) -> Result<serde_json::Value, String> {
    for attempt in 0..3 {
        let output = Command::new("/usr/bin/curl")
            .args([
                "-s",
                "-w",
                "\n%{http_code}",
                "-H",
                &format!("Authorization: Bearer {}", access_token),
                url,
            ])
            .output()
            .map_err(|e| format!("Curl isteği hatası: {}", e))?;

        let text = String::from_utf8_lossy(&output.stdout);
        let trimmed = text.trim_end();
        let (body, status_code) = if let Some(idx) = trimmed.rfind('\n') {
            let code_str = &trimmed[idx + 1..];
            let code: u32 = code_str.parse().unwrap_or(0);
            (&trimmed[..idx], code)
        } else {
            (trimmed, 0)
        };

        if status_code == 429 {
            if attempt < 2 {
                thread::sleep(Duration::from_secs(3));
                continue;
            }
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            if (200..300).contains(&status_code) {
                return Ok(json);
            } else if let Some(err_obj) = json.get("error") {
                let msg = err_obj.get("message").and_then(|v| v.as_str()).unwrap_or("Bilinmeyen hata");
                return Err(format!("Spotify API ({}): {}", status_code, msg));
            }
            return Ok(json);
        }
    }
    Err("API isteği yanıt vermedi veya limit aşıldı.".to_string())
}

pub fn fetch_profile(access_token: &str) -> Result<UserProfile, String> {
    let json = curl_get("https://api.spotify.com/v1/me", access_token)?;
    let mut display_name = json.get("display_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let email = json.get("email").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let product = json.get("product").and_then(|v| v.as_str()).unwrap_or("premium").to_string();
    let id = json.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();

    if display_name.trim().is_empty() {
        if !email.is_empty() {
            if let Some(prefix) = email.split('@').next() {
                display_name = prefix.to_string();
            }
        } else if !id.is_empty() {
            display_name = id.clone();
        } else {
            display_name = std::env::var("USER").unwrap_or_else(|_| "ozan".to_string());
        }
    }

    let avatar_url = json
        .get("images")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|img| img.get("url"))
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .to_string();

    Ok(UserProfile {
        display_name,
        email,
        product,
        id,
        avatar_url,
    })
}

pub fn fetch_playlists(access_token: &str) -> Result<Vec<PlaylistItem>, String> {
    let cache_file = get_playlists_cache_path();

    match curl_get("https://api.spotify.com/v1/me/playlists?limit=50", access_token) {
        Ok(json) => {
            if let Some(items) = json.get("items").and_then(|v| v.as_array()) {
                let mut res = Vec::new();
                for it in items {
                    let id = it.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = it.get("name").and_then(|v| v.as_str()).unwrap_or("İsimsiz Liste").to_string();
                    let owner = it
                        .get("owner")
                        .and_then(|o| o.get("display_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Spotify")
                        .to_string();

                    let track_count = it
                        .get("tracks")
                        .and_then(|t| t.get("total"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;

                    let cover_url = it
                        .get("images")
                        .and_then(|arr| arr.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|img| img.get("url"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    if !id.is_empty() {
                        res.push(PlaylistItem {
                            id,
                            name,
                            owner,
                            track_count,
                            cover_url,
                        });
                    }
                }
                if !res.is_empty() {
                    if let Ok(serialized) = serde_json::to_string_pretty(&res) {
                        let _ = std::fs::write(&cache_file, serialized);
                    }
                    return Ok(res);
                }
            }
        }
        Err(e) => {
            // Check cache fallback
            if cache_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&cache_file) {
                    if let Ok(cached) = serde_json::from_str::<Vec<PlaylistItem>>(&content) {
                        return Ok(cached);
                    }
                }
            }
            return Err(e);
        }
    }

    if cache_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&cache_file) {
            if let Ok(cached) = serde_json::from_str::<Vec<PlaylistItem>>(&content) {
                return Ok(cached);
            }
        }
    }

    Ok(Vec::new())
}

pub fn fetch_playlist_tracks(access_token: &str, playlist_id: &str) -> Result<Vec<TrackItem>, String> {
    let cache_dir = crate::get_state_dir().join("cache");
    let _ = std::fs::create_dir_all(&cache_dir);
    let cache_file = cache_dir.join(format!("tracks_{}.json", playlist_id));

    let cached_opt: Option<Vec<TrackItem>> = if cache_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&cache_file) {
            serde_json::from_str(&content).ok()
        } else {
            None
        }
    } else {
        None
    };

    let url = format!("https://api.spotify.com/v1/playlists/{}/tracks?limit=100", playlist_id);
    match curl_get(&url, access_token) {
        Ok(json) => {
            if let Some(items) = json.get("items").and_then(|v| v.as_array()) {
                let mut res = Vec::new();
                for it in items {
                    let track = if let Some(t) = it.get("track") { t } else { it };
                    let id = track.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let title = track.get("name").and_then(|v| v.as_str()).unwrap_or("Bilinmeyen Şarkı").to_string();

                    let mut artists = Vec::new();
                    if let Some(art_arr) = track.get("artists").and_then(|v| v.as_array()) {
                        for a in art_arr {
                            if let Some(name) = a.get("name").and_then(|v| v.as_str()) {
                                artists.push(name);
                            }
                        }
                    }
                    let artist_str = if artists.is_empty() { "Spotify".to_string() } else { artists.join(", ") };

                    let album = track
                        .get("album")
                        .and_then(|alb| alb.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let cover_url = track
                        .get("album")
                        .and_then(|alb| alb.get("images"))
                        .and_then(|arr| arr.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|img| img.get("url"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let duration_ms = track.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                    let uri = track.get("uri").and_then(|v| v.as_str()).unwrap_or("").to_string();

                    if !id.is_empty() {
                        res.push(TrackItem {
                            id,
                            title,
                            artist: artist_str,
                            album,
                            duration_sec: (duration_ms / 1000) as u32,
                            cover_url,
                            uri,
                        });
                    }
                }
                if !res.is_empty() {
                    if let Ok(serialized) = serde_json::to_string_pretty(&res) {
                        let _ = std::fs::write(&cache_file, serialized);
                    }
                    return Ok(res);
                }
            }
            if let Some(cached) = cached_opt {
                return Ok(cached);
            }
            Ok(Vec::new())
        }
        Err(e) => {
            if let Some(cached) = cached_opt {
                return Ok(cached);
            }
            Err(e)
        }
    }
}

pub fn fetch_liked_songs(access_token: &str) -> Result<Vec<TrackItem>, String> {
    let cache_dir = crate::get_state_dir().join("cache");
    let _ = std::fs::create_dir_all(&cache_dir);
    let cache_file = cache_dir.join("tracks_liked.json");

    let cached_opt: Option<Vec<TrackItem>> = if cache_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&cache_file) {
            serde_json::from_str(&content).ok()
        } else {
            None
        }
    } else {
        None
    };

    match curl_get("https://api.spotify.com/v1/me/tracks?limit=50", access_token) {
        Ok(json) => {
            if let Some(items) = json.get("items").and_then(|v| v.as_array()) {
                let mut res = Vec::new();
                for it in items {
                    let track = if let Some(t) = it.get("track") { t } else { it };
                    let id = track.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let title = track.get("name").and_then(|v| v.as_str()).unwrap_or("Bilinmeyen").to_string();

                    let mut artists = Vec::new();
                    if let Some(art_arr) = track.get("artists").and_then(|v| v.as_array()) {
                        for a in art_arr {
                            if let Some(name) = a.get("name").and_then(|v| v.as_str()) {
                                artists.push(name);
                            }
                        }
                    }
                    let artist_str = if artists.is_empty() { "Spotify".to_string() } else { artists.join(", ") };
                    let album = track.get("album").and_then(|a| a.get("name")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let cover_url = track
                        .get("album")
                        .and_then(|a| a.get("images"))
                        .and_then(|arr| arr.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|img| img.get("url"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let duration_ms = track.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                    let uri = track.get("uri").and_then(|v| v.as_str()).unwrap_or("").to_string();

                    if !id.is_empty() {
                        res.push(TrackItem {
                            id,
                            title,
                            artist: artist_str,
                            album,
                            duration_sec: (duration_ms / 1000) as u32,
                            cover_url,
                            uri,
                        });
                    }
                }
                if !res.is_empty() {
                    if let Ok(serialized) = serde_json::to_string_pretty(&res) {
                        let _ = std::fs::write(&cache_file, serialized);
                    }
                    return Ok(res);
                }
            }
            if let Some(cached) = cached_opt {
                return Ok(cached);
            }
            Ok(Vec::new())
        }
        Err(e) => {
            if let Some(cached) = cached_opt {
                return Ok(cached);
            }
            Err(e)
        }
    }
}

pub fn search_tracks(access_token: &str, query: &str) -> Result<Vec<TrackItem>, String> {
    let encoded_query = query.replace(' ', "%20");
    let url = format!("https://api.spotify.com/v1/search?type=track&limit=30&q={}", encoded_query);
    let json = curl_get(&url, access_token)?;

    let items = json
        .get("tracks")
        .and_then(|t| t.get("items"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Arama sonucu bulunamadı".to_string())?;

    let mut res = Vec::new();
    for track in items {
        let id = track.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let title = track.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let mut artists = Vec::new();
        if let Some(art_arr) = track.get("artists").and_then(|v| v.as_array()) {
            for a in art_arr {
                if let Some(name) = a.get("name").and_then(|v| v.as_str()) {
                    artists.push(name);
                }
            }
        }
        let artist_str = artists.join(", ");
        let album = track.get("album").and_then(|a| a.get("name")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let cover_url = track
            .get("album")
            .and_then(|a| a.get("images"))
            .and_then(|arr| arr.as_array())
            .and_then(|arr| arr.first())
            .and_then(|img| img.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let duration_ms = track.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
        let uri = track.get("uri").and_then(|v| v.as_str()).unwrap_or("").to_string();

        if !id.is_empty() {
            res.push(TrackItem {
                id,
                title,
                artist: artist_str,
                album,
                duration_sec: (duration_ms / 1000) as u32,
                cover_url,
                uri,
            });
        }
    }

    Ok(res)
}

pub fn ensure_valid_token() -> Result<String, String> {
    let mut auth = get_auth_data();
    if auth.spotify_access_token.is_empty() {
        return Err("Spotify hesabı henüz bağlanmamış.".to_string());
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Refresh if within 5 minutes of expiring
    if auth.spotify_token_expires_at <= now + 300 {
        refresh_spotify_token(&mut auth)?;
    }

    Ok(auth.spotify_access_token)
}

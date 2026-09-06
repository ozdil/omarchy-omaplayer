use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use eframe::egui::{self, Align, Color32, FontId, Layout, Pos2, Rect, RichText, Stroke, Vec2};

use super::theme::OmarchyTheme;
use crate::services::{
    radio::get_curated_stations,
    spotify::{self, UserProfile},
    youtube,
    AccountTier, PlaylistItem, RadioStation, ServiceKind, TrackItem,
};
use crate::{
    control_playback, get_auth_data, get_playback_info, play_query_stream, play_stream,
    play_track_stream, save_auth_data, set_volume, start_spotify_oauth, AuthData, PlaybackInfo,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentView {
    Services,
    Playlists,
    PlaylistDetail,
    LikedSongs,
    Radios,
    Search,
    Settings,
}

pub struct OmaPlayerApp {
    // Current Navigation
    pub current_view: CurrentView,
    pub selected_service: ServiceKind,
    pub selected_tier: AccountTier,

    // Dynamic Omarchy Theme
    pub theme: OmarchyTheme,
    pub last_theme_check: Instant,

    // Auth & Profile
    pub auth_data: AuthData,
    pub user_profile: Option<UserProfile>,
    pub is_authenticating: bool,
    pub auth_status_msg: String,
    pub auth_rx: Option<Receiver<Result<AuthData, String>>>,
    pub profile_rx: Option<Receiver<Result<UserProfile, String>>>,

    // Library Data
    pub playlists: Vec<PlaylistItem>,
    pub is_loading_playlists: bool,
    pub playlists_rx: Option<Receiver<Result<Vec<PlaylistItem>, String>>>,

    pub selected_playlist: Option<PlaylistItem>,
    pub playlist_tracks: Vec<TrackItem>,
    pub is_loading_tracks: bool,
    pub tracks_rx: Option<Receiver<Result<Vec<TrackItem>, String>>>,

    pub liked_songs: Vec<TrackItem>,
    pub is_loading_liked: bool,
    pub liked_rx: Option<Receiver<Result<Vec<TrackItem>, String>>>,

    // Queue & Continuous Playback
    pub queue: Vec<TrackItem>,
    pub queue_index: usize,

    // Radio
    pub radio_stations: Vec<RadioStation>,

    // Search
    pub search_query: String,
    pub search_results: Vec<TrackItem>,
    pub is_searching: bool,
    pub search_rx: Option<Receiver<Result<Vec<TrackItem>, String>>>,

    // Playback
    pub playback_info: PlaybackInfo,
    pub last_status_check: Instant,
    pub current_volume: u32,
    pub visualizer_phase: f32,

    // Notification toast
    pub toast_msg: Option<(String, Instant)>,
}

impl Default for OmaPlayerApp {
    fn default() -> Self {
        let auth = get_auth_data();
        let tier = if auth.spotify_premium {
            AccountTier::Premium
        } else {
            AccountTier::Free
        };

        let mut app = Self {
            current_view: if auth.spotify_access_token.is_empty() {
                CurrentView::Services
            } else {
                CurrentView::Playlists
            },
            selected_service: ServiceKind::Spotify,
            selected_tier: tier,

            theme: OmarchyTheme::new(),
            last_theme_check: Instant::now(),

            auth_data: auth.clone(),
            user_profile: None,
            is_authenticating: false,
            auth_status_msg: String::new(),
            auth_rx: None,
            profile_rx: None,

            playlists: Vec::new(),
            is_loading_playlists: false,
            playlists_rx: None,

            selected_playlist: None,
            playlist_tracks: Vec::new(),
            is_loading_tracks: false,
            tracks_rx: None,

            liked_songs: Vec::new(),
            is_loading_liked: false,
            liked_rx: None,

            queue: Vec::new(),
            queue_index: 0,

            radio_stations: get_curated_stations(),

            search_query: String::new(),
            search_results: Vec::new(),
            is_searching: false,
            search_rx: None,

            playback_info: get_playback_info(),
            last_status_check: Instant::now(),
            current_volume: 80,
            visualizer_phase: 0.0,

            toast_msg: None,
        };

        // If authenticated, automatically load initial profile and playlists
        if !app.auth_data.spotify_access_token.is_empty() {
            app.load_profile_and_playlists();
        }

        app
    }
}

impl OmaPlayerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let app = Self::default();
        cc.egui_ctx.set_visuals(app.theme.to_egui_visuals());
        app
    }

    pub fn set_toast(&mut self, msg: &str) {
        self.toast_msg = Some((msg.to_string(), Instant::now()));
    }

    pub fn start_spotify_connect_flow(&mut self) {
        self.is_authenticating = true;
        self.auth_status_msg = "Tarayıcı açılıyor... Lütfen Spotify'da oturumu onaylayın.".to_string();

        let (tx, rx) = channel();
        self.auth_rx = Some(rx);

        let client_id = if !self.auth_data.spotify_client_id.is_empty() {
            self.auth_data.spotify_client_id.clone()
        } else {
            crate::DEFAULT_SPOTIFY_CLIENT_ID.to_string()
        };

        thread::spawn(move || {
            let res = start_spotify_oauth(&client_id);
            let _ = tx.send(res);
        });
    }

    pub fn load_profile_and_playlists(&mut self) {
        let token = self.auth_data.spotify_access_token.clone();
        if token.is_empty() {
            return;
        }

        self.is_loading_playlists = true;
        let (tx, rx) = channel();
        self.playlists_rx = Some(rx);

        let tok = token.clone();
        thread::spawn(move || {
            let lists = spotify::fetch_playlists(&tok);
            let _ = tx.send(lists);
        });

        // Also fetch profile asynchronously
        let (p_tx, p_rx) = channel();
        self.profile_rx = Some(p_rx);
        let tok_prof = token.clone();
        thread::spawn(move || {
            let res = spotify::fetch_profile(&tok_prof);
            let _ = p_tx.send(res);
        });
    }

    pub fn load_playlist_tracks(&mut self, playlist: PlaylistItem) {
        let token = self.auth_data.spotify_access_token.clone();
        if token.is_empty() {
            return;
        }

        self.selected_playlist = Some(playlist.clone());
        self.is_loading_tracks = true;
        self.current_view = CurrentView::PlaylistDetail;

        let (tx, rx) = channel();
        self.tracks_rx = Some(rx);

        let pid = playlist.id.clone();
        thread::spawn(move || {
            let tracks = spotify::fetch_playlist_tracks(&token, &pid);
            let _ = tx.send(tracks);
        });
    }

    pub fn load_liked_songs(&mut self) {
        let token = self.auth_data.spotify_access_token.clone();
        if token.is_empty() {
            return;
        }

        self.is_loading_liked = true;
        self.current_view = CurrentView::LikedSongs;

        let (tx, rx) = channel();
        self.liked_rx = Some(rx);

        thread::spawn(move || {
            let tracks = spotify::fetch_liked_songs(&token);
            let _ = tx.send(tracks);
        });
    }

    pub fn perform_search(&mut self) {
        let q = self.search_query.trim().to_string();
        if q.is_empty() {
            return;
        }

        self.is_searching = true;
        self.current_view = CurrentView::Search;
        let (tx, rx) = channel();
        self.search_rx = Some(rx);

        let token = self.auth_data.spotify_access_token.clone();
        let service = self.selected_service;

        thread::spawn(move || {
            if service == ServiceKind::Spotify && !token.is_empty() {
                let res = spotify::search_tracks(&token, &q);
                let _ = tx.send(res);
            } else {
                let yt_tracks = youtube::search_youtube(&q);
                let _ = tx.send(Ok(yt_tracks));
            }
        });
    }

    pub fn play_track(&mut self, track: &TrackItem) {
        self.set_toast(&format!("Oynatılıyor: {} • {}", track.title, track.artist));
        play_track_stream(&track.title, &track.artist, "spotify");
        self.playback_info = get_playback_info();
    }

    pub fn play_radio(&mut self, st: &RadioStation) {
        self.set_toast(&format!("Canlı Yayın: {}", st.name));
        play_stream(&st.url, &st.name, &st.genre, "radio");
        self.playback_info = get_playback_info();
    }

    pub fn play_queue_index(&mut self, idx: usize) {
        if idx < self.queue.len() {
            self.queue_index = idx;
            let track = self.queue[idx].clone();
            self.play_track(&track);
        }
    }

    pub fn play_next(&mut self) {
        if !self.queue.is_empty() && self.queue_index + 1 < self.queue.len() {
            self.play_queue_index(self.queue_index + 1);
        } else {
            control_playback("next");
            self.playback_info = get_playback_info();
        }
    }

    pub fn play_prev(&mut self) {
        if !self.queue.is_empty() && self.queue_index > 0 {
            self.play_queue_index(self.queue_index - 1);
        } else {
            control_playback("prev");
            self.playback_info = get_playback_info();
        }
    }

    pub fn shuffle_queue(&mut self) {
        use std::time::SystemTime;
        let mut seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(123456789);

        let len = self.queue.len();
        if len > 1 {
            for i in (1..len).rev() {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let j = (seed % ((i + 1) as u64)) as usize;
                self.queue.swap(i, j);
            }
        }
    }

    pub fn play_current_playlist_all(&mut self, shuffle: bool) {
        if !self.playlist_tracks.is_empty() {
            self.queue = self.playlist_tracks.clone();
            if shuffle {
                self.shuffle_queue();
            }
            self.play_queue_index(0);
        } else if let Some(p) = self.selected_playlist.clone() {
            self.set_toast(&format!("Oynatılıyor: {}", p.name));
            play_query_stream(&format!("{} playlist", p.name), "spotify");
            self.playback_info = get_playback_info();
        }
    }

    pub fn play_playlist_immediately(&mut self, playlist: PlaylistItem) {
        self.load_playlist_tracks(playlist.clone());
        self.play_current_playlist_all(false);
    }

    fn check_async_messages(&mut self) {
        // 1. Auth check
        if let Some(ref rx) = self.auth_rx {
            if let Ok(res) = rx.try_recv() {
                self.is_authenticating = false;
                match res {
                    Ok(new_auth) => {
                        self.auth_data = new_auth;
                        self.set_toast(&format!("Spotify Bağlandı: {} (Premium)", self.auth_data.spotify_user));
                        self.load_profile_and_playlists();
                        self.current_view = CurrentView::Playlists;
                    }
                    Err(e) => {
                        self.auth_status_msg = format!("Hata: {}", e);
                        self.set_toast(&format!("Yetkilendirme Hatası: {}", e));
                    }
                }
                self.auth_rx = None;
            }
        }

        // 2. Profile check
        if let Some(ref rx) = self.profile_rx {
            if let Ok(res) = rx.try_recv() {
                if let Ok(prof) = res {
                    if !prof.display_name.is_empty() {
                        self.auth_data.spotify_user = prof.display_name.clone();
                    }
                    self.auth_data.spotify_email = prof.email.clone();
                    self.auth_data.spotify_id = prof.id.clone();
                    self.auth_data.spotify_premium = prof.product == "premium";
                    save_auth_data(&self.auth_data);
                    self.user_profile = Some(prof);
                }
                self.profile_rx = None;
            }
        }

        // 3. Playlists check
        if let Some(ref rx) = self.playlists_rx {
            if let Ok(res) = rx.try_recv() {
                self.is_loading_playlists = false;
                if let Ok(lists) = res {
                    self.playlists = lists;
                }
                self.playlists_rx = None;
            }
        }

        // 4. Tracks check
        if let Some(ref rx) = self.tracks_rx {
            if let Ok(res) = rx.try_recv() {
                self.is_loading_tracks = false;
                if let Ok(tr) = res {
                    self.playlist_tracks = tr;
                }
                self.tracks_rx = None;
            }
        }

        // 5. Liked check
        if let Some(ref rx) = self.liked_rx {
            if let Ok(res) = rx.try_recv() {
                self.is_loading_liked = false;
                if let Ok(tr) = res {
                    self.liked_songs = tr;
                }
                self.liked_rx = None;
            }
        }

        // 6. Search check
        if let Some(ref rx) = self.search_rx {
            if let Ok(res) = rx.try_recv() {
                self.is_searching = false;
                if let Ok(tr) = res {
                    self.search_results = tr;
                }
                self.search_rx = None;
            }
        }

        // 7. Playback status poll (throttled to every 800ms)
        if self.last_status_check.elapsed() > Duration::from_millis(800) {
            self.playback_info = get_playback_info();
            self.last_status_check = Instant::now();
        }
    }
}

impl eframe::App for OmaPlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Theme Hot-Reload: Check every 2 seconds if Omarchy system theme changed
        if self.last_theme_check.elapsed() > Duration::from_secs(2) {
            if self.theme.maybe_reload() {
                ui.ctx().set_visuals(self.theme.to_egui_visuals());
            }
            self.last_theme_check = Instant::now();
        }

        self.check_async_messages();

        // 30 FPS Visualizer animation only when audio is actively playing
        let is_playing = self.playback_info.status == "PLAYING";
        if is_playing {
            self.visualizer_phase += 0.12;
            if self.visualizer_phase > 100.0 {
                self.visualizer_phase = 0.0;
            }
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        } else if self.is_authenticating {
            ui.ctx().request_repaint_after(Duration::from_millis(150));
        }

        let theme = self.theme.clone();

        // TOP NAVIGATION & HEADER
        egui::Panel::top("top_panel")
            .frame(egui::Frame::new().fill(theme.background).inner_margin(egui::Margin::symmetric(14, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🎵 OmaPlayer").font(FontId::proportional(20.0)).strong().color(theme.accent));
                    ui.label(RichText::new("PRO HI-FI").font(FontId::proportional(11.0)).color(theme.fg_muted));

                    ui.separator();

                    // Search input in header
                    ui.label("🔍");
                    let search_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Şarkı, sanatçı veya albüm ara...")
                            .desired_width(260.0),
                    );
                    if search_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.perform_search();
                    }

                    if ui.button("Ara").clicked() {
                        self.perform_search();
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // User Account Pill
                        let user_name = if !self.auth_data.spotify_user.is_empty() {
                            &self.auth_data.spotify_user
                        } else {
                            "Giriş Yapılmadı"
                        };

                        let badge_text = format!("{} {} • {}", self.selected_service.icon(), user_name, self.selected_tier.label());
                        let badge_color = if !self.auth_data.spotify_access_token.is_empty() {
                            theme.accent
                        } else {
                            theme.fg_muted
                        };

                        if ui.button(RichText::new(badge_text).color(badge_color).strong()).clicked() {
                            self.current_view = CurrentView::Services;
                        }

                        // Theme indicator chip
                        ui.label(RichText::new(format!("🎨 {}", theme.name)).font(FontId::proportional(12.0)).color(theme.fg_muted));
                    });
                });
            });

        // BOTTOM PERSISTENT PLAYER BAR
        egui::Panel::bottom("bottom_player_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme.card_bg)
                    .stroke(Stroke::new(1.0, theme.card_border))
                    .inner_margin(egui::Margin::symmetric(14, 10)),
            )
            .show(ui, |ui| {
                ui.columns(3, |cols| {
                    // Col 1: Current Track Info
                    cols[0].vertical(|ui| {
                        let title_text = if !self.playback_info.title.is_empty() {
                            &self.playback_info.title
                        } else {
                            "Çalan parça yok"
                        };
                        let artist_text = if !self.playback_info.artist.is_empty() {
                            &self.playback_info.artist
                        } else {
                            "OmaPlayer Çalma Listesi Seçin"
                        };

                        ui.label(
                            RichText::new(title_text)
                                .font(FontId::proportional(14.0))
                                .strong()
                                .color(theme.fg_bright),
                        );
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(artist_text)
                                    .font(FontId::proportional(12.0))
                                    .color(theme.fg_muted),
                            );
                            if self.playback_info.is_running {
                                ui.label(
                                    RichText::new(format!("• {}", self.playback_info.source_name))
                                        .font(FontId::proportional(11.0))
                                        .color(theme.accent),
                                );
                            }
                        });
                    });

                    // Col 2: Player Controls & Seek Slider
                    cols[1].vertical_centered(|ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;

                            if ui.button(RichText::new("🔀").font(FontId::proportional(15.0))).clicked() {
                                self.shuffle_queue();
                                self.set_toast("Kuyruk karıştırıldı");
                            }

                            if ui.button(RichText::new("⏮").font(FontId::proportional(18.0))).clicked() {
                                self.play_prev();
                            }

                            let play_icon = if self.playback_info.status == "PLAYING" { "⏸" } else { "▶" };
                            let play_btn = egui::Button::new(
                                RichText::new(play_icon)
                                    .font(FontId::proportional(20.0))
                                    .color(if theme.is_dark { Color32::from_rgb(6, 10, 7) } else { Color32::WHITE })
                            )
                            .corner_radius(18.0)
                            .fill(theme.accent);

                            if ui.add(play_btn).clicked() {
                                control_playback("play_pause");
                                self.playback_info = get_playback_info();
                            }

                            if ui.button(RichText::new("⏭").font(FontId::proportional(18.0))).clicked() {
                                self.play_next();
                            }

                            if ui.button(RichText::new("⏹").font(FontId::proportional(15.0))).clicked() {
                                control_playback("stop");
                                self.playback_info = get_playback_info();
                            }
                        });

                        ui.add_space(3.0);
                        let pos = self.playback_info.position_sec;
                        let len = self.playback_info.length_sec.max(1);
                        let frac = (pos as f32 / len as f32).clamp(0.0, 1.0);
                        let pos_str = format!("{}:{:02}", pos / 60, pos % 60);
                        let len_str = format!("{}:{:02}", len / 60, len % 60);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new(pos_str).font(FontId::proportional(11.0)).color(theme.fg_muted));
                            ui.add(egui::ProgressBar::new(frac).desired_width(180.0));
                            ui.label(RichText::new(len_str).font(FontId::proportional(11.0)).color(theme.fg_muted));
                        });
                    });

                    // Col 3: Volume & Real-Time Waveform Visualizer
                    cols[2].vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("🔊");
                            let mut vol = self.current_volume as f32;
                            if ui.add_sized([90.0, 18.0], egui::Slider::new(&mut vol, 0.0..=100.0).show_value(false)).changed() {
                                self.current_volume = vol as u32;
                                set_volume(self.current_volume);
                            }
                            ui.label(RichText::new(format!("{}%", self.current_volume)).font(FontId::proportional(11.0)).color(theme.fg_muted));
                        });

                        ui.add_space(4.0);
                        // Real-time animated audio spectrum visualizer (themed)
                        let (rect, _resp) = ui.allocate_exact_size(Vec2::new(140.0, 20.0), egui::Sense::hover());
                        let painter = ui.painter_at(rect);
                        let num_bars = 14;
                        let bar_w = rect.width() / (num_bars as f32);

                        for i in 0..num_bars {
                            let h_factor = if is_playing {
                                let f = (self.visualizer_phase + (i as f32 * 0.48)).sin().abs();
                                0.2 + 0.8 * f
                            } else {
                                0.12
                            };
                            let h = rect.height() * h_factor;
                            let x = rect.min.x + (i as f32 * bar_w);
                            let y = rect.max.y - h;
                            let bar_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(bar_w - 2.0, h));
                            painter.rect_filled(bar_rect, 1.5, theme.accent);
                        }
                    });
                });
            });

        // LEFT NAVIGATION SIDEBAR
        egui::Panel::left("left_sidebar")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme.background)
                    .stroke(Stroke::new(1.0, theme.card_border))
                    .inner_margin(egui::Margin::symmetric(10, 12)),
            )
            .show(ui, |ui| {
                ui.set_width(210.0);
                ui.label(RichText::new("KÜTÜPHANEM").font(FontId::proportional(11.0)).color(theme.fg_muted).strong());
                ui.add_space(6.0);

                let render_nav_btn = |ui: &mut egui::Ui, is_active: bool, icon: &str, label: &str, theme: &OmarchyTheme| -> bool {
                    let fill = if is_active { theme.selection } else { Color32::TRANSPARENT };
                    let stroke = if is_active { Stroke::new(1.0, theme.accent) } else { Stroke::NONE };
                    let text_color = if is_active { theme.accent } else { theme.foreground };

                    let btn = egui::Button::new(
                        RichText::new(format!("{}  {}", icon, label))
                            .font(FontId::proportional(14.0))
                            .color(text_color)
                            .strong()
                    )
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(6.0);

                    ui.add_sized([190.0, 32.0], btn).clicked()
                };

                if render_nav_btn(ui, self.current_view == CurrentView::Playlists || self.current_view == CurrentView::PlaylistDetail, "📂", "Çalma Listelerim", &theme) {
                    self.current_view = CurrentView::Playlists;
                    if self.playlists.is_empty() {
                        self.load_profile_and_playlists();
                    }
                }

                ui.add_space(2.0);
                if render_nav_btn(ui, self.current_view == CurrentView::LikedSongs, "★", "Beğenilen Şarkılar", &theme) {
                    self.load_liked_songs();
                }

                ui.add_space(2.0);
                if render_nav_btn(ui, self.current_view == CurrentView::Radios, "📻", "Canlı Radyolar", &theme) {
                    self.current_view = CurrentView::Radios;
                }

                ui.add_space(2.0);
                if render_nav_btn(ui, self.current_view == CurrentView::Search, "🔍", "Müzik Arama", &theme) {
                    self.current_view = CurrentView::Search;
                }

                ui.add_space(16.0);
                ui.label(RichText::new("AYARLAR").font(FontId::proportional(11.0)).color(theme.fg_muted).strong());
                ui.add_space(6.0);

                if render_nav_btn(ui, self.current_view == CurrentView::Services, "⚙️", "Servis & Giriş", &theme) {
                    self.current_view = CurrentView::Services;
                }

                ui.add_space(2.0);
                if render_nav_btn(ui, self.current_view == CurrentView::Settings, "🛠", "Sistem & Ses", &theme) {
                    self.current_view = CurrentView::Settings;
                }

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Omarchy Linux Native").font(FontId::proportional(11.0)).color(theme.fg_muted));
                    ui.label(RichText::new(format!("Tema: {}", theme.name)).font(FontId::proportional(11.0)).color(theme.accent));
                });
            });

        // CENTRAL CONTENT AREA
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme.background).inner_margin(16.0))
            .show(ui, |ui| {
                // Toast notification
                if let Some((ref msg, t)) = self.toast_msg {
                    if t.elapsed() < Duration::from_secs(4) {
                        egui::Frame::new()
                            .fill(theme.selection)
                            .stroke(Stroke::new(1.0, theme.accent))
                            .corner_radius(6.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.label(RichText::new(format!("💡 {}", msg)).color(theme.fg_bright));
                            });
                        ui.add_space(8.0);
                    }
                }

                match self.current_view {
                    CurrentView::Playlists => self.render_playlists_view(ui),
                    CurrentView::PlaylistDetail => self.render_playlist_detail_view(ui),
                    CurrentView::LikedSongs => self.render_liked_songs_view(ui),
                    CurrentView::Radios => self.render_radios_view(ui),
                    CurrentView::Search => self.render_search_view(ui),
                    CurrentView::Services => self.render_services_view(ui),
                    CurrentView::Settings => self.render_settings_view(ui),
                }
            });
    }
}

impl OmaPlayerApp {
    fn render_services_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        ui.heading(RichText::new("Müzik Servisi Seçimi & Yetkilendirme").color(theme.fg_bright));
        ui.add_space(12.0);

        // 1. Service cards
        ui.label(RichText::new("1. Müzik Servisini Seçin:").font(FontId::proportional(14.0)).strong().color(theme.fg_bright));
        ui.add_space(6.0);

        let services = [
            (ServiceKind::Spotify, "Spotify", "Kişisel çalma listeleri, beğenilenler, 320 kbps akış"),
            (ServiceKind::YouTubeMusic, "YouTube Music", "Geniş müzik arşivi, anında canlı akış"),
            (ServiceKind::WebRadio, "Canlı Radyolar", "Lofi, Jazz, Synthwave, Rock istasyonları"),
            (ServiceKind::LocalLibrary, "Yerel Arşiv", "Bilgisayarınızdaki FLAC / MP3 dosyaları"),
        ];

        ui.horizontal(|ui| {
            for (s, title, desc) in services {
                let is_selected = self.selected_service == s;
                egui::Frame::new()
                    .fill(if is_selected { theme.selection } else { theme.card_bg })
                    .stroke(Stroke::new(if is_selected { 1.5 } else { 1.0 }, if is_selected { theme.accent } else { theme.card_border }))
                    .corner_radius(8.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_width(175.0);
                        ui.set_height(95.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("{} {}", s.icon(), title)).font(FontId::proportional(14.0)).strong().color(theme.fg_bright));
                            ui.add_space(4.0);
                            ui.label(RichText::new(desc).font(FontId::proportional(11.0)).color(theme.fg_muted));
                            ui.add_space(4.0);
                            if ui.button(if is_selected { "✓ Seçili" } else { "Seç" }).clicked() {
                                self.selected_service = s;
                            }
                        });
                    });
            }
        });

        ui.add_space(20.0);

        // 2. Account Tier Selection
        ui.label(RichText::new("2. Hesap Türünü Belirleyin:").font(FontId::proportional(14.0)).strong().color(theme.fg_bright));
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.radio_value(&mut self.selected_tier, AccountTier::Premium, "🌟 Premium (320 kbps Hi-Fi, Kişisel Çalma Listeleri)");
            ui.add_space(20.0);
            ui.radio_value(&mut self.selected_tier, AccountTier::Free, "🎵 Free / Standart (Standart Akış & Arama)");
        });

        ui.add_space(20.0);

        // 3. Connect & Authorize
        ui.label(RichText::new("3. İnternetten Bağlan ve Onayla:").font(FontId::proportional(14.0)).strong().color(theme.fg_bright));
        ui.add_space(6.0);

        match self.selected_service {
            ServiceKind::Spotify => {
                let is_connected = !self.auth_data.spotify_access_token.is_empty();

                if is_connected {
                    ui.label(
                        RichText::new(format!("✓ Spotify Hesabı Bağlı: {} ({})", self.auth_data.spotify_user, if self.auth_data.spotify_premium { "Premium" } else { "Free" }))
                            .font(FontId::proportional(14.0))
                            .color(theme.accent)
                            .strong(),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let btn = egui::Button::new(RichText::new("📂 Çalma Listelerime Git").font(FontId::proportional(14.0)).color(theme.fg_bright))
                            .fill(theme.selection)
                            .stroke(Stroke::new(1.0, theme.accent))
                            .corner_radius(6.0);

                        if ui.add(btn).clicked() {
                            self.current_view = CurrentView::Playlists;
                            if self.playlists.is_empty() {
                                self.load_profile_and_playlists();
                            }
                        }

                        if ui.button(RichText::new("🔄 Yeniden Bağlan").color(theme.red)).clicked() {
                            self.start_spotify_connect_flow();
                        }
                    });
                } else {
                    if self.is_authenticating {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("Tarayıcıda Spotify açıldı. Lütfen 'Kabul Et' butonuna tıklayın...").color(theme.accent));
                        });
                    } else {
                        let btn = egui::Button::new(RichText::new("🌐 Spotify Hesabını İnternetten Bağla").font(FontId::proportional(15.0)).strong().color(if theme.is_dark { Color32::from_rgb(6, 10, 7) } else { Color32::WHITE }))
                            .fill(theme.accent)
                            .corner_radius(8.0);

                        if ui.add(btn).clicked() {
                            self.start_spotify_connect_flow();
                        }
                    }
                }

                if !self.auth_status_msg.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new(&self.auth_status_msg).color(theme.fg_muted));
                }
            }
            ServiceKind::YouTubeMusic => {
                ui.label("YouTube Music doğrudan arama ve yüksek kaliteli akış için hazırdır.");
                if ui.button("🔍 YouTube Music'te Ara").clicked() {
                    self.current_view = CurrentView::Search;
                }
            }
            ServiceKind::WebRadio => {
                ui.label("Canlı internet radyoları için oturum açmaya gerek yoktur. Anında dinleyebilirsiniz.");
                if ui.button("📻 Canlı Radyo İstasyonlarını Aç").clicked() {
                    self.current_view = CurrentView::Radios;
                }
            }
            ServiceKind::LocalLibrary => {
                ui.label("Yerel müzik dizininiz taranmaya hazır.");
            }
        }
    }

    fn render_playlists_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Kişisel Spotify Çalma Listelerim").color(theme.fg_bright));
            if self.is_loading_playlists {
                ui.spinner();
                ui.label(RichText::new("Listeler yükleniyor...").color(theme.fg_muted));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("🔄 Yenile").clicked() {
                    self.load_profile_and_playlists();
                }
            });
        });
        ui.add_space(10.0);

        if self.playlists.is_empty() {
            if self.auth_data.spotify_access_token.is_empty() {
                ui.label("Spotify hesabınız bağlı değil. Lütfen önce Servis Seçimi bölümünden giriş yapın.");
                if ui.button("Giriş Yap").clicked() {
                    self.current_view = CurrentView::Services;
                }
            } else if !self.is_loading_playlists {
                ui.label("Çalma listesi bulunamadı veya henüz yüklenmedi.");
            }
            return;
        }

        let mut play_playlist_now = None;
        let mut clicked_playlist = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("playlists_grid")
                .num_columns(3)
                .spacing([14.0, 14.0])
                .show(ui, |ui| {
                    for (i, p) in self.playlists.iter().enumerate() {
                        egui::Frame::new()
                            .fill(theme.card_bg)
                            .stroke(Stroke::new(1.0, theme.card_border))
                            .corner_radius(8.0)
                            .inner_margin(14.0)
                            .show(ui, |ui| {
                                ui.set_width(230.0);
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("🎵").font(FontId::proportional(24.0)));
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(&p.name).font(FontId::proportional(15.0)).strong().color(theme.fg_bright));
                                            ui.label(RichText::new(format!("{} parça • {}", p.track_count, p.owner)).font(FontId::proportional(11.0)).color(theme.fg_muted));
                                        });
                                    });

                                    ui.add_space(10.0);
                                    ui.horizontal(|ui| {
                                        // Big prominent play button directly on playlist card
                                        let play_btn = egui::Button::new(
                                            RichText::new("▶ Oynat")
                                                .font(FontId::proportional(13.0))
                                                .strong()
                                                .color(if theme.is_dark { Color32::from_rgb(6, 10, 7) } else { Color32::WHITE }),
                                        )
                                        .fill(theme.accent)
                                        .corner_radius(4.0);

                                        if ui.add(play_btn).clicked() {
                                            play_playlist_now = Some(p.clone());
                                        }

                                        let view_btn = egui::Button::new(
                                            RichText::new("📋 Şarkılar")
                                                .font(FontId::proportional(13.0))
                                                .color(theme.fg_bright),
                                        )
                                        .fill(theme.selection)
                                        .corner_radius(4.0);

                                        if ui.add(view_btn).clicked() {
                                            clicked_playlist = Some(p.clone());
                                        }
                                    });
                                });
                            });

                        if (i + 1) % 3 == 0 {
                            ui.end_row();
                        }
                    }
                });
        });

        if let Some(p) = play_playlist_now {
            self.play_playlist_immediately(p);
        } else if let Some(p) = clicked_playlist {
            self.load_playlist_tracks(p);
        }
    }

    fn render_playlist_detail_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        if let Some(ref p) = self.selected_playlist.clone() {
            // Header Action Bar
            ui.horizontal(|ui| {
                if ui.button(RichText::new("← Geri").font(FontId::proportional(13.0))).clicked() {
                    self.current_view = CurrentView::Playlists;
                }

                ui.heading(RichText::new(&p.name).color(theme.fg_bright));

                if self.is_loading_tracks {
                    ui.spinner();
                    ui.label(RichText::new("Şarkılar yükleniyor...").color(theme.fg_muted));
                }
            });

            ui.add_space(6.0);

            // Playlist Control Bar with Play All & Shuffle buttons
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} parça • Sahibi: {}", p.track_count, p.owner)).color(theme.fg_muted));

                ui.add_space(16.0);

                let play_all_btn = egui::Button::new(
                    RichText::new("▶ Listeyi Başlat")
                        .font(FontId::proportional(14.0))
                        .strong()
                        .color(if theme.is_dark { Color32::from_rgb(6, 10, 7) } else { Color32::WHITE }),
                )
                .fill(theme.accent)
                .corner_radius(6.0);

                if ui.add(play_all_btn).clicked() {
                    self.play_current_playlist_all(false);
                }

                let shuffle_btn = egui::Button::new(
                    RichText::new("🔀 Karışık Çal")
                        .font(FontId::proportional(14.0))
                        .color(theme.fg_bright),
                )
                .fill(theme.selection)
                .stroke(Stroke::new(1.0, theme.accent))
                .corner_radius(6.0);

                if ui.add(shuffle_btn).clicked() {
                    self.play_current_playlist_all(true);
                }
            });

            ui.add_space(12.0);

            if self.playlist_tracks.is_empty() {
                if self.is_loading_tracks {
                    ui.label(RichText::new("Spotify'dan parça listesi alınıyor...").color(theme.fg_muted));
                } else {
                    ui.label(RichText::new("Parça listesi Spotify API tarafından henüz iletilmedi. Ancak yukarıdaki '▶ Listeyi Başlat' butonuna basarak doğrudan çalabilirsiniz!").color(theme.fg_muted));
                }
                return;
            }

            let mut to_play_idx = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("tracks_table")
                    .num_columns(5)
                    .spacing([14.0, 8.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new("#").strong().color(theme.fg_muted));
                        ui.label(RichText::new("Başlık").strong().color(theme.fg_bright));
                        ui.label(RichText::new("Sanatçı").strong().color(theme.fg_bright));
                        ui.label(RichText::new("Süre").strong().color(theme.fg_muted));
                        ui.label(RichText::new("Oynat").strong().color(theme.accent));
                        ui.end_row();

                        for (idx, tr) in self.playlist_tracks.iter().enumerate() {
                            let is_current = self.playback_info.title == tr.title;

                            ui.label(
                                RichText::new(format!("{}", idx + 1))
                                    .color(if is_current { theme.accent } else { theme.fg_muted }),
                            );

                            ui.label(
                                RichText::new(&tr.title)
                                    .color(if is_current { theme.accent } else { theme.fg_bright })
                                    .strong(),
                            );

                            ui.label(RichText::new(&tr.artist).color(theme.fg_muted));
                            ui.label(RichText::new(tr.duration_formatted()).color(theme.fg_muted));

                            let play_label = if is_current { "⏸ Çalıyor" } else { "▶ Çal" };
                            let btn = egui::Button::new(
                                RichText::new(play_label).color(if is_current {
                                    if theme.is_dark { Color32::from_rgb(6, 10, 7) } else { Color32::WHITE }
                                } else {
                                    theme.fg_bright
                                }),
                            )
                            .fill(if is_current { theme.accent } else { theme.selection })
                            .corner_radius(4.0);

                            if ui.add(btn).clicked() {
                                to_play_idx = Some(idx);
                            }

                            ui.end_row();
                        }
                    });
            });

            if let Some(idx) = to_play_idx {
                self.queue = self.playlist_tracks.clone();
                self.play_queue_index(idx);
            }
        }
    }

    fn render_liked_songs_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        ui.horizontal(|ui| {
            ui.heading(RichText::new("★ Beğenilen Şarkılarım").color(theme.fg_bright));
            if self.is_loading_liked {
                ui.spinner();
                ui.label(RichText::new("Şarkılar alınıyor...").color(theme.fg_muted));
            }

            if !self.liked_songs.is_empty() {
                ui.add_space(16.0);
                let play_all_btn = egui::Button::new(
                    RichText::new("▶ Tümünü Çal")
                        .font(FontId::proportional(14.0))
                        .strong()
                        .color(if theme.is_dark { Color32::from_rgb(6, 10, 7) } else { Color32::WHITE }),
                )
                .fill(theme.accent)
                .corner_radius(6.0);

                if ui.add(play_all_btn).clicked() {
                    self.queue = self.liked_songs.clone();
                    self.play_queue_index(0);
                }

                let shuffle_btn = egui::Button::new(
                    RichText::new("🔀 Karışık")
                        .font(FontId::proportional(14.0))
                        .color(theme.fg_bright),
                )
                .fill(theme.selection)
                .stroke(Stroke::new(1.0, theme.accent))
                .corner_radius(6.0);

                if ui.add(shuffle_btn).clicked() {
                    self.queue = self.liked_songs.clone();
                    self.shuffle_queue();
                    self.play_queue_index(0);
                }
            }
        });

        ui.add_space(12.0);

        if self.liked_songs.is_empty() && !self.is_loading_liked {
            ui.label(RichText::new("Beğenilen şarkı bulunamadı veya henüz yüklenmedi.").color(theme.fg_muted));
            return;
        }

        let mut to_play_idx = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("liked_table")
                .num_columns(5)
                .spacing([14.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label(RichText::new("#").strong().color(theme.fg_muted));
                    ui.label(RichText::new("Başlık").strong().color(theme.fg_bright));
                    ui.label(RichText::new("Sanatçı").strong().color(theme.fg_bright));
                    ui.label(RichText::new("Süre").strong().color(theme.fg_muted));
                    ui.label(RichText::new("Oynat").strong().color(theme.accent));
                    ui.end_row();

                    for (idx, tr) in self.liked_songs.iter().enumerate() {
                        let is_current = self.playback_info.title == tr.title;
                        ui.label(RichText::new(format!("{}", idx + 1)).color(if is_current { theme.accent } else { theme.fg_muted }));
                        ui.label(RichText::new(&tr.title).color(if is_current { theme.accent } else { theme.fg_bright }).strong());
                        ui.label(RichText::new(&tr.artist).color(theme.fg_muted));
                        ui.label(RichText::new(tr.duration_formatted()).color(theme.fg_muted));

                        let btn = egui::Button::new(RichText::new(if is_current { "⏸" } else { "▶ Çal" }).color(theme.fg_bright))
                            .fill(if is_current { theme.accent } else { theme.selection })
                            .corner_radius(4.0);

                        if ui.add(btn).clicked() {
                            to_play_idx = Some(idx);
                        }
                        ui.end_row();
                    }
                });
        });

        if let Some(idx) = to_play_idx {
            self.queue = self.liked_songs.clone();
            self.play_queue_index(idx);
        }
    }

    fn render_radios_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        ui.heading(RichText::new("📻 Canlı Küresel Web Radyoları").color(theme.fg_bright));
        ui.label(RichText::new("Kesintisiz yüksek kaliteli internet canlı yayınları (Giriş gerekmez)").color(theme.fg_muted));
        ui.add_space(14.0);

        let mut clicked_station = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("radios_grid")
                .num_columns(2)
                .spacing([16.0, 14.0])
                .show(ui, |ui| {
                    for (i, st) in self.radio_stations.iter().enumerate() {
                        egui::Frame::new()
                            .fill(theme.card_bg)
                            .stroke(Stroke::new(1.0, theme.card_border))
                            .corner_radius(8.0)
                            .inner_margin(14.0)
                            .show(ui, |ui| {
                                ui.set_width(360.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("📻").font(FontId::proportional(26.0)));
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(&st.name).font(FontId::proportional(15.0)).strong().color(theme.fg_bright));
                                        ui.label(RichText::new(&st.genre).font(FontId::proportional(12.0)).color(theme.fg_muted));
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        let is_current = self.playback_info.title == st.name;
                                        let btn = egui::Button::new(
                                            RichText::new(if is_current { "⏸ Yayında" } else { "▶ Dinle" })
                                                .strong()
                                                .color(if is_current {
                                                    if theme.is_dark { Color32::from_rgb(6, 10, 7) } else { Color32::WHITE }
                                                } else {
                                                    theme.fg_bright
                                                }),
                                        )
                                        .fill(if is_current { theme.accent } else { theme.selection })
                                        .corner_radius(6.0);

                                        if ui.add(btn).clicked() {
                                            clicked_station = Some(st.clone());
                                        }
                                    });
                                });
                            });

                        if (i + 1) % 2 == 0 {
                            ui.end_row();
                        }
                    }
                });
        });

        if let Some(st) = clicked_station {
            self.play_radio(&st);
        }
    }

    fn render_search_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        ui.heading(RichText::new("🔍 Müzik Arama").color(theme.fg_bright));
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text("Şarkı adı, sanatçı veya albüm yazın...")
                    .desired_width(320.0),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.perform_search();
            }

            let search_btn = egui::Button::new(
                RichText::new("Ara").color(if theme.is_dark { Color32::from_rgb(6, 10, 7) } else { Color32::WHITE })
            )
            .fill(theme.accent)
            .corner_radius(4.0);

            if ui.add(search_btn).clicked() {
                self.perform_search();
            }

            if self.is_searching {
                ui.spinner();
                ui.label(RichText::new("Aranıyor...").color(theme.fg_muted));
            }
        });

        ui.add_space(14.0);

        if self.search_results.is_empty() && !self.is_searching {
            ui.label(RichText::new("Arama yapmak için yukarıya bir terim yazın.").color(theme.fg_muted));
            return;
        }

        let mut to_play = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("search_table")
                .num_columns(5)
                .spacing([14.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label(RichText::new("#").strong().color(theme.fg_muted));
                    ui.label(RichText::new("Başlık").strong().color(theme.fg_bright));
                    ui.label(RichText::new("Sanatçı").strong().color(theme.fg_bright));
                    ui.label(RichText::new("Süre").strong().color(theme.fg_muted));
                    ui.label(RichText::new("İşlem").strong().color(theme.accent));
                    ui.end_row();

                    for (idx, tr) in self.search_results.iter().enumerate() {
                        ui.label(RichText::new(format!("{}", idx + 1)).color(theme.fg_muted));
                        ui.label(RichText::new(&tr.title).color(theme.fg_bright).strong());
                        ui.label(RichText::new(&tr.artist).color(theme.fg_muted));
                        ui.label(RichText::new(tr.duration_formatted()).color(theme.fg_muted));

                        let btn = egui::Button::new(RichText::new("▶ Çal").color(theme.fg_bright))
                            .fill(theme.selection)
                            .corner_radius(4.0);

                        if ui.add(btn).clicked() {
                            to_play = Some(tr.clone());
                        }
                        ui.end_row();
                    }
                });
        });

        if let Some(tr) = to_play {
            self.play_track(&tr);
        }
    }

    fn render_settings_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        ui.heading(RichText::new("Sistem & Ses Ayarları").color(theme.fg_bright));
        ui.add_space(12.0);

        egui::Frame::new()
            .fill(theme.card_bg)
            .stroke(Stroke::new(1.0, theme.card_border))
            .corner_radius(8.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Spotify Bağlantı Detayları").strong().color(theme.accent).font(FontId::proportional(15.0)));
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Görünen Ad:").strong().color(theme.fg_bright));
                    ui.label(RichText::new(&self.auth_data.spotify_user).color(theme.accent).strong());
                });

                if !self.auth_data.spotify_email.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("E-posta:").strong().color(theme.fg_bright));
                        ui.label(RichText::new(&self.auth_data.spotify_email).color(theme.fg_muted));
                    });
                }

                if !self.auth_data.spotify_id.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Kullanıcı ID:").strong().color(theme.fg_bright));
                        ui.label(RichText::new(&self.auth_data.spotify_id).monospace().color(theme.fg_muted));
                    });
                }

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Plan:").strong().color(theme.fg_bright));
                    ui.label(RichText::new(if self.auth_data.spotify_premium { "Spotify Premium (320 kbps Hi-Fi)" } else { "Free" }).color(theme.accent));
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Aktif Omarchy Teması:").strong().color(theme.fg_bright));
                    ui.label(RichText::new(&theme.name).color(theme.accent).strong());
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Client ID:").strong().color(theme.fg_bright));
                    ui.label(RichText::new(&self.auth_data.spotify_client_id).monospace().color(theme.fg_muted));
                });

                ui.add_space(16.0);

                if ui.button(RichText::new("✕ Oturumu ve Token'ları Sıfırla").color(theme.red)).clicked() {
                    self.auth_data.spotify_user.clear();
                    self.auth_data.spotify_email.clear();
                    self.auth_data.spotify_id.clear();
                    self.auth_data.spotify_access_token.clear();
                    self.auth_data.spotify_refresh_token.clear();
                    save_auth_data(&self.auth_data);
                    self.playlists.clear();
                    self.set_toast("Oturum sıfırlandı.");
                    self.current_view = CurrentView::Services;
                }
            });
    }
}

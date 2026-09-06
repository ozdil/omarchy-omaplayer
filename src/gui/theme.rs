use eframe::egui::{Color32, Stroke, Visuals};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct OmarchyTheme {
    pub name: String,
    pub is_dark: bool,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub selection: Color32,
    pub muted: Color32,
    pub background: Color32,
    pub card_bg: Color32,
    pub card_hover: Color32,
    pub card_border: Color32,
    pub foreground: Color32,
    pub fg_muted: Color32,
    pub fg_bright: Color32,
    pub red: Color32,
    pub green: Color32,
    pub purple: Color32,
    last_file_path: Option<PathBuf>,
    last_modified: Option<SystemTime>,
}

fn parse_hex(hex: &str, fallback: Color32) -> Color32 {
    let s = hex.trim().trim_matches('"').trim_matches('\'').trim_start_matches('#');
    if s.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) {
            return Color32::from_rgb(r, g, b);
        }
    }
    fallback
}

fn lighten_color(c: Color32, amount: u8) -> Color32 {
    Color32::from_rgb(
        c.r().saturating_add(amount),
        c.g().saturating_add(amount),
        c.b().saturating_add(amount),
    )
}

impl OmarchyTheme {
    pub fn new() -> Self {
        let mut theme = Self::default();
        theme.reload();
        theme
    }

    pub fn default() -> Self {
        // Default to Neuramancer CRT Phosphor palette
        let accent = Color32::from_rgb(0, 255, 102); // #00FF66
        let background = Color32::from_rgb(6, 10, 7); // #060A07
        let card_bg = Color32::from_rgb(14, 23, 17); // #0E1711
        let card_border = Color32::from_rgb(27, 59, 36); // #1B3B24
        let foreground = Color32::from_rgb(214, 255, 224); // #D6FFE0
        let fg_muted = Color32::from_rgb(72, 123, 86); // #487B56
        let selection = Color32::from_rgb(13, 51, 26); // #0D331A

        Self {
            name: "Neuramancer".to_string(),
            is_dark: true,
            accent,
            accent_hover: lighten_color(accent, 30),
            selection,
            muted: card_border,
            background,
            card_bg,
            card_hover: lighten_color(card_bg, 14),
            card_border,
            foreground,
            fg_muted,
            fg_bright: Color32::from_rgb(239, 255, 243),
            red: Color32::from_rgb(255, 51, 102),
            green: Color32::from_rgb(0, 255, 102),
            purple: Color32::from_rgb(147, 51, 234),
            last_file_path: None,
            last_modified: None,
        }
    }

    fn find_colors_file() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        let p1 = PathBuf::from(&home).join(".local/state/omarchy/current/theme/colors.toml");
        if p1.exists() {
            return Some(p1);
        }
        let p2 = PathBuf::from(&home).join(".config/omarchy/themes/neuramancer/colors.toml");
        if p2.exists() {
            return Some(p2);
        }
        None
    }

    pub fn maybe_reload(&mut self) -> bool {
        if let Some(path) = Self::find_colors_file() {
            if let Ok(metadata) = fs::metadata(&path) {
                if let Ok(mod_time) = metadata.modified() {
                    if self.last_file_path.as_ref() != Some(&path) || self.last_modified != Some(mod_time) {
                        return self.reload();
                    }
                }
            }
        }
        false
    }

    pub fn reload(&mut self) -> bool {
        let path = match Self::find_colors_file() {
            Some(p) => p,
            None => return false,
        };

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let metadata = fs::metadata(&path).ok();
        self.last_modified = metadata.and_then(|m| m.modified().ok());
        self.last_file_path = Some(path.clone());

        // Parse key-value pairs from colors.toml
        let mut map = std::collections::HashMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_lowercase();
                let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
                map.insert(key, val);
            }
        }

        let mode = map.get("mode").map(|s| s.as_str()).unwrap_or("dark");
        self.is_dark = mode != "light";

        if let Some(acc) = map.get("accent") {
            self.accent = parse_hex(acc, self.accent);
            self.accent_hover = lighten_color(self.accent, 25);
        }
        if let Some(bg) = map.get("background") {
            self.background = parse_hex(bg, self.background);
        }
        if let Some(lbg) = map.get("lighter_background") {
            self.card_bg = parse_hex(lbg, self.card_bg);
            self.card_hover = lighten_color(self.card_bg, 14);
        }
        if let Some(sel) = map.get("selection") {
            self.selection = parse_hex(sel, self.selection);
        }
        if let Some(mut_c) = map.get("muted") {
            self.muted = parse_hex(mut_c, self.muted);
            self.card_border = self.muted;
        }
        if let Some(fg) = map.get("foreground") {
            self.foreground = parse_hex(fg, self.foreground);
        }
        if let Some(dfg) = map.get("dark_foreground") {
            self.fg_muted = parse_hex(dfg, self.fg_muted);
        }
        if let Some(bfg) = map.get("bright_foreground") {
            self.fg_bright = parse_hex(bfg, self.fg_bright);
        }
        if let Some(r) = map.get("red") {
            self.red = parse_hex(r, self.red);
        }
        if let Some(g) = map.get("green") {
            self.green = parse_hex(g, self.green);
        }

        // Try reading theme name from theme.conf or path
        if let Some(parent) = path.parent() {
            let conf_path = parent.join("theme.conf");
            if let Ok(c) = fs::read_to_string(conf_path) {
                for line in c.lines() {
                    if let Some((k, v)) = line.split_once('=') {
                        if k.trim() == "display_name" || k.trim() == "name" {
                            self.name = v.trim().trim_matches('"').to_string();
                            break;
                        }
                    }
                }
            }
        }

        true
    }

    pub fn to_egui_visuals(&self) -> Visuals {
        let mut visuals = if self.is_dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        visuals.panel_fill = self.background;
        visuals.window_fill = self.background;
        visuals.extreme_bg_color = self.card_bg;

        visuals.widgets.noninteractive.bg_fill = self.card_bg;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.card_border);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.foreground);

        visuals.widgets.inactive.bg_fill = self.card_bg;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, self.card_border);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, self.foreground);

        visuals.widgets.hovered.bg_fill = self.card_hover;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, self.accent);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, self.fg_bright);

        visuals.widgets.active.bg_fill = self.selection;
        visuals.widgets.active.bg_stroke = Stroke::new(1.5, self.accent);
        visuals.widgets.active.fg_stroke = Stroke::new(1.5, self.accent);

        visuals.selection.bg_fill = self.selection;
        visuals.selection.stroke = Stroke::new(1.0, self.accent);

        visuals
    }
}

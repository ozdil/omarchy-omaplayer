# 🎵 OmaPlayer (`ozdil.omaplayer`)

[![Language: Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org)
[![Security: Hardened](https://img.shields.io/badge/Security-Strict%20Sanitization-brightgreen.svg)](#güvenlik-ve-arındırma)
[![Omarchy Plugin](https://img.shields.io/badge/Omarchy-Plugin-blue.svg)](https://omarchy.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**OmaPlayer**, Omarchy Linux için %100 saf **Rust** diliyle geliştirilmiş, ultra hızlı (<1ms), bellek güvenli ve çok kaynaklı **Terminal Müzik Çaları ve Canlı İnternet Radyo Stüdyosudur**.

---

## 🌟 Öne Çıkan Özellikler

- 🦀 **%100 Saf Rust Mimarisi**: Harici yorumlayıcı (Python vb.) bağımlılığı olmadan, doğrudan makine koduna derlenen kompakt ve ultra hızlı ELF ikili dosyaları (`omaplayer-engine`, `omaplayer-dashboard`, `omaplayer-status`).
- ⚡ **Sıfır Gecikme (<1ms)**: Doğrudan Unix domain soketi üzerinden yerel MPV IPC iletişimi.
- 🛡️ **Katı Terminal Arındırma & Güvenlik**: Ağdan ve MPV'den gelen tüm meta veriler (parça adı, sanatçı, akış başlığı) katı bayt sınırına tabi tutulur; tüm OSC (pano `\x1b]52`, köprü `\x1b]8`), CSI (imleç, ekran sıfırlama), C0/C1 kontrol kodları, Unicode Bidi yönlendirme karakterleri ve işaretleme parantezleri derleme anında arındırılır.
- 🎛️ **Çok Kaynaklı Destek**: Spotify, YouTube Music, Qobuz, yerel kütüphane ve 5 adet canlı 7/24 internet radyosu (Lofi, Jazz, Synthwave, Rock, TRT Radyo 3).
- 🖥️ **Çift Modlu TUI Arayüzü**:
  - **Kompakt Mod** (< 92 sütun): Şık HUD paneli, spektrum animasyonu ve ses göstergesi.
  - **Tam Ekran Stüdyo** (>= 92 sütun): 3 sütunlu profesyonel ses iş istasyonu ve donanım telemetrisi.

---

## 🌟 Desteklenen Müzik & Yayın Kaynakları

1. 󰓇 **Spotify:** Masaüstü ve web MPRIS oynatıcı entegrasyonu.
2. 󰗃 **YouTube Music:** Tarayıcı ve yerel medya kontrolü.
3. 🎵 **Qobuz (Hi-Res):** 24-bit stüdyo kalitesinde kayıpsız müzik desteği.
4. 🌊 **Tidal & SoundCloud:** Bağımsız ve kayıpsız ses akışları.
5. 📻 **Canlı İnternet Radyoları (Tamamen Ücretsiz):**
   - ☕ **Lofi Beats:** Kodlama ve çalışma için sakin arka plan müzikleri.
   - 🎷 **Jazz Radio Classics:** Klasik caz ve blues.
   - 🌆 **Nightwave Plaza:** Synthwave / Retrowave / Cyberpunk atmosferi.
   - 🎸 **Classic Rock Radio:** Rock efsaneleri.
   - 📻 **TRT Radyo 3:** Klasik müzik ve kültür yayınları.
6. 📁 **Yerel Müzik Arşivi:** `~/Music` klasöründeki FLAC, MP3, WAV dosyaları.

---

## ⚡ Klavye Kısayolları

- `[Space]` ➔ ⏯️ Oynat / Duraklat
- `[+]` / `[-]` ➔ 🔊 Ses Seviyesini Ayarla (%5 artır / azalt)
- `[x]` ➔ ⏹️ Durdur
- `[1]` ➔ 󰓇 Spotify Beğenilen Şarkılar
- `[2]` ➔ 󰗃 YouTube Music Popüler Listesi
- `[3]` ➔ 📻 Canlı Radyo İstasyonu Seçici
- `[p]` ➔ 📂 Kişisel Çalma Listeleri
- `[5]` ➔ ⚙️ Hesap & Üyelik Ayarları
- `[f]` ➔ 🔍 Şarkı / Sanatçı Arama
- `[q]` ➔ 🚪 Çıkış

---

## 🚀 Kurulum & Kaynaktan Derleme

### Hızlı Kurulum (Hazır İkililer)
```bash
git clone https://github.com/ozdil/omarchy-omaplayer.git ~/.config/omarchy/plugins/omaplayer
chmod +x ~/.config/omarchy/plugins/omaplayer/omaplayer-*
```

### Kaynaktan Derleme (Rust & Cargo)
```bash
cd ~/.config/omarchy/plugins/omaplayer
cargo build --release
cp target/release/omaplayer-* .
```

### Kaldırma
```bash
rm -rf ~/.config/omarchy/plugins/omaplayer
```

---

## 📜 Lisans
MIT License © 2026 Ozan Özdil ([@ozdil](https://github.com/ozdil))

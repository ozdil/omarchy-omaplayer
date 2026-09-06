#!/usr/bin/env python3
"""
Regression test suite for OmaPlayer terminal sanitization.
Validates mitigation against OSC/CSI escape injection, Bidi overrides, C0/C1 controls,
and overlong payload bounding in media metadata.
"""

import os
import sys
import unittest
import importlib.util

# Dynamically import omaplayer-dashboard
script_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
dash_path = os.path.join(script_dir, "omaplayer-dashboard")

spec = importlib.util.spec_from_loader("omaplayer_dashboard", importlib.machinery.SourceFileLoader("omaplayer_dashboard", dash_path))
dash = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dash)

sanitize_terminal_str = dash.sanitize_terminal_str
strip_ansi = dash.strip_ansi
render_compact_view = dash.render_compact_view
render_fullscreen_view = dash.render_fullscreen_view


class TestTerminalSanitization(unittest.TestCase):

    def test_osc52_clipboard_injection(self):
        """OSC 52 clipboard write sequence must be completely stripped."""
        payload = "\x1b]52;c;ZXhpdCAw\x07Malicious Track"
        cleaned = sanitize_terminal_str(payload)
        self.assertNotIn("\x1b]52", cleaned)
        self.assertNotIn("ZXhpdCAw", cleaned)
        self.assertEqual(cleaned, "Malicious Track")

    def test_osc8_hyperlink_injection(self):
        """OSC 8 terminal hyperlink sequences must be stripped."""
        payload = "\x1b]8;;https://evil.attacker.com/exploit\x1b\\Click for Free Premium\x1b]8;;\x1b\\"
        cleaned = sanitize_terminal_str(payload)
        self.assertNotIn("evil.attacker.com", cleaned)
        self.assertNotIn("\x1b]8", cleaned)
        self.assertEqual(cleaned, "Click for Free Premium")

    def test_csi_cursor_and_screen_clear_injection(self):
        """CSI escape sequences (screen clears, cursor repositioning) must be removed."""
        payload = "\x1b[2J\x1b[H\x1b[?25lEvil Track Title\x1b[10;20H"
        cleaned = sanitize_terminal_str(payload)
        self.assertNotIn("\x1b[", cleaned)
        self.assertEqual(cleaned, "Evil Track Title")

    def test_bidi_override_injection(self):
        """Unicode bidirectional controls (RLO, LRE, PDF, etc.) must be removed."""
        payload = "Normal Song \u202eReverseTitle\u202c • \u200e\u200f\u061c\u2066\u2067\u2068\u2069Artist"
        cleaned = sanitize_terminal_str(payload)
        for bidi_char in ["\u202e", "\u202c", "\u200e", "\u200f", "\u061c", "\u2066", "\u2067", "\u2068", "\u2069"]:
            self.assertNotIn(bidi_char, cleaned)
        self.assertEqual(cleaned, "Normal Song ReverseTitle • Artist")

    def test_c0_c1_del_control_characters(self):
        """All C0, C1 controls, ESC, and DEL must be stripped."""
        payload = "Track\x00\x07\x08\x0b\x0c\x0e\x0f\x1b\x7f\x80\x9b\x9fName"
        cleaned = sanitize_terminal_str(payload)
        for bad_code in range(0, 32):
            self.assertNotIn(chr(bad_code), cleaned)
        self.assertNotIn("\x7f", cleaned)
        for bad_code in range(128, 160):
            self.assertNotIn(chr(bad_code), cleaned)
        self.assertEqual(cleaned, "TrackName")

    def test_markup_and_delimiter_filtering(self):
        """HTML/XML tags, markup brackets, quotes, backslashes must be stripped."""
        payload = "<script>alert('pwn')</script> & \"quoted\" `backticks` \\backslash\\"
        cleaned = sanitize_terminal_str(payload)
        for char in ["<", ">", "&", "'", '"', "`", "\\"]:
            self.assertNotIn(char, cleaned)

    def test_strict_byte_and_char_bounding(self):
        """Overlong payloads (>64KB) must be bounded in bytes and character length."""
        huge_payload = "A" * 70000
        cleaned = sanitize_terminal_str(huge_payload, max_len=40, max_bytes=256)
        self.assertEqual(len(cleaned), 40)
        self.assertEqual(cleaned, "A" * 40)

    def test_end_to_end_dashboard_rendering_with_hostile_metadata(self):
        """End-to-end test rendering views with hostile OSC/CSI/Bidi metadata in all fields."""
        hostile_data = {
            "status": "PLAYING",
            "is_running": True,
            "source": "spotify",
            "source_name": "\x1b]8;;http://evil.com\x1b\\EvilSource\x1b]8;;\x1b\\",
            "title": "\x1b]52;c;ZXhpdCAw\x07EvilTitle \u202eRLO\u202c",
            "artist": "\x1b[2J\x1b[HInjectedArtist\x00\x07",
            "codec": "FLAC\x1b[31m",
            "bitrate_kbps": 320,
            "sample_rate": "48.0 kHz",
            "is_lossless": True,
            "quality_label": "LOSSLESS (\x1b]52;c;malicious\x07FLAC 24-bit)",
            "position_sec": 45,
            "length_sec": 210,
            "volume_pct": 85
        }
        auth = {"spotify_user": "\x1b]8;;http://evil.com\x1b\\attacker\x1b]8;;\x1b\\", "spotify_premium": True}
        wave_frames = [" ▂▃▄▅▆▇█"]

        # Render compact view
        compact_out = render_compact_view(hostile_data, auth, wave_frames, 0, 60)
        self.assertNotIn("]52;", compact_out)
        self.assertNotIn("http://evil.com", compact_out)
        self.assertNotIn("\u202e", compact_out)
        self.assertNotIn("\x1b[2J", compact_out)

        # Render fullscreen view
        fs_out = render_fullscreen_view(hostile_data, auth, wave_frames, 0, 110)
        self.assertNotIn("]52;", fs_out)
        self.assertNotIn("http://evil.com", fs_out)
        self.assertNotIn("\u202e", fs_out)
        self.assertNotIn("\x1b[2J", fs_out)


if __name__ == "__main__":
    unittest.main()

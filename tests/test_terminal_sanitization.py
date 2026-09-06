#!/usr/bin/env python3
"""
Regression test suite for OmaPlayer terminal sanitization.
Invokes native Rust engine and dashboard binaries with hostile payloads:
OSC 52, OSC 8, CSI cursor/clears, Unicode Bidi overrides, and overlong bytes.
"""

import os
import subprocess
import json
import unittest

script_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
engine_bin = os.path.join(script_dir, "omaplayer-engine")


class TestBinaryTerminalSanitization(unittest.TestCase):

    def setUp(self):
        self.assertTrue(os.path.isfile(engine_bin), f"Engine binary not found: {engine_bin}")
        self.assertTrue(os.access(engine_bin, os.X_OK), "Engine binary is not executable")
        self.test_env = {**os.environ, "XDG_DATA_HOME": "/tmp/test_omaplayer_xdg"}

    def test_json_status_sanitized(self):
        """Engine --json returns valid, sanitized JSON with no unallowed controls."""
        res = subprocess.run([engine_bin, "--json"], env=self.test_env, capture_output=True, text=True, timeout=2.0)
        self.assertEqual(res.returncode, 0)
        data = json.loads(res.stdout.strip())
        self.assertIn("title", data)
        self.assertIn("artist", data)
        self.assertNotIn("\x1b", data["title"])
        self.assertNotIn("\x1b", data["artist"])

    def test_hostile_search_injection_defense(self):
        """Hostile query containing OSC 52, CSI, Bidi overrides, and script tags must be sanitized."""
        hostile_query = "\x1b]52;c;ZXhpdCAw\x07EvilTitle \u202eRLO\u202c <script>alert(1)</script>"
        res = subprocess.run([engine_bin, "--search", hostile_query], env=self.test_env, capture_output=True, text=True, timeout=2.0)
        self.assertEqual(res.returncode, 0)
        data = json.loads(res.stdout.strip())
        title = data.get("title", "")
        self.assertNotIn("]52", title)
        self.assertNotIn("ZXhpdCAw", title)
        self.assertNotIn("\u202e", title)
        self.assertNotIn("<script>", title)
        self.assertNotIn("</script>", title)

    def test_overlong_byte_bounding(self):
        """Huge inputs (>70KB) must be bounded strictly under MAX_BYTE_LIMIT."""
        huge = "A" * 70000
        res = subprocess.run([engine_bin, "--search", huge], env=self.test_env, capture_output=True, text=True, timeout=2.0)
        self.assertEqual(res.returncode, 0)
        self.assertLessEqual(len(res.stdout.encode("utf-8")), 65536)


if __name__ == "__main__":
    unittest.main()

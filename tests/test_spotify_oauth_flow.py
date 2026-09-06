import unittest
import urllib.request
import subprocess
import time
import os

class TestSpotifyOAuthFlow(unittest.TestCase):
    def test_custom_callback_listener_8888(self):
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        bin_path = os.path.join(root, "target", "release", "omaplayer-engine")
        if not os.path.isfile(bin_path):
            bin_path = os.path.join(root, "target", "debug", "omaplayer-engine")

        p = subprocess.Popen(
            [bin_path, "--auth-spotify", "test_mock_custom_id"],
            cwd=root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True
        )

        time.sleep(0.3)
        try:
            with urllib.request.urlopen("http://127.0.0.1:8888/callback?code=TEST_MOCK_CODE&state=abc", timeout=5) as resp:
                self.assertEqual(resp.status, 200)
                html = resp.read().decode("utf-8")
                self.assertIn("OmaPlayer", html)
                self.assertIn("Yetkilendirme Başarılı", html)
        finally:
            out, err = p.communicate(timeout=5)
            self.assertIn("Onay kodu alındı", out)

    def test_default_oauth_listener_8989(self):
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        bin_path = os.path.join(root, "target", "release", "omaplayer-engine")
        if not os.path.isfile(bin_path):
            bin_path = os.path.join(root, "target", "debug", "omaplayer-engine")

        p = subprocess.Popen(
            [bin_path, "--auth-spotify", "d420a117a32841c2b3474932e49fb54b"],
            cwd=root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True
        )

        time.sleep(0.3)
        try:
            with urllib.request.urlopen("http://127.0.0.1:8989/login?code=TEST_DEFAULT_CODE&state=abc", timeout=5) as resp:
                self.assertEqual(resp.status, 200)
                html = resp.read().decode("utf-8")
                self.assertIn("OmaPlayer", html)
                self.assertIn("Yetkilendirme Başarılı", html)
        finally:
            out, err = p.communicate(timeout=5)
            self.assertIn("Onay kodu alındı", out)

if __name__ == "__main__":
    unittest.main()

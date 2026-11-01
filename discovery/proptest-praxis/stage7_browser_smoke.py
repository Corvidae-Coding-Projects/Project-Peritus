#!/usr/bin/env python3
"""Real isolated Chromium smoke checks; no external sites or user browser profile."""

import argparse
import hashlib
import importlib.metadata
import json
import platform
import subprocess
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument("--owner", type=Path,
                    default=ROOT / "crates/tools/peritus-tools-computer/src")
parser.add_argument("--revision", help="Read the three driver sources from an exact Git revision")
parser.add_argument("--assert-failed-pages", action="store_true",
                    help="Require non-2xx pages to fail without publishing a reading")
options = parser.parse_args()
if options.revision:
    _source_directory = tempfile.TemporaryDirectory()
    OWNER = Path(_source_directory.name)
    for name in ("browser.py", "driver.py", "desktop.py"):
        relative = f"crates/tools/peritus-tools-computer/src/{name}"
        (OWNER / name).write_bytes(subprocess.check_output(
            ["git", "-C", str(ROOT), "show", f"{options.revision}:{relative}"]))
else:
    OWNER = options.owner.resolve()
sys.path.insert(0, str(OWNER))
from browser import Browsers
from driver import image


class Pages(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/cooldown":
            self.send_response(302)
            self.send_header("Location", "/retry")
            self.end_headers()
            return
        status = {"/retry": 429, "/unknown": 599}.get(self.path, 200)
        body = (b'<html><title>Just a moment...</title>'
                b'<script src="/cdn-cgi/challenge-platform/check"></script></html>'
                if self.path == "/challenge_body" else
                b'<html><title>Page</title><button onclick="this.innerText=\'Clicked\'">'
                b'Button</button></html>')
        self.send_response(status)
        self.send_header("Content-Type", "text/html")
        if self.path == "/challenge_header":
            self.send_header("cf-mitigated", "challenge")
        if status == 429:
            self.send_header("Retry-After", "7")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


def main():
    server = ThreadingHTTPServer(("127.0.0.1", 0), Pages)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    counts = {"fresh_click": 0, "driver_navigation": 0, "closed_tab_recovery": 0,
              "http_200": 0, "redirect_http_429": 0, "unknown_http_599": 0,
              "challenge_body_200": 0, "challenge_header_200": 0}
    outcomes = {}
    try:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            browser = Browsers(directory, directory, image)
            try:
                url = lambda suffix: f"http://127.0.0.1:{server.server_port}{suffix}"
                first = browser.control({"action": "open", "url": url("/page")})
                assert first["success"] and first["url"] == url("/page")
                clicked = browser.control({"action": "click", "session": first["session"],
                    "observation": first["observation"], "ref": first["elements"][0]["ref"]})
                assert "Clicked" in clicked["text"]
                counts["fresh_click"] += 1
                navigated = browser.control({"action": "navigate", "session": first["session"],
                    "observation": clicked["observation"], "url": url("/second")})
                assert navigated["url"] == url("/second")
                counts["driver_navigation"] += 1
                browser.sessions[first["session"]]["page"].close()
                closed = browser.control({"action": "observe", "session": first["session"]})
                assert closed["state"] == "closed"
                recovered = browser.control({"action": "new_tab", "session": first["session"],
                    "observation": closed["observation"], "url": url("/page")})
                assert recovered["url"] == url("/page")
                counts["closed_tab_recovery"] += 1
                for path, status, key in [("/page", 200, "http_200"),
                    ("/cooldown", 429, "redirect_http_429"),
                    ("/unknown", 599, "unknown_http_599"),
                    ("/challenge_body", 200, "challenge_body_200"),
                    ("/challenge_header", 200, "challenge_header_200")]:
                    response = browser.read_web({"url": url(path), "wait_ms": 0})
                    actual_status = response.get("status", response.get("http_status"))
                    assert actual_status == status, (path, status, response)
                    if options.assert_failed_pages and path != "/page":
                        assert response["success"] is False and "path" not in response, response
                        if status == 429:
                            assert response.get("retry_after") == "7", response
                        if path.startswith("/challenge_"):
                            assert response.get("failure") == "challenge_required", response
                    if response["success"]:
                        assert Path(response["path"]).is_file()
                    else:
                        assert "path" not in response
                    outcomes[path] = {"success": response["success"],
                                      "http_status": actual_status,
                                      "failure": response.get("failure")}
                    counts[key] += 1
            finally:
                browser.shutdown()
    finally:
        server.shutdown()
    fingerprints = {name: hashlib.sha256((OWNER / name).read_bytes()).hexdigest()
                    for name in ("browser.py", "driver.py")}
    print(json.dumps({"status": "pass", "os": platform.system(),
        "python": platform.python_version(),
        "playwright": importlib.metadata.version("playwright"),
        "source_sha256": fingerprints, "source_revision": options.revision,
        "checks": counts, "outcomes": outcomes}, sort_keys=True))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Local HTTP boundary from HarnessBench rubrics to the native Peritus account router."""

from __future__ import annotations

import argparse
import json
import subprocess
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

MAX_REQUEST_BYTES = 32 * 1024 * 1024
LOCAL_TOKEN = "peritus-local-rubric"


class RubricHandler(BaseHTTPRequestHandler):
    agent: Path

    def do_GET(self) -> None:
        if self.path == "/healthz":
            self._respond(200, {"status": "ready"})
        else:
            self._respond(404, {"error": {"message": "not found"}})

    def do_POST(self) -> None:
        if self.path not in ("/chat/completions", "/v1/chat/completions"):
            self._respond(404, {"error": {"message": "not found"}})
            return
        if self.headers.get("Authorization") != f"Bearer {LOCAL_TOKEN}":
            self._respond(401, {"error": {"message": "invalid local token"}})
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            self._respond(400, {"error": {"message": "invalid content length"}})
            return
        if length <= 0 or length > MAX_REQUEST_BYTES:
            self._respond(413, {"error": {"message": "request size is outside the bound"}})
            return
        body = self.rfile.read(length)
        try:
            completed = subprocess.run(
                [str(self.agent), "rubric"],
                input=body,
                capture_output=True,
                timeout=190,
                check=False,
            )
        except subprocess.TimeoutExpired:
            self._respond(504, {"error": {"message": "native rubric request timed out"}})
            return
        if completed.returncode != 0:
            detail = completed.stderr.decode("utf-8", errors="replace")[:4000]
            self._respond(502, {"error": {"message": detail}})
            return
        try:
            response = json.loads(completed.stdout.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            self._respond(502, {"error": {"message": "native rubric response was malformed"}})
            return
        self._respond(200, response)

    def log_message(self, format: str, *args: object) -> None:
        print(f"[peritus-rubric] {self.address_string()} {format % args}", flush=True)

    def _respond(self, status: int, value: object) -> None:
        body = json.dumps(value, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--agent", type=Path, required=True)
    parser.add_argument("--port", type=int, default=8765)
    args = parser.parse_args()
    agent = args.agent.expanduser().resolve()
    if not agent.is_file():
        raise SystemExit(f"native agent does not exist: {agent}")
    RubricHandler.agent = agent
    server = ThreadingHTTPServer(("127.0.0.1", args.port), RubricHandler)
    print(f"[peritus-rubric] ready on http://127.0.0.1:{args.port}/v1", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()

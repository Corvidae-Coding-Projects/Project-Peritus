#!/usr/bin/env python3
"""Exercise the actual bundled controller and real browser, retaining observed HTML and receipts."""
import argparse
import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import threading
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--python', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--chromium-only', action='store_true', help='Hide system Chrome from PATH; use installed Playwright Chromium')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    driver = Path(__file__).resolve().parents[3] / 'crates/tools/peritus-tools-computer/src/driver.py'
    class Page(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path == '/missing':
                self.send_response(404)
                self.end_headers()
                return
            body = ('<title>Live compass</title><main id="result">Loading</main>'
                    '<script>setTimeout(()=>document.getElementById("result").textContent="BROWSER-CANARY-水-🦜",120)</script>').encode()
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        def log_message(self, *_): pass
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Page)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    env = dict(os.environ)
    if args.chromium_only:
        env['PATH'] = str(args.output / 'empty-path')
    log = (args.output / 'controller.log').open('w')
    process = subprocess.Popen([args.python, str(driver), 'serve', '--state', str(args.output), '--workspace', str(args.output)], env=env, stdout=log, stderr=log)
    serial = 0
    def call(method, params):
        nonlocal serial
        serial += 1
        ready = json.loads((args.output / 'ready.json').read_text())
        with socket.create_connection(('127.0.0.1', ready['port']), timeout=45) as connection:
            connection.settimeout(45)
            connection.sendall(json.dumps({'token':ready['token'], 'id':f'replay-{serial}', 'method':method, 'params':params}).encode() + b'\n')
            return json.loads(connection.makefile('rb').readline())
    try:
        deadline = time.monotonic() + 15
        while not (args.output / 'ready.json').exists():
            if process.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError((args.output / 'controller.log').read_text())
            time.sleep(.05)
        root = f'http://127.0.0.1:{server.server_port}'
        existing = call('browser', {'action':'open', 'url':root, 'visible':False})
        assert existing['success'], existing
        session = existing['session']
        observations = []
        for wait in [0, 250, 500]:
            result = call('web', {'url':root, 'wait_ms':wait})
            assert result['success'], result
            html = Path(result['path']).read_text()
            assert '<main id="result">BROWSER-CANARY-水-🦜</main>' in html
            observations.append({key:result[key] for key in ['url','status','bytes','sha256']})
        rejected = call('web', {'url':'file:///etc/passwd'})
        assert not rejected['success'] and 'HTTP(S)' in rejected['error']
        still_open = call('browser', {'action':'observe', 'session':session})
        assert still_open['success'] and still_open['session'] == session
        call('browser', {'action':'close', 'session':session})
        call('shutdown', {})
        process.wait(timeout=10)
        assert process.returncode == 0
        (args.output / 'evidence.json').write_text(json.dumps({'rendered':observations,'file_url_rejected':True,'existing_session_preserved':True,'chromium_only':args.chromium_only}, indent=2))
        print(json.dumps({'rendered':len(observations),'file_url_rejected':True,'existing_session_preserved':True,'output':str(args.output)}))
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
        log.close()
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == '__main__': main()

"""Authenticated persistent computer-control driver owned by a Peritus C2 process."""
import argparse
import hashlib
import hmac
import importlib.util
import json
import os
from pathlib import Path
import secrets
import signal
import shutil
import socket
import sys
import threading
import time

from browser import Browsers
from desktop import Desktops, birth

MAX_REQUEST = 128 * 1024
MAX_RESPONSE = 256 * 1024


def status():
    browser = None
    if importlib.util.find_spec('playwright') is not None:
        from playwright.sync_api import sync_playwright
        with sync_playwright() as engine:
            browser = engine.chromium.executable_path
    return {'success': True, 'python': sys.executable,
            'playwright': importlib.util.find_spec('playwright') is not None,
            'browser_executable': browser,
            'browser_installed': browser is not None and Path(browser).is_file(),
            'pillow': importlib.util.find_spec('PIL') is not None,
            'native_accessibility': importlib.util.find_spec('gi') is not None and sys.platform == 'linux',
            'x11_input': shutil.which('xdotool') is not None,
            'isolated_desktop': shutil.which('Xvfb') is not None,
            'wayland_screenshot': shutil.which('spectacle') is not None,
            'setup': 'peritus automation setup'}


def image(raw, target):
    from PIL import Image
    with Image.open(raw) as picture:
        original = picture.size
        picture.thumbnail((1280, 960))
        picture.convert('RGB').save(target, format='PNG', optimize=True)
        width, height = picture.size
    raw.unlink()
    data = target.read_bytes()
    if len(data) > 4 * 1024 * 1024:
        target.unlink()
        raise ValueError('Screenshot exceeds 4 MiB; reduce the target viewport')
    return {'path': str(target), 'media_type': 'image/png', 'sha256': hashlib.sha256(data).hexdigest(),
            'bytes': len(data), 'width': width, 'height': height, 'scale': width / original[0]}


def atomic(path, value):
    temporary = path.with_suffix('.new')
    with temporary.open('w', encoding='utf-8') as stream:
        json.dump(value, stream, ensure_ascii=False)
        stream.flush()
        os.fsync(stream.fileno())
    temporary.replace(path)


def serve(root, workspace, owner_pid=None):
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    receipts = root / 'receipts'
    receipts.mkdir(exist_ok=True, mode=0o700)
    browser = Browsers(root, workspace, image)
    desktop = Desktops(root, workspace, image)
    token = secrets.token_hex(32)
    server = socket.socket()
    server.bind(('127.0.0.1', 0))
    server.listen(4)
    def terminate(signum, frame):
        raise SystemExit(128 + signum)
    signal.signal(signal.SIGTERM, terminate)
    if owner_pid is not None and sys.platform == 'linux':
        owner_birth = birth(owner_pid)
        def watch_owner():
            while True:
                time.sleep(1)
                try:
                    if birth(owner_pid) == owner_birth:
                        continue
                except FileNotFoundError:
                    pass
                # A controller must not become an orphaned browser/desktop owner.
                # SIGTERM enters the same finally cleanup as an explicit shutdown.
                os.kill(os.getpid(), signal.SIGTERM)
                return
        threading.Thread(target=watch_owner, daemon=True).start()
    os.umask(0o077)
    atomic(root / 'ready.json', {'port': server.getsockname()[1], 'token': token, 'pid': os.getpid()})
    try:
        while True:
            connection, _ = server.accept()
            stop = False
            with connection:
                connection.settimeout(5)
                try:
                    line = connection.makefile('rb').readline(MAX_REQUEST + 1)
                    if len(line) > MAX_REQUEST or not line.endswith(b'\n'):
                        raise ValueError('Request exceeds its frame bound')
                    request = json.loads(line)
                    if not hmac.compare_digest(request.get('token', ''), token):
                        raise ValueError('Unauthorized automation client')
                    identifier = request['id']
                    if not isinstance(identifier, str) or not identifier or len(identifier) > 256:
                        raise ValueError('Invalid operation identity')
                    key = hashlib.sha256(identifier.encode()).hexdigest()
                    receipt = receipts / (key + '.json')
                    method = request['method']
                    params = request['params']
                    digest = hashlib.sha256(json.dumps([method, params], sort_keys=True).encode()).hexdigest()
                    if receipt.exists() and method != 'web':
                        retained = json.loads(receipt.read_text())
                        if retained['digest'] != digest:
                            raise ValueError('Operation identity was reused with different arguments')
                        result = retained.get('result', {'success': False, 'error': 'Operation outcome is uncertain; observe fresh state instead of repeating the action', 'uncertain': True})
                    else:
                        atomic(receipt, {'digest': digest, 'state': 'pending'})
                        try:
                            if method == 'browser':
                                result = browser.control(params)
                            elif method == 'web':
                                result = browser.read_web(params)
                            elif method == 'desktop':
                                result = desktop.control(params)
                            elif method == 'status':
                                result = status()
                            elif method == 'shutdown':
                                result = {'success': True, 'closed': True}
                                stop = True
                            else:
                                raise ValueError('Unknown computer-control method')
                        except Exception as error:
                            result = {'success': False, 'error': str(error)[:3000], 'recovery': 'Observe the selected session before retrying; the previous action may have partially executed'}
                        atomic(receipt, {'digest': digest, 'state': 'complete', 'result': result})
                    encoded = json.dumps(result, ensure_ascii=False).encode() + b'\n'
                    if len(encoded) > MAX_RESPONSE:
                        encoded = json.dumps({'success': False, 'error': 'Observation exceeds 256 KiB; narrow the target interface'}).encode() + b'\n'
                    try:
                        connection.sendall(encoded)
                    except OSError:
                        # A caller may disconnect during a real action. Its completed
                        # receipt remains replayable; do not kill unrelated live sessions.
                        pass
                except Exception as error:
                    try:
                        connection.sendall(json.dumps({'success': False, 'error': str(error)[:3000]}).encode() + b'\n')
                    except OSError:
                        pass
            if stop:
                break
    finally:
        browser.shutdown()
        desktop.shutdown()
        server.close()
        (root / 'ready.json').unlink(missing_ok=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('operation', choices=['status', 'serve'])
    parser.add_argument('--state', type=Path)
    parser.add_argument('--workspace', type=Path)
    parser.add_argument('--owner-pid', type=int)
    arguments = parser.parse_args()
    if arguments.operation == 'status':
        print(json.dumps(status()))
    else:
        if arguments.state is None or arguments.workspace is None:
            parser.error('serve requires --state and --workspace')
        serve(arguments.state.resolve(), arguments.workspace.resolve(), arguments.owner_pid)

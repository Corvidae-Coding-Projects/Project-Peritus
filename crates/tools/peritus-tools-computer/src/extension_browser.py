"""Private loopback Chrome-extension backend for Peritus browser sessions."""
import base64
import json
import os
from pathlib import Path
import socket


MAX_REQUEST = 128 * 1024
MAX_RESPONSE = 4 * 1024 * 1024


class ExtensionBrowsers:
    def __init__(self, root, screenshot):
        self.root = root
        self.screenshot = screenshot
        self.sessions = set()

    def available(self):
        return bool(os.environ.get('PERITUS_CHROME_EXTENSION_SOCKET') and
                    (os.environ.get('PERITUS_CHROME_EXTENSION_TOKEN_FILE') or
                     os.environ.get('PERITUS_CHROME_EXTENSION_TOKEN')))

    def request(self, args):
        path = os.environ.get('PERITUS_CHROME_EXTENSION_SOCKET')
        token = os.environ.get('PERITUS_CHROME_EXTENSION_TOKEN')
        token_file = os.environ.get('PERITUS_CHROME_EXTENSION_TOKEN_FILE')
        if token is None and token_file:
            candidate = Path(token_file)
            if candidate.is_symlink() or not candidate.is_file():
                raise ValueError('The Chrome extension bridge token file is unavailable')
            token = candidate.read_text(encoding='ascii').strip()
        if not path or not token:
            raise ValueError('The private Chrome extension is not installed; run peritus automation chrome-extension install')
        payload = dict(args)
        payload['_token'] = token
        encoded = json.dumps(payload, ensure_ascii=False).encode('utf-8') + b'\n'
        if len(encoded) > MAX_REQUEST:
            raise ValueError('Chrome extension request exceeds its byte limit')
        connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        connection.settimeout(45)
        try:
            connection.connect(path)
            connection.sendall(encoded)
            chunks = bytearray()
            while not chunks.endswith(b'\n') and len(chunks) <= MAX_RESPONSE:
                part = connection.recv(min(65536, MAX_RESPONSE + 1 - len(chunks)))
                if not part:
                    break
                chunks.extend(part)
        except (FileNotFoundError, ConnectionRefusedError, socket.timeout, OSError) as error:
            raise ValueError(f'Chrome extension bridge is unavailable: {error}; enable the unpacked Peritus extension in Chrome') from error
        finally:
            connection.close()
        if len(chunks) > MAX_RESPONSE or not chunks.endswith(b'\n'):
            raise ValueError('Chrome extension response was oversized or interrupted')
        response = json.loads(chunks)
        if not response.get('success'):
            raise ValueError(response.get('error', 'Chrome extension action failed'))
        return response

    def control(self, args):
        result = self.request(args)
        identifier = result.get('session')
        if identifier:
            self.sessions.add(identifier)
        if args.get('action') == 'close' and args.get('session'):
            self.sessions.discard(args['session'])
        screenshot = result.pop('screenshot', None)
        if screenshot is not None:
            if not isinstance(screenshot, str) or not screenshot.startswith('data:image/jpeg;base64,'):
                raise ValueError('Chrome extension returned an invalid screenshot')
            try:
                data = base64.b64decode(screenshot.split(',', 1)[1], validate=True)
            except (ValueError, TypeError) as error:
                raise ValueError('Chrome extension returned malformed screenshot bytes') from error
            if len(data) > MAX_RESPONSE:
                raise ValueError('Chrome extension screenshot exceeds 4 MiB')
            safe = identifier.removeprefix('extension:')
            directory = self.root / ('extension-' + safe)
            directory.mkdir(mode=0o700, exist_ok=True)
            observation = result.get('observation', 'observation')
            raw = directory / ('raw-' + observation + '.jpg')
            raw.write_bytes(data)
            result['image'] = self.screenshot(raw, directory / (observation + '.png'))
        return result

    def shutdown(self):
        for identifier in list(self.sessions):
            try:
                self.request({'action': 'close', 'session': identifier})
            except Exception:
                pass
        self.sessions.clear()

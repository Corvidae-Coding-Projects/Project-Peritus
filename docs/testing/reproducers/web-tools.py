#!/usr/bin/env python3
"""Operate the built Peritus web CLI against real HTTP documents with independent byte oracles.

No SearXNG implementation is substituted: live search is a separate real-service journey.
"""
import argparse
import concurrent.futures
import gzip
import hashlib
import http.server
import json
import os
from pathlib import Path
import random
import subprocess
import threading


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, XDG_DATA_HOME=str(args.output / 'data'))
    env.pop('PERITUS_WEB_SEARCH_URL', None)
    binary = str(args.binary.resolve())
    documents = {}
    requests = []
    cases = []
    rng = random.Random(20261017)

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            requests.append(self.path)
            if self.path == '/redirect':
                self.send_response(302)
                self.send_header('Location', '/html')
                self.end_headers()
                return
            if self.path in ('/loop', '/unsafe-redirect'):
                self.send_response(302)
                self.send_header('Location', self.path if self.path == '/loop' else 'file:///etc/passwd')
                self.end_headers()
                return
            if self.path == '/oversized':
                self.send_response(200)
                self.send_header('Content-Type', 'text/plain')
                self.send_header('Content-Length', str(64 * 1024 * 1024 + 1))
                self.end_headers()
                return
            if self.path in ('/missing', '/limited'):
                self.send_response(404 if self.path == '/missing' else 429)
                self.send_header('Retry-After', '2')
                self.end_headers()
                return
            body, media, zipped = documents.get(self.path, (b'not found', 'text/plain', False))
            self.send_response(200)
            self.send_header('Content-Type', media)
            if zipped:
                body = gzip.compress(body)
                self.send_header('Content-Encoding', 'gzip')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_):
            pass

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    root = f'http://127.0.0.1:{server.server_port}'

    def run(*parts, success=True):
        result = subprocess.run([binary, '--json', 'web', *map(str, parts)], env=env,
                                text=True, capture_output=True, timeout=45)
        if success:
            assert result.returncode == 0, (parts, result.stderr)
            return json.loads(result.stdout)
        assert result.returncode != 0, (parts, result.stdout)
        return json.loads(result.stderr)['error']['message']

    try:
        status = run('status')
        assert not status['search_ready']
        assert 'not configured' in run('search', 'Rust ownership', success=False)
        cases.append('unconfigured search leaves native reading available')
        alphabet = ['a', 'Z', ' ', '\n', 'é', '水', '🦜', '\t']
        for index, length in enumerate([0, 1, 4, 7, 31, 80, 257, 4096, 65539, 1800000]):
            text = ''.join(rng.choice(alphabet) for _ in range(length)) + f'\nBEACON-{index}-END'
            encoded = text.encode()
            path = f'/document-{index}'
            documents[path] = (encoded, 'text/plain; charset=utf-8', bool(index % 2))
            opened = run('open', root + path)
            assert opened['raw_sha256'] == hashlib.sha256(encoded).hexdigest()
            reference = opened['source']
            max_bytes = 65537 if length > 65536 else [4, 7, 257, 65537][min(index // 3, 3)]
            reconstructed = ''
            offset = 0
            pages = 0
            while True:
                page = run('read', reference, offset, max_bytes)
                assert page['source'] == reference and page['offset'] == offset
                assert len(page['text'].encode()) <= max_bytes
                assert page['start_line'] == 1 + encoded[:offset].count(b'\n')
                reconstructed += page['text']
                pages += 1
                if page['next_offset'] is None:
                    break
                assert page['next_offset'] > offset
                offset = page['next_offset']
                assert pages < 200
            assert reconstructed == text, (index, len(reconstructed), len(text))
            hits = run('find', reference, f'BEACON-{index}-END')
            assert hits['matches'][0]['offset'] == encoded.index(f'BEACON-{index}-END'.encode())
            again = run('open', root + path)
            assert again['cached'] and again['source'] == reference
            assert requests.count(path) == 1
            cases.append({'generated_document': index, 'bytes': len(encoded), 'pages': pages, 'gzip': bool(index % 2)})

        html = '<!doctype html><title>Harbor &amp; Wind</title><style>HIDDEN_STYLE</style><h1>Harbor</h1><pre>  keep indent\n水 🦜</pre><script>HIDDEN_SCRIPT</script><p><a href="/target">Target source</a></p>'
        documents['/html'] = (html.encode(), 'text/html; charset=utf-8', False)
        opened = run('open', root + '/redirect')
        assert opened['url'] == root + '/html' and opened['title'] == 'Harbor & Wind'
        assert 'HIDDEN_' not in opened['text']
        assert '  keep indent\n水 🦜' in opened['text']
        assert opened['links'][0]['url'] == root + '/target'
        assert opened['scripts_present']
        reference = opened['source']
        # Refresh changes the current snapshot without rewriting previous references.
        documents['/changing'] = (b'first retained value', 'text/plain', False)
        first = run('open', root + '/changing')
        documents['/changing'] = (b'second current value', 'text/plain', False)
        assert run('open', root + '/changing')['text'] == 'first retained value'
        second = run('open', root + '/changing', '--refresh')
        assert second['text'] == 'second current value' and second['source'] != first['source']
        assert run('read', first['source'])['text'] == 'first retained value'
        cases.append('refresh retains previous snapshot across fresh processes')
        # Independent HTML structures and late content, rather than only one landing page.
        for index, size in enumerate([1, 3, 50, 70000]):
            token = f'HTML-CANARY-{index}-🦜'
            markup = '<title>Generated source</title><base href="/nested/"><main><pre>' + ('prefix 水\n' * size) + token + '</pre><a href="leaf">Leaf</a></main>'
            path = f'/html-{index}'
            documents[path] = (markup.encode(), 'Text/HTML; Charset=UTF-8', False)
            source = run('open', root + path)
            assert source['title'] == 'Generated source'
            assert source['links'][0]['url'] == root + '/nested/leaf'
            hit = run('find', source['source'], token)['matches'][0]
            assert hit['line'] == size + 1
            passage = run('read', source['source'], hit['offset'], 200)
            assert passage['text'].startswith(token)
            cases.append({'generated_html': index, 'lines': size, 'late_token_offset': hit['offset']})
        for offset in [len(opened['text'].encode()) + 1, opened['text'].encode().index('水'.encode()) + 1]:
            assert 'UTF-8 boundary' in run('read', reference, offset, 20, success=False)
        assert run('find', reference, 'ABSENT')['matches'] == []
        assert 'must not be empty' in run('find', reference, '', success=False)
        cases.append('redirect/title/entities/relative links/scripts/styles/preformatted/invalid offsets')

        for path, body, charset, expected in [
            ('/latin', 'café déjà vu'.encode('latin1'), 'iso-8859-1', 'café déjà vu'),
            ('/utf16', 'Unicode 水 🦜'.encode('utf-16'), 'utf-16', 'Unicode 水 🦜'),
        ]:
            documents[path] = (body, 'text/plain; charset=' + charset, False)
            assert run('open', root + path)['text'] == expected
            cases.append('charset ' + charset)
        documents['/invalid'] = (b'bad\xff', 'text/plain; charset=utf-8', False)
        documents['/pdf'] = (b'%PDF-1.7\n', 'application/pdf', False)
        for suffix, fragment in [('/missing', 'HTTP 404'), ('/limited', 'HTTP 429'),
                                 ('/invalid', 'invalid bytes'), ('/pdf', 'unsupported page media'),
                                 ('/loop', 'web request'), ('/unsafe-redirect', 'HTTP 302'),
                                 ('/oversized', '64 MiB transfer')]:
            assert fragment in run('open', root + suffix, success=False)
            cases.append('rejection ' + suffix)
        for url in ['file:///etc/passwd', 'ftp://example.org/data', 'http://user:secret@127.0.0.1/']:
            assert 'HTTP(S)' in run('open', url, success=False)
        assert 'source must' in run('read', '../escape', success=False)
        cases.append('scheme/credential/path admission')

        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
            results = list(executor.map(lambda _: run('open', root + '/html'), range(8)))
        assert len({item['source'] for item in results}) == 1
        cases.append('eight fresh clients share coherent immutable cache')
        # Corruption is rejected, never accepted under the old reference.
        path = args.output / 'data/peritus/web/sources' / (reference + '.json')
        path.write_bytes(path.read_bytes() + b' ')
        assert 'integrity mismatch' in run('read', reference, success=False)
        cases.append('retained-source integrity rejection')
        report = {'seed': 20261017, 'cases': cases, 'requests': requests, 'binary': binary,
                  'binary_sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest()}
        (args.output / 'evidence.json').write_text(json.dumps(report, indent=2))
        print(json.dumps({'passed_scenarios': len(cases), 'generated_documents': 10, 'output': str(args.output)}))
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == '__main__':
    main()

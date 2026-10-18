"""Real Ubuntu setup, namespace-policy repair, and sandboxed browser consumer.

Run only on a disposable GitHub runner: it may install dependencies and profiles.
No simulated browser, unit suite, provider request, or live user state is used.
"""

import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import threading


ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'dist/runtime-replay-evidence'


def run(arguments, environment, name, success=True):
    result = subprocess.run(arguments, env=environment, cwd=ROOT,
                            capture_output=True, text=True, timeout=600)
    (OUT / (name + '.stdout')).write_text(result.stdout)
    (OUT / (name + '.stderr')).write_text(result.stderr)
    if success and result.returncode:
        raise RuntimeError(f'{name} failed with {result.returncode}; see retained output')
    return result


def main():
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise RuntimeError('requires a disposable GitHub Actions Ubuntu runner')
    OUT.mkdir(parents=True)
    security = Path('/proc/sys/kernel/apparmor_restrict_unprivileged_userns')
    before = security.read_text()
    if before.strip() != '1':
        raise RuntimeError('this replay requires the actual AppArmor restriction enabled')
    environment = dict(os.environ, XDG_DATA_HOME=str(OUT / 'data'),
                       XDG_CACHE_HOME=str(OUT / 'cache'), PERITUS_INSTALL_DEPS='1')
    subprocess.run(['cargo', 'build', '--release', '--locked', '--jobs', '2',
                    '--package', 'peritus-tools-computer'], cwd=ROOT, check=True)
    source = OUT / 'consumer.rs'
    source.write_text('''fn main() {
    let result = if std::env::args().nth(1).as_deref() == Some("ensure") {
        peritus_tools_computer::ensure_setup().and_then(|()| peritus_tools_computer::status())
    } else { peritus_tools_computer::setup() };
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => { eprintln!("{error}"); std::process::exit(1); }
    }
}''')
    library = ROOT / 'target/release/libperitus_tools_computer.rlib'
    consumer = OUT / 'setup-consumer'
    subprocess.run(['rustc', '--edition=2024', '-O', '-C', 'panic=abort',
                    '-C', 'lto=thin', str(source),
                    '-L', 'dependency=' + str(ROOT / 'target/release/deps'),
                    '--extern', 'peritus_tools_computer=' + str(library),
                    '-o', str(consumer)], cwd=ROOT, check=True)
    disabled = run([str(consumer)], dict(environment, PERITUS_INSTALL_DEPS='0'),
                   'disabled-cold', success=False)
    assert disabled.returncode and 'PERITUS_INSTALL_DEPS=0' in disabled.stderr
    assert not (OUT / 'data/peritus/automation/runtime.json').exists()
    status = json.loads(run([str(consumer)], environment, 'cold-setup').stdout)
    assert status['browser_ready'] and status['browser_installed']
    run([str(consumer), 'ensure'], environment, 'fresh-process')
    profiles = list(Path('/etc/apparmor.d').glob('peritus-chromium-*'))
    assert len(profiles) == 1
    subprocess.run(['sudo', '--', '/usr/sbin/apparmor_parser', '--remove', str(profiles[0])],
                    check=True)
    run([str(consumer), 'ensure'], environment, 'unloaded-policy-repair')

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            data = b'<html><body id="proof">REAL-HTTP-7429</body></html>'
            self.send_response(200)
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, *_):
            pass

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    try:
        probe = OUT / 'browser-probe.py'
        probe.write_text('''import json, shutil, sys
from pathlib import Path
from playwright.sync_api import sync_playwright
with sync_playwright() as p:
    observed = []
    for executable in (None, p.chromium.executable_path):
        with p.chromium.launch(headless=True, chromium_sandbox=True, executable_path=executable) as browser:
            page = browser.new_page()
            assert page.goto(sys.argv[1]).status == 200
            assert page.locator('#proof').inner_text() == 'REAL-HTTP-7429'
            page.evaluate("document.body.dataset.proof = 'JS-水-9274'")
            assert page.evaluate('document.body.dataset.proof') == 'JS-水-9274'
            renderers = []
            processes = browser.new_browser_cdp_session().send('SystemInfo.getProcessInfo')['processInfo']
            for process in processes:
                if process['type'] != 'renderer':
                    continue
                path = Path('/proc') / str(process['id'])
                try:
                    cmd = (path / 'cmdline').read_bytes()
                    assert b'--type=renderer' in cmd
                    assert b'--no-sandbox' not in cmd and b'--disable-namespace-sandbox' not in cmd
                    fields = dict(line.split(':', 1) for line in (path / 'status').read_text().splitlines() if ':' in line)
                    assert fields['Seccomp'].strip() == '2'
                    assert fields['NoNewPrivs'].strip() == '1'
                    renderers.append(str(path))
                except FileNotFoundError:
                    continue
            assert renderers
            observed.append({'executable': executable or 'managed headless shell', 'renderers': renderers})
    neighbor = Path(p.chromium.executable_path).with_name('peritus-unlisted-chrome')
    shutil.copy2(p.chromium.executable_path, neighbor)
    try:
        try:
            unexpected = p.chromium.launch(headless=True, chromium_sandbox=True, executable_path=str(neighbor))
        except Exception as error:
            assert 'No usable sandbox!' in str(error) or 'Chromium sandboxing failed!' in str(error)
        else:
            unexpected.close()
            raise AssertionError('The unlisted neighboring executable acquired namespace permission')
    finally:
        neighbor.unlink()
    print(json.dumps(observed))
''')
        url = f'http://127.0.0.1:{server.server_port}/'
        run([status['python'], str(probe), url], environment, 'real-browser')
        # A copied cache containing literal glob/variable syntax must still get
        # exact attachments rather than broad wildcard or variable expansion.
        literal = OUT / r'cache literal [x] {q} @{HOME} 水 \ " * ? ^ ,'
        shutil.copytree(OUT / 'cache/ms-playwright', literal)
        varied = dict(environment, PLAYWRIGHT_BROWSERS_PATH=str(literal))
        run([str(consumer)], varied, 'literal-path-setup')
        run([status['python'], str(probe), url], varied, 'literal-path-browser')
        package_local = dict(environment, PLAYWRIGHT_BROWSERS_PATH='0')
        run([str(consumer)], package_local, 'package-local-setup')
        run([status['python'], str(probe), url], package_local, 'package-local-browser')
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    profiles = []
    for profile in Path('/etc/apparmor.d').glob('peritus-chromium-*'):
        metadata = profile.stat()
        assert metadata.st_uid == 0 and metadata.st_mode & 0o022 == 0
        profiles.append({'path': str(profile), 'text': profile.read_text()})
    assert len(profiles) == 3
    assert security.read_text() == before
    receipt = {'result': 'PASS', 'commit': subprocess.check_output(
        ['git', 'rev-parse', 'HEAD'], text=True).strip(),
        'consumer_sha256': hashlib.sha256(consumer.read_bytes()).hexdigest(),
        'security_before': before, 'security_after': security.read_text(), 'profiles': profiles}
    (OUT / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({'result': 'PASS', 'profiles': len(profiles)}))


if __name__ == '__main__':
    main()

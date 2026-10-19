#!/usr/bin/env python3
"""Operate the actual CLI/TUI, daemon, signed-in provider, and disposable Python CLI."""
import argparse
import codecs
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import termios
import time

import pyte

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--provider-state', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--expect-old', action='store_true')
parser.add_argument('--model', help='Optional account model for this disposable instance only')
args = parser.parse_args()
root = args.output.resolve()
root.mkdir(mode=0o700, parents=True, exist_ok=False)
binary = args.binary.resolve()
project = root / 'project'
project.mkdir()
(project / 'greet.py').write_text('def greet():\n    return "Before"\n\nif __name__ == "__main__":\n    print(greet())\n')
(project / 'AGENTS.md').write_text('# Disposable greeting CLI\ngreet.py owns the program. Use real operations; do not create or run unit tests, formal verification, lint gates, or unrelated commands. This is a tiny fixture.\n')
env = dict(os.environ, TERM='xterm-256color', RAYON_NUM_THREADS='2', TOKIO_WORKER_THREADS='2')
# The controller sets NO_COLOR for shell logs; this replay explicitly exercises a color terminal.
env.pop('NO_COLOR', None)
for name in ('config', 'state', 'cache', 'data'):
    env[f'XDG_{name.upper()}_HOME'] = str(root / name)
state = root / 'state/peritus/product-state'
state.mkdir(parents=True)
provider = json.loads(args.provider_state.read_text())
if args.model:
    provider['providers']['account_models']['codex-account'] = args.model
# Product state contains provider selections; credentials stay in the OS key store.
(state / args.provider_state.name).write_text(json.dumps(provider))
master, slave = pty.openpty()
width, height = 140, 48
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
process = subprocess.Popen([str(binary), 'open', str(project)], cwd=project, env=env,
                           stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
os.close(slave)
screen = pyte.Screen(width, height)
stream = pyte.Stream(screen)
decoder = codecs.getincrementaldecoder('utf-8')('replace')
raw = bytearray()
frames = (root / 'frames.jsonl').open('w')
previous = ''
observed = set()
checks = []

def display():
    return '\n'.join(''.join(screen.buffer[y][x].data for x in range(screen.columns))
                     for y in range(screen.lines))

def pump(seconds=.1):
    global previous
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        if select.select([master], [], [], max(0, min(.05, until-time.monotonic())))[0]:
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            raw.extend(data)
            stream.feed(decoder.decode(data))
            if b'\x1b[?u' in data:
                os.write(master, b'\x1b[?1;2c')
            if b'\x1b[6n' in data:
                os.write(master, f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
        text = display()
        if text != previous:
            frames.write(json.dumps({'time': time.time(), 'width': screen.columns,
                                     'height': screen.lines, 'text': text})+'\n')
            frames.flush()
            previous = text
        if 'Steering: saved r2' in text and 'in model request r1' in text:
            observed.add('saved-before-incorporated')
        if 'Steering: saved r2' in text and 'in model request r2' in text:
            observed.add('incorporated')
        if 'Last change: Updated greet.py:2' in text or 'Last change: Wrote notes.md' in text:
            observed.add('pinned-change')
        if 'Failed workspace_patch' in text:
            observed.add('rejected-patch')
        if 'Failed python3' in text and 'Exit code: 7' in text:
            observed.add('failed-command')
        if 'Results' in text and 'STEER-ACCEPTED' in text and 'https://example.com' in text and any(symbol in text for symbol in ('┬', '┼')):
            observed.add('rendered-report')

def wait(condition, timeout=180):
    until = time.monotonic() + timeout
    while time.monotonic() < until and process.poll() is None:
        pump()
        if condition():
            return
    raise RuntimeError('Timed out waiting for actual UI/state:\n'+display())

def save(label):
    (root / f'{label}.screen.txt').write_text(display())
    cells = [{'y': y, 'x': x, 'text': cell.data, 'fg': cell.fg, 'bg': cell.bg,
              'bold': cell.bold, 'italics': cell.italics, 'underline': cell.underscore}
             for y, row in screen.buffer.items() for x, cell in row.items() if cell.data.strip()]
    (root / f'{label}.cells.json').write_text(json.dumps(cells, ensure_ascii=False))

def submit(text):
    if text.startswith('/'):
        os.write(master, text.encode())
    else:
        os.write(master, b'\x1b[200~'+text.encode()+b'\x1b[201~')
    pump(.2)
    os.write(master, b'\r')

def resize(columns, rows):
    screen.resize(lines=rows, columns=columns)
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack('HHHH', rows, columns, 0, 0))
    os.kill(process.pid, signal.SIGWINCH)
    pump(1)

def records():
    directory = root / 'state/peritus/daemon/workbench-v1/runs'
    return [json.loads(path.read_text()) for path in directory.glob('*.json')]

try:
    answered = set()
    end = time.monotonic()+180
    while time.monotonic() < end and process.poll() is None:
        pump()
        text = display()
        for prompt, answer in [('Trust this workspace?', 'y'), ('Providers (comma-separated', '1'),
                               ('Choose a model number', 'manual gpt-6.1-sol')]:
            if prompt in text and prompt not in answered:
                answered.add(prompt)
                os.write(master, (answer+'\r').encode())
        if 'Ready · Enter sends' in text:
            break
    else:
        raise RuntimeError('Startup failed:\n'+display())
    save('01-ready')
    submit('/effort low')
    pump(.3)
    submit('In this disposable fixture, make exactly these small changes using the workspace tools. Read greet.py. First demonstrate a rejected workspace_patch on greet.py with old="MISSING-FEEDBACK-FRAGMENT" and new="unused"; this deliberate rejection must leave the file unchanged. Then use workspace_patch to replace Before with After café 雪. Use workspace_write to create notes.md containing only FEEDBACK-NOTE. After the edits, use run_command with program python3 and args ["-c", "import time; print(\'FEEDBACK-PAUSE\', flush=True); time.sleep(15)"] so I can steer during a real running tool, then run python3 greet.py. Also demonstrate one intentionally failing run_command using python3 with args ["-c", "raise SystemExit(7)"]; exit 7 is the expected outcome of this demonstration, so report it and continue. Do not add or run unit tests or other gates. Finish with a short Markdown report: heading Results, a bold success phrase, a two-column table naming the file and observed greeting, a fenced Python example of greet(), and a link to https://example.com. Keep the report under 20 lines.')
    wait(lambda: 'After café 雪' in (project/'greet.py').read_text() and
         'Calling python3' in display() and 'FEEDBACK-PAUSE' in display(), 300)
    save('02-real-command-active')
    submit('Keep the existing task and edits. Include STEER-ACCEPTED in your final report heading.')
    wait(lambda: any(row['interaction']['incorporated'] >= 2 for row in records()), 180)
    save('03-steering-incorporated')
    if args.expect_old:
        wait(lambda: any(any(a[1] == 2 and 'Results' in a[2] and 'https://example.com' in a[2]
                             for a in row['interaction']['activities']) for row in records()), 300)
    else:
        wait(lambda: 'rendered-report' in observed, 300)
    pump(1)
    save('04-report')
    assert subprocess.check_output(['python3', str(project/'greet.py')], text=True).strip() == 'After café 雪'
    assert (project/'notes.md').read_text() == 'FEEDBACK-NOTE'
    checks.append('actual-file-edit-and-cli-output')
    # Check styling in cells from the actual terminal, not source strings or snapshots.
    heading_rows = [row for row in screen.buffer.values()
                    if 'Results' in ''.join(row[x].data for x in range(screen.columns))
                    and 'STEER-ACCEPTED' in ''.join(row[x].data for x in range(screen.columns))]
    headings = [cell for row in heading_rows for cell in row.values()
                if cell.bold and cell.fg != 'default']
    table = any(symbol in display() for symbol in ('┬', '┼'))
    if args.expect_old:
        assert 'pinned-change' not in observed
        assert '**' in display() or '| File' in display() or '```' in display()
        checks.append('original-unformatted-output-and-hidden-feedback-reproduced')
    else:
        assert {'saved-before-incorporated', 'incorporated', 'pinned-change', 'rejected-patch', 'failed-command'} <= observed, observed
        assert headings and table and '**' not in display(), display()
        checks.append('persistent-receipts-change-preview-rejection-and-markdown')
    # This is a UI campaign, not candidate qualification. Stop its owned work once the report
    # has exercised the changed display; independent review can otherwise add unrelated work.
    identity_path = root/'state/peritus/daemon/daemon.instance'
    identity = dict(line.split('=', 1) for line in identity_path.read_text().splitlines())
    for line in subprocess.check_output(['ss', '-xlpH'], text=True).splitlines():
        if f"pid={identity['pid']}," in line and records()[0]['phase'] not in (7, 9, 11):
            subprocess.run([str(binary), '--endpoint', line.split()[4], 'runs', 'cancel',
                            '--run', records()[0]['run_id']], env=env, check=True, capture_output=True, timeout=30)
            # An interrupted in-place run can retain effects as RecoveryRequired.
            wait(lambda: records()[0]['phase'] in (7, 8, 9, 10, 11), 30)
    for columns, rows in [(80,24), (48,20), (36,14), (140,40)]:
        resize(columns, rows)
        assert process.poll() is None
        assert 'Steering:' in display() or args.expect_old
        save(f'resize-{columns}x{rows}')
    checks.append('four-terminal-sizes')
    # Fresh ordinary client reconstructs the same durable run without starting new work.
    run = records()[0]['run_id']
    requests = records()[0]['progress']['model_requests']
    os.write(master, b'\x11')
    process.wait(timeout=12)
    os.close(master)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 48, 140, 0, 0))
    screen = pyte.Screen(140, 48)
    stream = pyte.Stream(screen)
    decoder = codecs.getincrementaldecoder('utf-8')('replace')
    process = subprocess.Popen([str(binary), 'open', str(project), '--run', run], cwd=project,
                              env=env, stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
    os.close(slave)
    wait(lambda: 'STEER-ACCEPTED' in display() and ('Steering:' in display() or args.expect_old), 60)
    save('05-fresh-client')
    assert records()[0]['progress']['model_requests'] == requests
    checks.append('fresh-client-retains-formatted-durable-history-without-inference')
    (root/'result.json').write_text(json.dumps({'checks':checks, 'observed':sorted(observed),
        'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(), 'run':run}, indent=2))
    print(json.dumps({'status':'PASS','checks':checks,'observed':sorted(observed),'output':str(root)}), flush=True)
finally:
    if process.poll() is None:
        os.write(master, b'\x11')
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.terminate()
            process.wait(timeout=10)
    frames.close()
    (root/'terminal.ansi').write_bytes(raw)
    (root/'last.screen.txt').write_text(display())
    os.close(master)
    endpoints = list((root/'state/peritus/daemon').glob('*.sock'))
    identity_path = root/'state/peritus/daemon/daemon.instance'
    if identity_path.exists():
        identity = dict(line.split('=', 1) for line in identity_path.read_text().splitlines())
        for line in subprocess.check_output(['ss', '-xlpH'], text=True).splitlines():
            if f"pid={identity['pid']}," in line:
                endpoints.append(Path(line.split()[4]))
    for endpoint in set(endpoints):
        subprocess.run([str(binary), '--endpoint', str(endpoint), 'shutdown', '--wait'],
                       env=env, capture_output=True, timeout=40)

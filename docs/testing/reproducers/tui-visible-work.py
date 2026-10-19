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


# Independent fixture and requirements, deliberately larger than the former UI previews.
read_lines=[f'{n:03}: OBSERVATION-{n:03} '+('source-body-café-雪 '*6)+(' TOKEN-LOOKUP' if n in (11,47,83) else '') for n in range(100)]
(project/'observations.txt').write_text('\n'.join(read_lines)+'\n')
old_lines=[f'# OLD-LINE-{n:02} '+('source-context '*6) for n in range(42)]
(project/'patch.py').write_text('\n'.join(old_lines)+'\nprint("VISIBLE-CLI")\n')
summary_while_running=False

def endpoint():
    identity=dict(line.split('=',1) for line in (root/'state/peritus/daemon/daemon.instance').read_text().splitlines())
    return next(line.split()[4] for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines() if f"pid={identity['pid']}," in line)

def cancel():
    subprocess.run([str(binary),'--endpoint',endpoint(),'runs','cancel','--run',records()[0]['run_id']],env=env,check=True,capture_output=True,timeout=30)
    wait(lambda:records()[0]['phase'] in (7,8,9,10,11),40)

def find_screen(marker,label):
    os.write(master,b'\x1b[F');pump(.2)
    for _ in range(220):
        if marker in display():save(label);return
        os.write(master,b'\x1b[5~');pump(.04)
    raise AssertionError('Not scrollable: '+marker)

try:
    answered=set();end=time.monotonic()+100
    while time.monotonic()<end and process.poll() is None:
        pump()
        text=display()
        if 'Trust this workspace?' in text and 'trust' not in answered:
            answered.add('trust');os.write(master,b'y\r')
        if 'Ready · Enter sends' in text:break
    else:raise AssertionError('Startup failed')
    submit('/effort xhigh');pump(.3)
    submit('Use exactly the workspace tools to make this tiny fixture change. List the root, search for TOKEN-LOOKUP in observations.txt, read all 100 lines of observations.txt with max_bytes 65536, read patch.py, and read the real-testing SKILL.md with skill_read. Use workspace_patch to replace the complete consecutive comment block OLD-LINE-00 through OLD-LINE-41 with the exact same block except every OLD-LINE becomes NEW-LINE. Keep the padding and following print unchanged. Run python3 patch.py and finish with a short Markdown heading Results and the actual output. No unit tests or unrelated gates. Do the requested operations directly, with one initial grounding check, and keep planning brief.')
    started=time.monotonic();summary_observations=[]
    while time.monotonic()-started<420:
        pump(.2)
        rows=records()
        if rows:
            activities=rows[0]['interaction']['activities']
            request_start=max((i for i,a in enumerate(activities) if a[1]==4 and a[2].startswith('Requesting model ')),default=-1)
            if any(a[1]==4 and a[3]=='Provider thinking summary' for a in activities[request_start+1:]):
                identity=dict(line.split('=',1) for line in (root/'state/peritus/daemon/daemon.instance').read_text().splitlines())
                parents={};names={}
                for line in subprocess.check_output(['ps','-eo','pid,ppid,comm'],text=True).splitlines()[1:]:
                    fields=line.split()
                    if len(fields)==3:parents[int(fields[0])]=int(fields[1]);names[int(fields[0])]=fields[2]
                owned=int(identity['pid'])
                for pid,name in names.items():
                    if 'codex' not in name:continue
                    ancestor=pid
                    for _ in range(12):
                        ancestor=parents.get(ancestor,0)
                        if ancestor==owned:summary_while_running=True;break
                        if not ancestor:break
                if summary_while_running and not summary_observations:
                    summary_observations.append(time.monotonic()-started);save('live-published-summary')
            if 'NEW-LINE-41' in (project/'patch.py').read_text() and any(a[1]==2 and 'Results' in a[2] and 'VISIBLE-CLI' in a[2] for a in activities):break
    else:raise AssertionError('No completed requested edit/report within campaign observation window')
    assert subprocess.check_output(['python3',str(project/'patch.py')],text=True).strip()=='VISIBLE-CLI'
    assert 'OLD-LINE-' not in (project/'patch.py').read_text()
    assert summary_while_running,'No published summary observed while the actual provider process was running'
    cancel();pump(.5)
    find_screen('OBSERVATION-047','read-middle')
    cell_colors=[cell.fg for row in screen.buffer.values() for cell in row.values() if cell.data.strip()]
    assert any(color in ('default','white') for color in cell_colors),'Tool body is still muted'
    find_screen('TOKEN-LOOKUP','search-hits')
    find_screen('-# OLD-LINE-23','removed-middle')
    find_screen('+# NEW-LINE-23','added-middle')
    find_screen('Build it. Run it.','skill-body')
    assert not any(' more lines · /details' in json.loads(line)['text'] for line in (root/'frames.jsonl').read_text().splitlines()),'UI still hides returned lines'
    save('before-continue')
    efficiency=next((root/'state/peritus/daemon/product-runs').glob('*.efficiency.jsonl'))
    previous_events=len(efficiency.read_text().splitlines())
    submit('Continue. Keep the existing completed edits. Make one more tiny change: use workspace_patch to change the print literal VISIBLE-CLI to CONTINUED-CLI in patch.py, run python3 patch.py, and report the output. Reuse the current plan and observations.')
    wait(lambda:'CONTINUED-CLI' in (project/'patch.py').read_text(),300)
    events=[json.loads(line) for line in efficiency.read_text().splitlines()[previous_events:]]
    assert any(event.get('event')=='design-reused' for event in events),'Continue regenerated the plan'
    assert not any(event.get('event')=='model-view' and 'designer' in str(event) for event in events),'Designer inference ran during continue'
    assert subprocess.check_output(['python3',str(project/'patch.py')],text=True).strip()=='CONTINUED-CLI'
    cancel();save('continued-with-plan-reuse')
    (root/'result.json').write_text(json.dumps({'status':'PASS','published_summary_while_real_provider_running':summary_while_running,'first_live_summary_seconds':summary_observations,'continued_plan_reused':True,'body_search_and_both_patch_middles_scrollable':True,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()},indent=2))
    print((root/'result.json').read_text(),flush=True)
finally:
    if process.poll() is None:
        os.write(master,b'\x11')
        try:process.wait(timeout=12)
        except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=12)
    frames.close();(root/'terminal.ansi').write_bytes(raw);(root/'last.screen.txt').write_text(display());os.close(master)
    try:subprocess.run([str(binary),'--endpoint',endpoint(),'shutdown','--wait'],env=env,capture_output=True,timeout=40)
    except (StopIteration,FileNotFoundError):pass

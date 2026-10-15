#!/usr/bin/env python3
"""Operate actual Peritus via PTY, decode its screen, and replay human editing.

The PTY driver is a terminal protocol consumer, not a replacement application.
It can advertise kitty protocol support or legacy device attributes, but does
not establish macOS, physical keyboard, or cmux shortcut delivery.
Requires pyte. Uses an already configured, disposable offline XDG fixture.
"""
import argparse, codecs, fcntl, hashlib, json, os, pty, random, select, signal, struct, subprocess, termios, time
from pathlib import Path
import pyte

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--fixture', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--enhanced', action='store_true')
p.add_argument('--baseline', action='store_true')
p.add_argument('--workspace', type=Path)
p.add_argument('--mode-only', action='store_true')
p.add_argument('--folder', choices=['writable','read-only'])
a = p.parse_args()
root = a.fixture.resolve()
if not (root/'owned-fixture.json').exists() or json.loads((root/'owned-fixture.json').read_text()).get('owned_disposable_fixture') is not True:
    raise SystemExit('A disposable fixture marker is required; never use live state.')
workspace = (a.workspace or root/'repo').resolve()
if not workspace.is_relative_to(root): raise SystemExit('Workspace must be inside the disposable fixture.')
a.output.mkdir(parents=True, exist_ok=True)
env = os.environ.copy()
env.update({f'XDG_{name.upper()}_HOME': str(root/name) for name in ('config','state','cache')})
env['TERM'] = 'xterm-256color'
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 32, 120, 0, 0))
original_termios = termios.tcgetattr(slave)
proc = subprocess.Popen([str(a.binary.resolve()), 'open', str(workspace)], stdin=slave, stdout=slave, stderr=slave, env=env, start_new_session=True)
screen = pyte.Screen(120, 32)
stream = pyte.Stream(screen)
decoder = codecs.getincrementaldecoder('utf-8')('replace')
raw = bytearray()
results = []

def pump(seconds=.15):
    until = time.monotonic()+seconds
    while time.monotonic()<until:
        if select.select([master], [], [], min(.03, max(.001, until-time.monotonic())))[0]:
            try: data = os.read(master, 65536)
            except OSError: return
            raw.extend(data)
            stream.feed(decoder.decode(data))
            # Real terminal capability protocol replies; no application/API stubs.
            if b'\x1b[?u' in data:
                os.write(master, b'\x1b[?0u\x1b[?1;2c' if a.enhanced else b'\x1b[?1;2c')
            if b'\x1b[6n' in data:
                os.write(master, f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())

def send(data, delay=.15):
    os.write(master, data)
    pump(delay)

def draft():
    display = screen.display
    top = max((i for i, line in enumerate(display) if line.startswith('┌')), default=-1)
    if top < 0: raise RuntimeError('Composer was not visible: '+ '\n'.join(display))
    lines = []
    for row in display[top+1:]:
        if row.startswith('└'): break
        if row.startswith('│'): lines.append(row[1:row.rfind('│')].rstrip())
    return '\n'.join(lines), display[top]

def capture(name):
    (a.output/f'{name}.screen.txt').write_text('\n'.join(screen.display)+'\n')

def reset(text):
    send(b'\x1b[1;5H\x1b[1;6F\x7f', .06)
    send(b'\x1b[200~'+text.encode()+b'\x1b[201~', .06)

HOME = b'\x1b[H'
END = b'\x1b[F'
LEFT = b'\x1b[D'
ALT_LEFT = b'\x1b[1;3D'
ALT_RIGHT = b'\x1b[1;3C'
CTRL_LEFT = b'\x1b[1;5D'
CTRL_RIGHT = b'\x1b[1;5C'
CMD_LEFT = b'\x1b[1;9D'
CMD_RIGHT = b'\x1b[1;9C'
SHIFT_ALT_LEFT = b'\x1b[1;4D'
SHIFT_CMD_LEFT = b'\x1b[1;10D'
ALT_BACKSPACE = b'\x1b\x7f'
CTRL_BACKSPACE = b'\x1b[127;5u'

def case(name, text, actions, expected):
    reset(text)
    for keys in actions: send(keys, .035)
    pump(.12)
    observed, title = draft()
    passed = observed == expected
    result = {'case':name,'input':text,'keys_hex':[k.hex() for k in actions],'expected':expected,'observed':observed,'pass':passed}
    results.append(result)
    if not passed: capture('failure-'+name)
    return passed

try:
    pump(3)
    capture('opened')
    results.append({'case':'real-daemon-connected','pass':any('Ready' in line for line in screen.display) and not any('Offline' in line for line in screen.display)})
    marker=b'\x1b[200~|\x1b[201~'
    # Independent literal expectations expose insert-vs-motion, wrong line, and selection loss.
    cases = [
        ('option-left','alpha beta',[ALT_LEFT,marker],'alpha |beta'),
        ('option-right','alpha beta',[HOME,ALT_RIGHT,marker],'alpha |beta'),
        ('legacy-alt-b','alpha beta',[b'\x1bb',marker],'alpha |beta'),
        ('legacy-alt-f','alpha beta',[HOME,b'\x1bf',marker],'alpha |beta'),
        ('ctrl-left-retained','alpha beta',[CTRL_LEFT,marker],'alpha |beta'),
        ('ctrl-right-retained','alpha beta',[HOME,CTRL_RIGHT,marker],'alpha |beta'),
        ('cmd-current-line-start','first line\nsecond line',[CMD_LEFT,marker],'first line\n|second line'),
        ('cmd-current-line-end','first line\nsecond line',[HOME,LEFT,CMD_RIGHT,marker],'first line|\nsecond line'),
        ('shift-option-replace','alpha beta',[SHIFT_ALT_LEFT,b'X'],'alpha X'),
        ('shift-legacy-replace','alpha beta',[b'\x1b[98;4u',b'X'],'alpha X'),
        ('shift-cmd-replace','first line\nsecond line',[SHIFT_CMD_LEFT,b'X'],'first line\nX'),
        ('selected-then-option','alpha beta',[b'\x1b[1;2D',ALT_LEFT,marker],'alpha |beta'),
        ('selected-then-legacy','alpha beta',[b'\x1b[1;2D',b'\x1bb',marker],'alpha |beta'),
        ('option-delete-word','alpha beta',[ALT_BACKSPACE,marker],'alpha |'),
        ('ctrl-delete-word','alpha beta',[CTRL_BACKSPACE,marker],'alpha |'),
        ('home-current-line','first line\nsecond line',[HOME,marker],'first line\n|second line'),
        ('end-current-line','first line\nsecond line',[HOME,END,marker],'first line\nsecond line|'),
        ('empty-line-start','a\n\nb',[CMD_LEFT,CMD_LEFT,marker],'a\n\n|b'),
        ('trailing-newline','a\n',[CMD_LEFT,marker],'a\n|'),
        ('unicode-word','café résumé',[ALT_LEFT,marker],'café |résumé'),
        ('unicode-line','漢字\ncafé',[CMD_LEFT,marker],'漢字\n|café'),
        ('empty-buffer','',[ALT_LEFT,ALT_RIGHT,CMD_LEFT,CMD_RIGHT,ALT_BACKSPACE,marker],'|'),
        ('enhanced-alt-b','alpha beta',[b'\x1b[98;3u',marker],'alpha |beta'),
        ('enhanced-alt-f','alpha beta',[HOME,b'\x1b[102;3u',marker],'alpha |beta'),
        ('release-no-motion','alpha beta',[b'\x1b[98;3:3u',marker],'alpha beta|'),
        ('repeat-word-motion','one two three',[b'\x1b[98;3:2u',b'\x1b[98;3:2u',marker],'one |two three'),
        ('delete-trailing-space','alpha beta   ',[ALT_BACKSPACE,marker],'alpha |'),
        ('super-char-no-insertion','alpha beta',[b'\x1b[98;9u',marker],'alpha beta|'),
    ]
    if not a.mode_only:
        for args in cases: case(*args)
    # Structured generated text: two distinct words with a requirement-defined boundary.
    seed=121122
    rng=random.Random(seed)
    words=['alpha','bravo','café','résumé','delta_7','éclair','omega42']
    for i in range(0 if a.mode_only else 36):
        left,right=rng.sample(words,2)
        gap=' '*rng.randint(1,4)
        sequence=rng.choice([[ALT_LEFT],[b'\x1bb'],[HOME,ALT_RIGHT],[HOME,b'\x1bf'],[CTRL_LEFT]])
        case(f'generated-{i:02}',left+gap+right,[*sequence,marker],left+gap+'|'+right)
    capture('editing-complete')
    if not a.baseline:
        # Mode selection is local UI only. Keyboard-enter commands; no inference.
        reset('')
        for mode in ['plan','review','chat','build','review','plan']:
            reset('')
            send(('/'+mode+'\r').encode(),.15)
            _,title=draft()
            readonly = mode in ('plan','review') or a.folder=='read-only'
            passed=(mode.title() in title if mode!='build' or not a.folder else 'Chat' in title) and (not readonly or 'read-only' in title)
            results.append({'case':'mode-'+mode,'title':title,'pass':passed})
        pump(6.3) # notice expiration must not remove the persistent mode hint
        capture('persistent-plan')
        route = 'read-only folder' if a.folder=='read-only' else '/chat' if a.folder else '/build'
        results.append({'case':'hint-after-notice','pass':'read-only' in draft()[1] and route in draft()[1]})
        for width,height in [(40,24),(60,24),(120,32)]:
            screen.resize(lines=height,columns=width)
            fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',height,width,0,0))
            os.kill(proc.pid,signal.SIGWINCH)
            pump(.4)
            capture(f'width-{width}')
            results.append({'case':f'width-{width}','title':draft()[1],'pass':'Plan' in draft()[1] and 'read-only' in draft()[1] and route in draft()[1]})
        send(b'/runs\r',.2)
        send(b'?',.2)
        capture('help-modes')
        displayed='\n'.join(screen.display)
        results.append({'case':'help-modes','pass':all(s in displayed for s in ['Conversation modes','cannot edit files or run commands','Mode changes are explicit'])})
    send(b'\x1b[113;5u' if a.enhanced and not a.baseline else b'\x11',.4)
    proc.wait(timeout=5)
    flags=termios.tcgetattr(slave)
    results.append({'case':'orderly-exit','pass':proc.returncode==0 and flags==original_termios,'exit_code':proc.returncode,'termios_restored':flags==original_termios})
    results.append({'case':'keyboard-flags-lifecycle','pass':(b'\x1b[>1u' in raw and b'\x1b[<1u' in raw) if a.enhanced and not a.baseline else b'\x1b[>1u' not in raw,'pushes':raw.count(b'\x1b[>1u'),'pops':raw.count(b'\x1b[<1u')})
finally:
    if proc.poll() is None:
        proc.terminate()
        try:proc.wait(timeout=3)
        except subprocess.TimeoutExpired:proc.kill();proc.wait()
    (a.output/'terminal.ansi').write_bytes(raw)
    (a.output/'results.json').write_text(json.dumps({'binary':str(a.binary.resolve()),'sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'enhanced':a.enhanced,'baseline':a.baseline,'seed':121122,'results':results},indent=2,ensure_ascii=False)+'\n')
    os.close(master);os.close(slave)
passed=sum(r['pass'] for r in results)
print(json.dumps({'cases':len(results),'passed':passed,'failures':[r['case'] for r in results if not r['pass']],'output':str(a.output)}))
if not a.baseline and passed!=len(results):raise SystemExit(1)

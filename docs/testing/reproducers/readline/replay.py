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
from wcwidth import wcwidth

class TerminalScreen(pyte.Screen):
    @property
    def display(self):
        # pyte leaves empty wide-glyph continuation cells after repainting. An orphan
        # continuation is a blank cell, not a missing first character to index.
        rows=[]
        for y in range(self.lines):
            output=[];skip=False
            for x in range(self.columns):
                if skip:skip=False;continue
                data=self.buffer[y][x].data or " "
                output.append(data)
                skip=wcwidth(data[0])==2
            rows.append("".join(output))
        return rows

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--fixture', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--enhanced', action='store_true')
p.add_argument('--baseline', action='store_true')
p.add_argument('--workspace', type=Path)
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
screen = TerminalScreen(120, 32)
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


CH=b'\x1b[1;5H';CE=b'\x1b[1;5F'
MARK=b'\x1b[200~|\x1b[201~'
try:
    start=time.monotonic()
    while time.monotonic()-start < 30:
        pump(.2)
        if any('Ready' in line for line in screen.display):break
        if proc.poll() is not None:break
    capture('opened')
    if not any('Ready' in line for line in screen.display):raise RuntimeError('Client not ready: '+ '\n'.join(screen.display))
    results.append({'case':'connected','pass':any('Ready' in line for line in screen.display)})
    cases=[
      ('ghostty-cmd-left-bytes','first\nsecond',[b'\x01',MARK],'first\n|second'),
      ('ghostty-cmd-right-bytes','first\nsecond',[CH,b'\x05',MARK],'first|\nsecond'),
      ('home-line','first\nsecond',[HOME,MARK],'first\n|second'),
      ('end-line','first\nsecond',[CH,END,MARK],'first|\nsecond'),
      ('buffer-start','first\nsecond',[CH,MARK],'|first\nsecond'),
      ('buffer-end','first\nsecond',[CH,CE,MARK],'first\nsecond|'),
      ('ctrl-b-unicode','café',[b'\x02',MARK],'caf|é'),
      ('ctrl-f-unicode','漢字',[CH,b'\x06',MARK],'漢|字'),
      ('ctrl-p','alpha\nbravo',[b'\x10',MARK],'alpha|\nbravo'),
      ('ctrl-n','alpha\nbravo',[CH,b'\x0e',MARK],'alpha\n|bravo'),
      ('ctrl-up-boundary','alpha',[b'\x10',MARK],'alpha|'),
      ('ctrl-down-boundary','alpha',[b'\x0e',MARK],'alpha|'),
      ('ctrl-h','café',[b'\x08',MARK],'caf|'),
      ('ctrl-d','漢字',[CH,b'\x04',MARK],'|字'),
      ('ctrl-w','alpha beta',[b'\x17',MARK],'alpha |'),
      ('ctrl-u-line','alpha\nbeta',[b'\x15',MARK],'alpha\n|'),
      ('ctrl-k-line','alpha\nbeta',[CH,b'\x06',b'\x0b',MARK],'a|\nbeta'),
      ('ctrl-k-newline','alpha\nbeta',[CH,b'\x05',b'\x0b',MARK],'alpha|beta'),
      ('alt-d','alpha beta',[CH,b'\x1bd',MARK],'|beta'),
      ('alt-delete','alpha beta',[CH,b'\x1b[3;3~',MARK],'|beta'),
      ('ctrl-delete','alpha beta',[CH,b'\x1b[3;5~',MARK],'|beta'),
      ('ctrl-shift-delete','alpha beta',[CH,b'\x1b[3;6~',MARK],'|beta'),
      ('ctrl-shift-backspace','alpha beta',[b'\x1b[127;6u',MARK],'alpha |'),
      ('ctrl-alt-h','alpha beta',[b'\x1b[104;7u',MARK],'alpha |'),
      ('super-backspace','alpha\nbeta',[b'\x1b[127;9u',MARK],'alpha\n|'),
      ('selected-ctrl-w','alpha beta',[CH,b'\x1b[1;6F',b'\x17',MARK],'|'),
      ('selected-alt-d','alpha beta',[b'\x1b[1;2D',b'\x1bd',MARK],'alpha bet|'),
      ('selected-ctrl-b','alpha beta',[b'\x1b[1;2D',b'\x02',MARK],'alpha bet|a'),
      ('selected-ctrl-f','alpha beta',[b'\x1b[1;2D',b'\x06',MARK],'alpha beta|'),
      ('word-alias-left','alpha beta',[ALT_LEFT,MARK],'alpha |beta'),
      ('word-alias-right','alpha beta',[CH,ALT_RIGHT,MARK],'alpha |beta'),
      ('super-left-retained','alpha\nbeta',[CMD_LEFT,MARK],'alpha\n|beta'),
      ('super-right-retained','alpha\nbeta',[CH,CMD_RIGHT,MARK],'alpha|\nbeta'),
      ('typing-letters','abefdk',[b'X',MARK],'abefdkX|'),
      ('ctrl-release','alpha',[b'\x1b[97;5:3u',MARK],'alpha|'),
      ('empty-all','',[b'\x01\x05\x02\x06\x10\x0e\x08\x04\x17\x15\x0b',MARK],'|'),
    ]
    for args in cases:case(*args)
    seed=12620261014;rng=random.Random(seed)
    words=['alpha','bravo','café','résumé','漢字','delta_7']
    for i in range(36):
      one,two=rng.sample(words,2);gap=' '*rng.randint(1,3)
      strategy=i%3
      if strategy==0:case(f'generated-{i:02}',one+'\n'+two,[b'\x01',MARK],one+'\n|'+two)
      elif strategy==1:case(f'generated-{i:02}',one+gap+two,[b'\x17',MARK],one+gap+'|')
      else:case(f'generated-{i:02}',one+gap+two,[CH,b'\x1bd',MARK],'|'+two)
    capture('edited')
    send(b'\x1b[113;5u' if a.enhanced else b'\x11',.4);proc.wait(timeout=5)
    results.append({'case':'orderly-exit','pass':proc.returncode==0 and termios.tcgetattr(slave)==original_termios})
    results.append({'case':'keyboard-flags','pass':(b'\x1b[>1u' in raw and b'\x1b[<1u' in raw) if a.enhanced else b'\x1b[>1u' not in raw})
finally:
    if proc.poll() is None:
        proc.terminate()
        try:proc.wait(timeout=3)
        except subprocess.TimeoutExpired:proc.kill();proc.wait()
    (a.output/'terminal.ansi').write_bytes(raw)
    (a.output/'results.json').write_text(json.dumps({'binary':str(a.binary),'enhanced':a.enhanced,'baseline':a.baseline,'sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'seed':12620261014,'results':results},indent=2,ensure_ascii=False)+'\n')
    os.close(master);os.close(slave)
print(json.dumps({'cases':len(results),'passed':sum(x['pass'] for x in results),'failures':[x['case'] for x in results if not x['pass']]}))

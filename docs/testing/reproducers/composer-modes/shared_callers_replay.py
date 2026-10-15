#!/usr/bin/env python3
"""Replay shared-editor routing through real Peritus/daemon/PTY interfaces.

Only local modal/field edits and metadata-only command-created conversation are
used. No model task, import, preview, confirmation, process attachment, or
substituted application/daemon response is used. Kitty capability replies emulate a
terminal's protocol, not macOS or cmux physical shortcut routing.
"""
import argparse, codecs, fcntl, hashlib, json, os, pty, select, signal, struct, subprocess, termios, time
from pathlib import Path
import pyte

p=argparse.ArgumentParser()
p.add_argument('--binary',type=Path,required=True)
p.add_argument('--fixture',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--baseline',action='store_true')
p.add_argument('--workspace',type=Path)
p.add_argument('--fields-only',action='store_true')
a=p.parse_args()
root=a.fixture.resolve()
if json.loads((root/'owned-fixture.json').read_text()).get('owned_disposable_fixture') is not True:
    raise SystemExit('Owned disposable fixture marker is required.')
workspace=(a.workspace or root/'repo').resolve()
if not workspace.is_relative_to(root):raise SystemExit('Workspace must be inside the owned disposable fixture.')
a.output.mkdir(parents=True,exist_ok=True)
env=os.environ.copy()
env.update({f'XDG_{name.upper()}_HOME':str(root/name) for name in ('config','state','cache')})
env['TERM']='xterm-256color'
master,slave=pty.openpty()
fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',40,120,0,0))
original=termios.tcgetattr(slave)
binary_hash=hashlib.sha256(a.binary.read_bytes()).hexdigest()
proc=subprocess.Popen([str(a.binary.resolve()),'open',str(workspace)],stdin=slave,stdout=slave,stderr=slave,env=env,start_new_session=True)
screen=pyte.Screen(120,40)
stream=pyte.Stream(screen)
decode=codecs.getincrementaldecoder('utf-8')('replace')
raw=bytearray()
report={'version':1,'binary':str(a.binary.resolve()),'sha256':binary_hash,'baseline':a.baseline,'fixture':str(root),'workspace':str(workspace),'fields_only':a.fields_only,'started':time.time(),'actions':[],'results':[]}
HOME=b'\x1b[H';END=b'\x1b[F';DELETE=b'\x1b[3~';ESC=b'\x1b';CTRL_BS=b'\x1b[127;5u';ALT_B=b'\x1bb';ALT_F=b'\x1bf';SUPER_B=b'\x1b[98;9u';QUIT=b'\x1b[113;5u'

def pump(seconds=.12):
    until=time.monotonic()+seconds
    while time.monotonic()<until:
        if select.select([master],[],[],min(.03,max(.001,until-time.monotonic())))[0]:
            try:data=os.read(master,65536)
            except OSError:return
            raw.extend(data);stream.feed(decode.decode(data))
            if b'\x1b[?u' in data:os.write(master,b'\x1b[?0u\x1b[?1;2c')
            if b'\x1b[6n' in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())

def send(data,delay=.12):
    report['actions'].append({'keys_hex':data.hex(),'delay':delay})
    pending=memoryview(data)
    while pending:
        count=os.write(master,pending)
        if not count:raise RuntimeError('PTY accepted no input bytes')
        pending=pending[count:]
    pump(delay)

def paste(text):send(b'\x1b[200~'+text.encode()+b'\x1b[201~',.20)

def capture(name):
    filename=f'{name}.screen.txt'
    (a.output/filename).write_text('\n'.join(screen.display)+'\n')
    return filename

def visible():return '\n'.join(screen.display)

def wait_for(text,timeout=5):
    until=time.monotonic()+timeout
    while text not in visible() and time.monotonic()<until:pump(.15)
    if text not in visible():
        capture('blocked-'+text.replace(' ','-').replace('/','_')[:35])
        raise RuntimeError(f'Actual UI route did not expose {text!r}')

def result(name,passed,expected,observed,**extra):
    report['results'].append({'case':name,'status':'PASS' if passed else 'FAIL','expected':expected,'observed':observed,**extra,'screen':capture(name)})

def clear():
    send(END)
    # The bounded texts have at most three distinct whitespace-separated words.
    # Existing Ctrl-word deletion is a user action, with no state injection.
    send(CTRL_BS*8,.20)

def editor_text(label,modal=False):
    lines=screen.display
    top=next(i for i,line in enumerate(lines) if '┌' in line and label in line)
    left=lines[top].index('┌');right=lines[top].rfind('┐')
    bottom=next(i for i in range(top+1,len(lines)) if len(lines[i])>left and lines[i][left]=='└')
    rows=lines[top+(3 if modal else 1):bottom-(1 if modal else 0)]
    return '\n'.join(row[left+1:right].rstrip() for row in rows).rstrip('\n')

def composer_clear():
    # Composer supports selection, unlike the shared modal/field editors.
    send(b'\x1b[1;5H\x1b[1;6F\x7f',.10)

def command(text):
    composer_clear()
    # Commands are typed, never pasted, and contain no inference arguments.
    send(text.encode()+b'\r',.50)

def field_cases(kind,key,label,capacity):
    send(key);wait_for(label)
    for direction in ('back','forward'):
        clear()
        text=('x'*(capacity-11)+' alpha beta') if direction=='back' else ('alpha beta '+'x'*(capacity-11))
        paste(text)
        # Confirm ordinary insertion is refused at the requested capacity.
        send(b'|')
        before=editor_text(label)
        result(f'{kind}-capacity-{direction}', '|' not in before, 'At capacity, ordinary marker insertion is rejected', before,capacity=capacity,input=text)
        if direction=='forward':send(HOME)
        send(ALT_B if direction=='back' else ALT_F)
        send(DELETE)
        send(b'|')
        observed=editor_text(label)
        expected='alpha |eta' if direction=='back' else 'alpha |eta '
        # At this size the real field viewport shows a window of the local draft.
        # The unique sentinel plus neighboring word verifies the cursor's effect.
        passed=expected in observed and observed.count('|')==1
        result(f'{kind}-alt-{direction}-at-capacity',passed,expected,observed,capacity=capacity,input=text,keys_hex=[*([] if direction=='back' else [HOME.hex()]),(ALT_B if direction=='back' else ALT_F).hex(),DELETE.hex(),'7c'])
    # Readline deletion must reach the shared handler even for Alt-modified characters.
    for chord,keys,expected in [('ctrl-a',[b'\x01'],'|alpha beta'),('ctrl-w',[b'\x17'],'alpha |'),('alt-d',[HOME,b'\x1bd'],'|beta')]:
        clear();paste('alpha beta')
        for sequence in keys:send(sequence)
        send(b'|');observed=editor_text(label)
        result(f'{kind}-readline-{chord}',observed==expected,expected,observed,input='alpha beta',keys_hex=[seq.hex() for seq in keys])
    send(ESC,.2)

try:
    pump(3.5)
    capture('opened')
    wait_for('Ready')
    if not a.fields_only:
        command('/runs');send(b'6');send(b'a');wait_for('Attach to daemon-owned process')
        for name,keys,expected in [('modal-alt-b',[ALT_B],'alpha |beta'),('modal-alt-f',[HOME,ALT_F],'alpha |beta'),('modal-super-char',[SUPER_B],'alpha beta|'),('modal-ctrl-a',[b'\x01'],'|alpha beta'),('modal-ctrl-w',[b'\x17'],'alpha |'),('modal-alt-d',[HOME,b'\x1bd'],'|beta')]:
            clear();paste('alpha beta')
            for key in keys:send(key)
            paste('|')
            observed=editor_text('Attach to daemon-owned process',True)
            result(name,observed==expected,expected,observed,input='alpha beta',keys_hex=[key.hex() for key in keys])
        send(ESC);send(ESC)
    command('/files')
    try:
        wait_for('Files · explicit references',timeout=4)
    except RuntimeError:
        capture('files-initial-route-blocked')
        # One ordinary local recovery attempt, with no provider/configuration
        # changes: new composer followed by Ctrl-R reconnect, then /files.
        send(ESC);command('/new');send(b'\x1b[114;5u',.6)
        command('/files')
        wait_for('Files · explicit references',timeout=4)
    field_cases('file-path',b'i','Workspace-relative path',4096)
    field_cases('file-range',b'g','Range: all',80)
    field_cases('file-caption',b't','Caption / instruction',8192)
    send(ESC)
    command('/attach')
    wait_for('retained image',timeout=8)
    # From the retained-image page, i opens the local preview form; another i
    # focuses its empty path. No path read/preview/confirmation is triggered.
    send(b'i');pump(.3)
    field_cases('image-path',b'i','Absolute image path',4096)
    field_cases('image-caption',b't','Caption / instruction',8192)
    send(ESC)
    send(QUIT,.40);proc.wait(timeout=5)
    result('orderly-exit',proc.returncode==0 and termios.tcgetattr(slave)==original,'Zero exit and restored original termios',{'exit_code':proc.returncode,'termios_restored':termios.tcgetattr(slave)==original})
except Exception as error:
    report['results'].append({'case':'route-completion','status':'BLOCKED','expected':'Directly affected real UI journeys complete','observed':str(error),'screen':capture('blocked')})
finally:
    if proc.poll() is None:
        # Only terminate the client owned by this driver, never the daemon.
        proc.terminate()
        try:proc.wait(timeout=3)
        except subprocess.TimeoutExpired:proc.kill();proc.wait()
    (a.output/'terminal.ansi').write_bytes(raw)
    report['finished']=time.time()
    report['summary']={status:sum(r['status']==status for r in report['results']) for status in ('PASS','FAIL','BLOCKED')}
    (a.output/'results.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
    os.close(master);os.close(slave)
print(json.dumps({'output':str(a.output),'summary':report['summary'],'failures':[r['case'] for r in report['results'] if r['status']!='PASS']}))
if not a.baseline and any(r['status']!='PASS' for r in report['results']):raise SystemExit(1)

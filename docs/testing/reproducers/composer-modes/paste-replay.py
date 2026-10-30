#!/usr/bin/env python3
"""Operate the real launcher, TUI and daemon; paste never submits a message."""
import argparse, codecs, fcntl, hashlib, json, os, pathlib, pty, re, select, struct, subprocess, termios, time
import pyte

p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=pathlib.Path,required=True);p.add_argument('--fixture',type=pathlib.Path,required=True);p.add_argument('--expect-old-quit',action='store_true');a=p.parse_args()
root=a.fixture.resolve();project=root/'project'
assert json.loads((root/'owned-fixture.json').read_text()).get('owned_disposable_fixture') is True
assert (project/'.git').exists(), 'Prepare a committed disposable Git workspace first'
env=dict(os.environ,TERM='xterm-256color',TOKIO_WORKER_THREADS='2')
for name in ('config','state','cache','data'):
    env['XDG_'+name.upper()+'_HOME']=str(root/name)
master,slave=pty.openpty()
fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',36,120,0,0))
child=subprocess.Popen([str(a.binary.resolve()),'open',str(project)],cwd=project,env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True)
os.close(slave)
screen=pyte.Screen(120,36);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf8')('replace');raw=bytearray();checks=[]
def view():return '\n'.join(screen.display)
def pump(seconds=.15):
    until=time.monotonic()+seconds
    while time.monotonic()<until:
        if select.select([master],[],[],min(.02,max(.001,until-time.monotonic())))[0]:
            try:data=os.read(master,65536)
            except OSError:return
            raw.extend(data);stream.feed(decoder.decode(data))
            if b'\x1b[?u' in data:os.write(master,b'\x1b[?1;2c')
            if b'\x1b[6n' in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
    (root/'live.screen').write_text(view())
def send(data,delay=.15):os.write(master,data);pump(delay)
def wait(predicate,seconds=10):
    until=time.monotonic()+seconds
    while time.monotonic()<until and child.poll() is None:
        pump(.05)
        if predicate():return
    raise AssertionError(view())
def draft():
    rows=screen.display;top=max((i for i,s in enumerate(rows) if s.startswith('┌')),default=-1)
    result=[]
    for s in rows[top+1:]:
        if s.startswith('└'):break
        if s.startswith('│'):result.append(s[1:s.rfind('│')].rstrip())
    return '\n'.join(result)
def clear():send(b'\x1b[1;5H\x1b[1;6F\x7f',.08)
def paste(text):send(b'\x1b[200~'+text.encode()+b'\x1b[201~',.2)
try:
    seen=set();end=time.monotonic()+60
    while time.monotonic()<end and child.poll() is None:
        pump(.1);text=view()
        for prompt,reply in [('Trust this workspace?','y'),('Providers (comma-separated','0'),('Reviewer provider [','0'),('Default [','1')]:
            if prompt in text and prompt not in seen:seen.add(prompt);send((reply+'\r').encode(),.2)
        if 'Peritus ·' in text and ('Ready ·' in text or 'Offline ·' in text):break
    else:raise AssertionError('bootstrap failed\n'+view())
    (root/'opened.screen').write_text(view())
    message='Shared https://doc.qt.io for reading.'
    for mode in ('ordinary','selection','fragmented'):
        clear()
        if mode=='selection':send(b'\x1bOQ')
        start=time.monotonic()
        if mode=='fragmented':
            for part in (b'\x1b[2',b'00~Shared https://doc.',b'qt.io for reading.\x1b[20',b'1~'):send(part,.06)
        else:paste(message)
        wait(lambda:draft()==message,3)
        send(b' CONTINUED',.1);wait(lambda:draft()==message+' CONTINUED',3)
        checks.append({'case':mode,'passed':True,'seconds':time.monotonic()-start})
        (root/(mode+'.screen')).write_text(view())
    send(b'\x1bOQ');send(b'\x11',.6)
    if a.expect_old_quit:
        assert child.poll() is None, 'Baseline unexpectedly handled Ctrl-Q in selection mode'
        checks.append({'case':'quit-in-selection','baseline_quit_ignored':True})
        send(b'\x1b');send(b'\x11')
    child.wait(timeout=3)
    assert child.returncode==0, 'Client exited unsuccessfully'
    if not a.expect_old_quit:checks.append({'case':'quit-in-selection','passed':child.returncode==0})
    (root/'result.json').write_text(json.dumps({'checks':checks,'passed':True},indent=2));print(json.dumps(checks),flush=True)
finally:
    if child.poll() is None:child.terminate();child.wait(timeout=10)
    (root/'terminal.ansi').write_bytes(raw)
    for config in (root/'config/peritus').glob('*.toml'):
        import tomllib
        d=tomllib.loads(config.read_text())
        state=pathlib.Path(d['paths']['state_root'])
        if state.is_relative_to(root):
            instance=state/'daemon.instance'
            if not instance.is_file():continue
            fields=dict(line.split('=',1) for line in instance.read_text().splitlines())
            name=fields.get('endpoint','')
            assert re.fullmatch(r'peritus-[0-9a-f]{32}',name), 'Unexpected fixture endpoint'
            original=str(state/(name+'.sock'))
            endpoint=original if len(original.encode())<=107 else '/tmp/peritus-'+hashlib.sha256(b'peritus/unix-runtime-endpoint/v1\0'+original.encode()).hexdigest()[:32]+'/daemon.sock'
            if pathlib.Path(endpoint).exists():
                subprocess.run([str(a.binary.resolve()),'--endpoint',endpoint,'shutdown','--wait'],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=30,check=True)
    os.close(master)

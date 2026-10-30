#!/usr/bin/env python3
"""Drive real Peritus mouse paste against owned X11 or Wayland clipboard services."""
import argparse,codecs,fcntl,hashlib,json,os,pathlib,pty,select,signal,struct,subprocess,termios,time,zlib
import pyte

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary',type=pathlib.Path,required=True)
p.add_argument('--output',type=pathlib.Path,required=True)
p.add_argument('--backend',choices=['x11','wayland'],default='x11')
p.add_argument('--runtime-dir',type=pathlib.Path)
p.add_argument('--wayland-display',default='peritus-test')
p.add_argument('--expect-old',action='store_true')
a=p.parse_args();root=a.output.resolve();root.mkdir(mode=0o700,parents=True,exist_ok=False)
project=root/'project';project.mkdir();(project/'README').write_text('Disposable mouse paste fixture\n')
subprocess.run(['git','init','-q',str(project)],check=True);subprocess.run(['git','-C',str(project),'add','README'],check=True)
subprocess.run(['git','-C',str(project),'-c','user.name=Paste Replay','-c','user.email=paste@localhost','-c','core.hooksPath=/dev/null','commit','-qm','Disposable baseline'],check=True)
env=dict(os.environ,TERM='xterm-256color',TOKIO_WORKER_THREADS='2')
for name in ('config','state','cache','data'):env['XDG_'+name.upper()+'_HOME']=str(root/name)
env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
xvfb=None;owners=[];child=None;master=None;checks=[];raw=bytearray()
if a.backend=='wayland':
    assert a.runtime_dir and (a.runtime_dir/'.peritus-clipboard-replay').read_text().strip()=='owned'
    env.update(XDG_RUNTIME_DIR=str(a.runtime_dir.resolve()),WAYLAND_DISPLAY=a.wayland_display)
else:
    display=next(n for n in range(180,230) if not pathlib.Path(f'/tmp/.X11-unix/X{n}').exists() and not pathlib.Path(f'/tmp/.X{n}-lock').exists())
    xvfb=subprocess.Popen(['Xvfb',f':{display}','-screen','0','640x480x24','-nolisten','tcp'],stdout=subprocess.DEVNULL,stderr=(root/'xvfb.log').open('wb'))
    env['DISPLAY']=f':{display}'
    end=time.monotonic()+5
    while not pathlib.Path(f'/tmp/.X11-unix/X{display}').exists():
        assert time.monotonic()<end;time.sleep(.03)

def clipboard(data,primary=False,mime='UTF8_STRING'):
    if a.backend=='wayland':
        target='text/plain;charset=utf-8' if mime=='UTF8_STRING' else mime
        command=['wl-copy','--foreground','--type',target]+(['--primary'] if primary else [])
        read=['wl-paste','--no-newline','--type',target]+(['--primary'] if primary else [])
    else:
        selection='primary' if primary else 'clipboard'
        command=['xclip','-selection',selection,'-target',mime,'-in','-quiet']
        read=['xclip','-selection',selection,'-target',mime,'-out']
    owner=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=(root/f'clipboard-{len(owners)}.log').open('wb'),env=env)
    owners.append(owner);owner.stdin.write(data);owner.stdin.close()
    end=time.monotonic()+4
    while time.monotonic()<end:
        result=subprocess.run(read,env=env,capture_output=True,timeout=1)
        if result.returncode==0 and result.stdout==data:return owner
        time.sleep(.03)
    raise AssertionError('Real clipboard ownership was not established')

def view():return '\n'.join(screen.display)
def pump(seconds=.12):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        if select.select([master],[],[],min(.02,max(.001,end-time.monotonic())))[0]:
            try:data=os.read(master,65536)
            except OSError:return
            raw.extend(data);stream.feed(decoder.decode(data))
            if b'\x1b[?u' in data:os.write(master,b'\x1b[?1;2c')
            if b'\x1b[6n' in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
    (root/'live.screen').write_text(view())
def send(data,delay=.12):os.write(master,data);pump(delay)
def wait(predicate,seconds=5):
    end=time.monotonic()+seconds
    while child.poll() is None and time.monotonic()<end:
        pump(.04)
        if predicate():return
    raise AssertionError(view())
def composer_top():
    return max(i for i,s in enumerate(screen.display) if s.startswith('┌') and any(x in s for x in ('Chat','Plan','Build','Review','Message')))
def draft():
    result=[]
    for s in screen.display[composer_top()+1:]:
        if s.startswith('└'):break
        if s.startswith('│'):result.append(s[1:s.rfind('│')].rstrip())
    return '\n'.join(result)
def clear():send(b'\x1b[1;5H\x1b[1;6F\x7f',.06)
def typed(text):send(text.encode(),.08)
def button(button,x,y):
    return f'\x1b[<{button};{x};{y}M\x1b[<{button};{x};{y}m'.encode()
def right():send(button(2,5,composer_top()+2))
def middle():send(button(1,5,composer_top()+2))
def item(label,extra=b''):
    wait(lambda:any('│ '+label in row for row in screen.display))
    y,row=next((y,row) for y,row in enumerate(screen.display) if '│ '+label in row)
    x=row.index('│ '+label)+3;send(button(0,x,y+1)+extra,.1)
def check(name,condition):
    assert condition,name+'\n'+view()
    checks.append({'case':name,'passed':True});(root/(name+'.screen')).write_text(view())
def png():
    def chunk(tag,value):return struct.pack('!I',len(value))+tag+value+struct.pack('!I',zlib.crc32(tag+value)&0xffffffff)
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('!2I5B',2,2,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0\xff\0\0\0\xff\0'*2))+chunk(b'IEND',b'')

try:
    master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',36,120,0,0))
    child=subprocess.Popen([str(a.binary.resolve()),'open',str(project)],cwd=project,env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
    screen=pyte.Screen(120,36);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf8')('replace')
    seen=set();end=time.monotonic()+60
    while child.poll() is None and time.monotonic()<end:
        pump(.1);text=view()
        for prompt,reply in [('Trust this workspace?','y'),('Providers (comma-separated','0'),('Reviewer provider [','0'),('Default [','1')]:
            if prompt in text and prompt not in seen:seen.add(prompt);send((reply+'\r').encode(),.1)
        if 'Peritus ·' in text and ('Ready ·' in text or 'Offline ·' in text):break
    else:raise AssertionError('Bootstrap failed\n'+view())
    typed('KEEP');right()
    if a.expect_old:
        frozen=view();typed('IGNORED');check('reported-right-click-freeze',draft()=='KEEP' and 'Paste/F2/Esc returns' in frozen)
        send(b'\x1b');typed('RECOVERED');check('escape-restores-input',draft()=='KEEPRECOVERED')
    else:
        wait(lambda:'│ Paste' in view());typed('LIVE');check('right-click-keeps-input-live',draft()=='KEEPLIVE')
        message='Shared https://doc.qt.io for reading.'
        clipboard(message.encode());clear();right();item('Paste');wait(lambda:draft()==message);check('reported-link-pasted',draft()==message)
        typed(' EDITABLE');check('typing-after-paste',draft()==message+' EDITABLE')
        clear();typed('HEADTAIL');send(b'\x1b[H'+b'\x1b[C'*4);right();item('Paste');wait(lambda:draft()=='HEAD'+message+'TAIL');check('paste-at-existing-caret',draft()=='HEAD'+message+'TAIL')
        clear();typed('left αβ right');send(b'\x1b[H'+b'\x1b[C'*5+b'\x1b[1;2C'*2)
        right();item('Copy');wait(lambda:'Copied to the desktop clipboard' in view());check('copy-keeps-draft',draft()=='left αβ right')
        data=subprocess.check_output(['wl-paste','-n','--type','text'] if a.backend=='wayland' else ['xclip','-selection','clipboard','-target','UTF8_STRING','-out'],env=env);check('copy-exact-unicode',data.decode()=='αβ')
        clipboard('Ω雪'.encode());right();item('Paste');wait(lambda:draft()=='left Ω雪 right');check('replace-unicode-selection',draft()=='left Ω雪 right')
        clipboard(b'CLIPBOARD-ONLY');clipboard('PRIMARY Ω'.encode(),primary=True);clear();middle();wait(lambda:draft()=='PRIMARY Ω');check('middle-click-uses-primary',draft()=='PRIMARY Ω')
        owner=clipboard(b'SHOULD-NOT-OVERWRITE');clear();typed('KEEP');owner.send_signal(signal.SIGSTOP)
        try:
            right();item('Paste');typed('Y');check('input-live-during-read',draft()=='KEEPY')
        finally:owner.send_signal(signal.SIGCONT)
        wait(lambda:'Paste destination changed' in view());check('newer-draft-preserved',draft()=='KEEPY')
        owner=clipboard(b'WRONG-CARET');clear();typed('KEEP');owner.send_signal(signal.SIGSTOP)
        try:right();item('Paste');send(b'\x1b[H')
        finally:owner.send_signal(signal.SIGCONT)
        wait(lambda:'Paste destination changed' in view());check('newer-caret-preserved',draft()=='KEEP');typed('A');check('caret-still-editable',draft()=='AKEEP')
        owner=clipboard(b'BLOCKED-OWNER');clear();typed('KEEP');owner.send_signal(signal.SIGSTOP)
        try:
            right();item('Paste');typed('TIMEOUT');wait(lambda:'did not respond within five seconds' in view(),7);check('timeout-keeps-input-live',draft()=='KEEPTIMEOUT')
        finally:owner.send_signal(signal.SIGCONT)
        clear();right();send(b'\x1b[200~BRACKETED\x1b[201~');wait(lambda:draft()=='BRACKETED');check('terminal-paste-closes-menu',draft()=='BRACKETED')
        clipboard(b'/new');clear();right();item('Paste');wait(lambda:draft()=='/new');send(b'\r');wait(lambda:'Pasted commands do not execute' in view());check('pasted-command-inert',draft()=='/new')
        clear();typed('KEEP');right();send(b'\x1b');typed('ESC');check('escape-dismisses-menu',draft()=='KEEPESC')
        right();fcntl.ioctl(master,termios.TIOCSWINSZ,struct.pack('HHHH',24,80,0,0));screen.resize(lines=24,columns=80);os.kill(child.pid,signal.SIGWINCH);pump(.3);typed('R');check('resize-dismisses-menu',draft()=='KEEPESCR')
        clipboard(png(),mime='image/png');clear();typed('KEEP');right();item('Paste');wait(lambda:'requested format' in view() or 'not valid UTF-8' in view());check('nontext-keeps-draft',draft()=='KEEP');typed('T');check('nontext-does-not-freeze',draft()=='KEEPT')
        send(b'\x16');wait(lambda:'Attach image' in view() or 'Images' in view(),10);check('image-paste-still-opens-preview','KEEP' in view());send(b'\x1b')
    assert not list((root/'state/peritus/daemon/product-runs').glob('*.trace')), 'Paste must not invoke a model'
    assert not list((root/'state/peritus/daemon/workbench-v1/runs').glob('*.json')), 'Paste must not launch a coding run'
    assert (project/'README').read_text()=='Disposable mouse paste fixture\n'
    send(b'\x11');child.wait(timeout=10);assert child.returncode==0
    receipt={'backend':a.backend,'expect_old':a.expect_old,'checks':checks,'passed':True,'model_inference':False};(root/'result.json').write_text(json.dumps(receipt,indent=2));print(json.dumps(receipt),flush=True)
finally:
    if child is not None and child.poll() is None:child.terminate();child.wait(timeout=10)
    (root/'terminal.ansi').write_bytes(raw)
    for owner in owners:
        if owner.poll() is None:
            owner.send_signal(signal.SIGCONT);owner.terminate()
            try:owner.wait(timeout=3)
            except subprocess.TimeoutExpired:owner.kill();owner.wait(timeout=3)
    if xvfb is not None:xvfb.terminate();xvfb.wait(timeout=10)
    if master is not None:os.close(master)
    import tomllib
    for config in (root/'config/peritus').glob('*.toml'):
        state=pathlib.Path(tomllib.loads(config.read_text())['paths']['state_root']);instance=state/'daemon.instance'
        if not state.is_relative_to(root) or not instance.is_file():continue
        fields=dict(line.split('=',1) for line in instance.read_text().splitlines());name=fields['endpoint'];assert name.startswith('peritus-') and pathlib.Path(name).name==name
        original=str(state/(name+'.sock'));endpoint=original if len(original.encode())<=107 else '/tmp/peritus-'+hashlib.sha256(b'peritus/unix-runtime-endpoint/v1\0'+original.encode()).hexdigest()[:32]+'/daemon.sock'
        if pathlib.Path(endpoint).exists():subprocess.run([str(a.binary.resolve()),'--endpoint',endpoint,'shutdown','--wait'],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=30,check=True)

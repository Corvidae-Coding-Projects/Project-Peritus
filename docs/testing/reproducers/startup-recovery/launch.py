#!/usr/bin/env python3
"""Operate the actual packaged launcher/TUI; inject only real OS listen latency."""
import argparse, codecs, fcntl, json, os, pty, select, struct, subprocess, termios, time
from pathlib import Path
import pyte

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--delay-listen', action='store_true')
p.add_argument('--expect-timeout', action='store_true')
p.add_argument('--reuse-existing', action='store_true', help='Open an owned earlier fixture once and check its workspace lineage/trust')
a = p.parse_args()
root = a.output.resolve(); root.mkdir(parents=True, exist_ok=a.reuse_existing)
project = root/'project'
if not a.reuse_existing:
    project.mkdir(); (project/'notes.md').write_text('Retain this original document.\n')
env = os.environ.copy(); env.update({f'XDG_{k.upper()}_HOME':str(root/k) for k in ('config','state','cache')}); env['TERM']='xterm-256color'
binary = str(a.binary.resolve())
results = []

def state():
    return json.loads(max((root/'state/peritus/product-state').glob('state-*.json')).read_text())
def active():
    s=state(); return next(x for x in s['workspaces']['recent'] if x['workspace_id']==s['workspaces']['active_workspace_id'])
def stop():
    marker=root/'state/peritus/daemon/daemon.instance'
    if marker.exists():
        pid=next((line[4:] for line in marker.read_text().splitlines() if line.startswith('pid=')),None)
        try:os.kill(int(pid),0)
        except (ProcessLookupError,TypeError):return
    else:return
    sockets=list((root/'state/peritus/daemon').glob('*.sock'))
    if sockets:
        subprocess.run([binary,'--endpoint',str(sockets[0]),'shutdown','--wait'],env=env,check=True,capture_output=True,timeout=45)

def trust_settings():
    master,slave=pty.openpty()
    process=subprocess.Popen([binary,'workspaces'],cwd=project,env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
    raw=bytearray();selected=False;finished=False;deadline=time.monotonic()+30
    try:
        while process.poll() is None and time.monotonic()<deadline:
            if select.select([master],[],[],.1)[0]:
                try:raw.extend(os.read(master,65536))
                except OSError:break
            if b'Enter a number to switch' in raw and not selected:
                os.write(master,b't1\r');selected=True
            if b'Workspace is ready.' in raw and not finished:
                os.write(master,b'\r');finished=True
        process.wait(timeout=5)
        assert process.returncode==0 and finished,raw[-4000:]
        (root/'04-settings.txt').write_bytes(raw)
    finally:
        if process.poll() is None:process.terminate();process.wait(timeout=5)
        os.close(master)

def launch(label, answer='y', delay=False):
    master,slave=pty.openpty(); fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',42,160,0,0))
    args=[binary,'open',str(project)]
    if delay: args=['strace','-f','-e','trace=listen','-e','inject=listen:delay_enter=35s','-o',str(root/(label+'.strace')),*args]
    process=subprocess.Popen(args,cwd=project,env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
    screen=pyte.Screen(160,42);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf8')('replace');raw=bytearray();answered=set();start=time.monotonic();ready=False
    try:
        while time.monotonic()-start<100 and process.poll() is None:
            if select.select([master],[],[],.1)[0]:
                try:data=os.read(master,65536)
                except OSError:break
                raw.extend(data);stream.feed(decoder.decode(data))
                if b'\x1b[?u' in data:os.write(master,b'\x1b[?1;2c')
                if b'\x1b[6n' in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
            text='\n'.join(screen.display)
            for prompt,reply in [('Trust this workspace?', 'y'),('Trust the current folder for this workspace?',answer),('Providers (comma-separated','0')]:
                if prompt in text and prompt not in answered:
                    answered.add(prompt);os.write(master,(reply+'\r').encode())
            if 'Peritus ·' in text and ('Ready ·' in text or 'Idle — ready' in text):ready=True;break
        elapsed=time.monotonic()-start
        (root/(label+'.ansi')).write_bytes(raw);(root/(label+'.screen.txt')).write_text('\n'.join(screen.display))
        if a.expect_timeout:
            assert not ready and b'did not become ready within 30 seconds' in raw,raw[-4000:]
            results.append({'label':label,'expected_timeout_reproduced':True,'seconds':elapsed});return
        assert ready,raw[-6000:]
        if delay:
            assert elapsed>=35 and b'daemon is recovering saved state' in raw
        results.append({'label':label,'ready':True,'seconds':elapsed,'prompts':sorted(answered),'profile':active()})
        (root/'steps.json').write_text(json.dumps(results,indent=2))
        os.write(master,b'\x11');stop();process.wait(timeout=10)
    finally:
        if process.poll() is None:process.terminate();process.wait(timeout=10)
        os.close(master)

try:
    previous=active() if a.reuse_existing else None
    launch('01-start',delay=a.delay_listen)
    if a.reuse_existing:
        assert active()['workspace_id']==previous['workspace_id'] and active()['trust']==previous['trust'] and active()['direct_folder']
        assert 'Trust the current folder for this workspace?' not in results[-1]['prompts']
    if not a.expect_timeout and not a.reuse_existing:
        original=active()['workspace_id']
        subprocess.run(['git','init','-q','--initial-branch=main',str(project)],check=True)
        launch('02-git-init')
        assert active()['workspace_id']==original and active()['direct_folder'] and active()['trust']=='trusted'
        # Preserve the old directory and create a genuinely different directory at its path.
        project.rename(root/'original-directory');project.mkdir();(project/'notes.md').write_text('A replacement directory.\n')
        launch('03-replacement-declined',answer='n')
        assert active()['workspace_id']==original and active()['trust']=='restricted' and active()['direct_folder']
        trust_settings()
        launch('05-retrusted')
        assert active()['workspace_id']==original and active()['trust']=='trusted'
        assert (root/'original-directory/notes.md').read_text()=='Retain this original document.\n'
    (root/'result.json').write_text(json.dumps({'passed':True,'steps':results},indent=2));print(json.dumps({'passed':True,'steps':[{'label':x['label'],'seconds':x['seconds']} for x in results]}),flush=True)
finally:
    stop()

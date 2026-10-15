#!/usr/bin/env python3
"""Real CLI/TUI + real daemon loss/restart, with isolated offline state."""
import argparse, codecs, fcntl, json, os, pty, select, signal, struct, subprocess, termios, time
from pathlib import Path
import pyte
from wcwidth import wcwidth

p=argparse.ArgumentParser()
p.add_argument('--binary',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--expect-bug',action='store_true')
p.add_argument('--attach-checks',type=int,default=0,help='Open additional real terminal windows during the Plan request; requires --with-retry')
p.add_argument('--with-retry',action='store_true',help='Use the configured real account for one cancelled Plan and Retry; no mock provider')
a=p.parse_args();binary=a.binary.resolve();root=a.output.resolve();root.mkdir(mode=0o700,parents=True,exist_ok=False)
project=root/'project';project.mkdir();(project/'notes.md').write_text('Keep this document unchanged.\n')
env=os.environ.copy();env.update({f'XDG_{k.upper()}_HOME':str(root/k) for k in ('config','state','cache')});env['TERM']='xterm-256color'
master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',36,140,0,0))
proc=subprocess.Popen([str(binary),'open',str(project)],cwd=project,env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
screen=pyte.Screen(140,36);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf8')('replace');raw=bytearray();owned=None;results=[]

def display():
    rows=[]
    for y in range(screen.lines):
        row=[];skip=False
        for x in range(screen.columns):
            if skip:skip=False;continue
            text=screen.buffer[y][x].data or ' ';row.append(text);skip=wcwidth(text[0])==2
        rows.append(''.join(row))
    return '\n'.join(rows)
def pump(seconds=.1):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        if select.select([master],[],[],min(.05,end-time.monotonic()))[0]:
            try:data=os.read(master,65536)
            except OSError:return
            raw.extend(data);stream.feed(decoder.decode(data))
            if b'\x1b[?u' in data:os.write(master,b'\x1b[?1;2c')
            if b'\x1b[6n' in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
def save(label):
    (root/(label+'.screen.txt')).write_text(display());(root/(label+'.ansi')).write_bytes(raw)
def wait_until(condition,seconds=20):
    end=time.monotonic()+seconds
    while time.monotonic()<end and proc.poll() is None:
        pump()
        if condition():return True
    return False
def endpoint():return next((root/'state/peritus/daemon').glob('*.sock'))
def shutdown():
    sockets=list((root/'state/peritus/daemon').glob('*.sock'))
    if sockets:
        r=subprocess.run([str(binary),'--endpoint',str(sockets[0]),'shutdown','--wait'],capture_output=True,env=env,timeout=40)
        assert r.returncode==0,r.stderr
def start_daemon():
    cfg=max((root/'config/peritus').glob('peritus-*.toml'))
    log=(root/'restarted-daemon.log').open('ab')
    return subprocess.Popen([str(binary.with_name('peritusd')),'serve','--config',str(cfg)],env=env,stdout=log,stderr=log,start_new_session=True)
def draft():
    lines=display().splitlines();top=max(i for i,line in enumerate(lines) if line.startswith('┌'))
    return '\n'.join(line[1:line.rfind('│')].rstrip() for line in lines[top+1:] if line.startswith('│'))
def paste(text):os.write(master,b'\x1b[200~'+text.encode()+b'\x1b[201~');pump(.3)

def confirm_fresh_attachment(run,number):
    # A separate ordinary launcher/TUI must open the same run on its first handshake.
    other_master,other_slave=pty.openpty()
    fcntl.ioctl(other_slave,termios.TIOCSWINSZ,struct.pack('HHHH',36,140,0,0))
    child=subprocess.Popen([str(binary),'open',str(project),'--run',run],cwd=project,env=env,stdin=other_slave,stdout=other_slave,stderr=other_slave,start_new_session=True)
    os.close(other_slave);other_screen=pyte.Screen(140,36);other_stream=pyte.Stream(other_screen);decode=codecs.getincrementaldecoder('utf8')('replace');started=time.monotonic()
    try:
        while time.monotonic()-started<8 and child.poll() is None:
            if select.select([other_master],[],[],.05)[0]:
                data=os.read(other_master,65536);other_stream.feed(decode.decode(data))
                if b'\x1b[?u' in data:os.write(other_master,b'\x1b[?1;2c')
                if b'\x1b[6n' in data:os.write(other_master,f'\x1b[{other_screen.cursor.y+1};{other_screen.cursor.x+1}R'.encode())
            text='\n'.join(other_screen.display)
            if 'Peritus ·' in text and 'PAPER-CRANE' in text and ('Ctrl-C stops' in text or 'input received' in text):break
        else:raise AssertionError('Fresh window failed first-attachment deadline: '+text)
        elapsed=time.monotonic()-started
        log=(root/'state/peritus/logs/peritus-tui.log').read_text()
        lines=[line for line in log.splitlines() if f'pid={child.pid} ' in line]
        assert len(lines)==1 and 'generation=1 stage=connected' in lines[0],lines
        (root/f'fresh-attachment-{number}.screen.txt').write_text(text)
        os.write(other_master,b'\x11');child.wait(timeout=12);assert child.returncode==0
        return round(elapsed,3)
    finally:
        if child.poll() is None:child.terminate();child.wait(timeout=10)
        os.close(other_master)

try:
    answered=set();end=time.monotonic()+45
    while time.monotonic()<end and proc.poll() is None:
        pump();text=display()
        for prompt,reply in [('Trust this workspace?','y'),('Providers (comma-separated','1' if a.with_retry else '0'),('Choose a model number','manual gpt-5.6-sol')]:
            if prompt in text and prompt not in answered:answered.add(prompt);os.write(master,(reply+'\r').encode())
        if 'Peritus ·' in text and 'Ready ·' in text and list((root/'state/peritus/daemon').glob('*.sock')):
            pump(1);break
    assert 'Peritus ·' in display() and 'Ready ·' in display(),display()
    paste('retain café 雪');assert draft()=='retain café 雪';save('01-connected-draft')
    identity=dict(line.split('=',1) for line in (root/'state/peritus/daemon/daemon.instance').read_text().splitlines() if '=' in line)
    original=int(identity['pid']);shutdown();assert wait_until(lambda:'disconnect' in display().lower() or 'reconnecting' in display().lower(),10)
    paste(' — edited offline');assert draft()=='retain café 雪 — edited offline';pump(6);save('02-offline-draft')
    assert proc.poll() is None
    owned=start_daemon();assert wait_until(lambda:list((root/'state/peritus/daemon').glob('*.sock')) and owned.poll() is None,10)
    recovered=wait_until(lambda:'Ready ·' in display() and 'Offline ·' not in display(),15);save('03-recovery')
    if a.expect_bug:
        assert not recovered,'Original regression did not reproduce'
        os.write(master,b'\x12');assert wait_until(lambda:'Ready ·' in display() and 'Offline ·' not in display(),15)
        assert draft()=='retain café 雪 — edited offline';results.append({'failure_reproduced':True,'manual_ctrl_r_recovers':True})
    else:
        assert recovered,'Manual reconnect still required: '+display()
        assert draft()=='retain café 雪 — edited offline';results.append({'graceful_loss_auto_recovered':True,'draft_retained':True})
        # Abrupt process death exercises EOF, stale handles and actual WAL/session recovery.
        pump(11);os.kill(owned.pid,signal.SIGKILL);owned.wait(timeout=5)
        assert wait_until(lambda:'disconnect' in display().lower() or 'reconnecting' in display().lower(),10)
        paste(' + crash recovery');assert draft()=='retain café 雪 — edited offline + crash recovery'
        pump(3);owned=start_daemon()
        assert wait_until(lambda:'Ready ·' in display() and 'Offline ·' not in display(),20),display()
        assert draft()=='retain café 雪 — edited offline + crash recovery';save('04-crash-recovery')
        results.append({'abrupt_loss_auto_recovered':True,'draft_retained':True})
        diagnostic=root/'state/peritus/logs/peritus-tui.log';text=diagnostic.read_text();assert 'stage=read/write' in text and 'stage=connected' in text and 'stage=connect' in text
        assert 'retain café' not in text and 'edited offline' not in text
        results.append({'bounded_backoff_failures_logged':True,'draft_not_logged':True})
        if a.with_retry:
            runs=root/'state/peritus/daemon/workbench-v1/runs'
            def record():
                paths=list(runs.glob('*.json')) if runs.exists() else []
                return json.loads(paths[0].read_text()) if paths else None
            os.write(master,b'\x01\x0b');pump(.2)
            os.write(master,b'/plan Read notes.md and list the project directory, using the read-only workspace tools. Then reply with PAPER-CRANE. Do not change files or run commands.\r')
            assert wait_until(lambda:record() is not None and record()['phase'] not in (7,8,9,10,11) and 'Ctrl-C stops' in display(),60),display()
            save('05-active-plan')
            if a.attach_checks:
                run=record()['run_id']
                elapsed=[confirm_fresh_attachment(run,number+1) for number in range(a.attach_checks)]
                results.append({'first_attempt_attachments':a.attach_checks,'elapsed_seconds':elapsed})
            os.write(master,b'/stop\r')
            assert wait_until(lambda:record() is not None and record()['phase']==9,40),display()
            assert wait_until(lambda:'Cancelled —' in display(),15),display()
            pump(1) # Let the stop acknowledgement settle before the next deliberate control.
            cancelled=record();run=cancelled['run_id'];received=cancelled['interaction']['public_input_count'];save('05-cancelled-plan')
            os.write(master,b'/retry\r')
            assert wait_until(lambda:record()['phase']!=9 and not record()['user_cancelled'],40),display()
            assert record()['run_id']==run and record()['interaction']['public_input_count']==received
            save('06-retry-admitted');shutdown();owned.wait(timeout=10)
            stopped=record();requests=stopped['progress']['model_requests']
            assert wait_until(lambda:'disconnect' in display().lower() or 'reconnecting' in display().lower(),10)
            pump(2);owned=start_daemon()
            assert wait_until(lambda:'Offline ·' not in display() and 'Connecting ·' not in display() and ('Recovery required' in display() or 'Cancelled' in display()),25),display()
            pump(6);after=record();assert after['run_id']==run and after['interaction']['public_input_count']==received and after['progress']['model_requests']==requests
            save('07-retry-reconciled');results.append({'actual_plan_cancel_retry':True,'run_id':run,'input_count_unchanged':True,'no_retry_reissued_after_loss':True,'requests_at_restart':requests})
    os.write(master,b'\x11');proc.wait(timeout=12);assert proc.returncode==0
    assert (project/'notes.md').read_text()=='Keep this document unchanged.\n'
    (root/'result.json').write_text(json.dumps({'passed':True,'steps':results,'original_daemon_pid':original},indent=2));print(json.dumps({'passed':True,'steps':results}),flush=True)
except BaseException:
    save('99-failure')
    raise
finally:
    if proc.poll() is None:proc.terminate();proc.wait(timeout=10)
    os.close(master)
    if owned is not None and owned.poll() is None:
        shutdown();owned.wait(timeout=10)
    elif (root/'state/peritus/daemon/daemon.instance').exists() and list((root/'state/peritus/daemon').glob('*.sock')):
        shutdown()

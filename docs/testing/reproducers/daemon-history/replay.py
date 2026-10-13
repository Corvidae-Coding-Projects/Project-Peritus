#!/usr/bin/env python3
"""Real terminal, daemon, and persisted history replay; no mocked application."""
import argparse,fcntl,hashlib,json,os,pty,select,signal,struct,subprocess,termios,time
from pathlib import Path
parser=argparse.ArgumentParser(description='Operate a disposable long-history Peritus TUI; never starts inference.')
parser.add_argument('--root',type=Path,required=True)
parser.add_argument('--workspace',required=True)
parser.add_argument('--run',required=True)
parser.add_argument('--screen-parser',required=True)
a=parser.parse_args()
root=a.root.resolve()
if not (root/'real-test-fixture.json').is_file() or json.loads((root/'real-test-fixture.json').read_text()).get('owned_disposable_fixture') is not True:
 raise SystemExit('Refusing to operate or shut down a state directory without its disposable fixture marker.')

env=os.environ.copy();env.update({f'XDG_{k.upper()}_HOME':str(root/k) for k in ('state','config','cache')});env['TERM']='xterm-256color'
master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',40,160,0,0))
p=subprocess.Popen([str(root/'bin/peritus'),'open',a.workspace,'--run',a.run],stdin=slave,stdout=slave,stderr=slave,env=env,start_new_session=True);os.close(slave)
raw=bytearray();events=[];screens={};start=time.monotonic()
def pump(seconds):
 deadline=time.monotonic()+seconds
 while time.monotonic()<deadline:
  if select.select([master],[],[],.1)[0]:
   try:chunk=os.read(master,65536)
   except OSError:return
   raw.extend(chunk)
   if b'\x1b[6n' in chunk:os.write(master,b'\x1b[1;1R')
def screen(name):
 capture=root/f'{name}.ansi';capture.write_bytes(raw)
 result=subprocess.run([a.screen_parser,str(capture)],capture_output=True,text=True,check=True)
 screens[name]=result.stdout;(root/f'{name}.screen').write_text(result.stdout)
 print(json.dumps({'action':name,'elapsed':round(time.monotonic()-start,2),'footer':result.stdout.splitlines()[-2:],'draft_present':'DISCONNECT_REPLAY_UNSENT_DRAFT' in result.stdout}),flush=True)
 events.append({'action':name,'elapsed':time.monotonic()-start})
def endpoint():
 state=root/'state/peritus/daemon';i=dict(l.split('=',1) for l in (state/'daemon.instance').read_text().splitlines());ep=str(state/(i['endpoint']+'.sock'))
 if len(ep.encode())>107:ep='/tmp/peritus-'+hashlib.sha256(b'peritus/unix-runtime-endpoint/v1\0'+ep.encode()).hexdigest()[:32]+'/daemon.sock'
 return ep,int(i['pid'])
try:
 pump(5);screen('opened')
 if 'Recovery required' not in screens['opened']:raise RuntimeError('history did not reopen')
 os.write(master,b'DISCONNECT_REPLAY_UNSENT_DRAFT');pump(2);screen('draft')
 os.write(master,b'\x12');pump(5);screen('reconnected')
 if 'DISCONNECT_REPLAY_UNSENT_DRAFT' not in screens['reconnected']:raise RuntimeError('reconnect lost draft')
 os.write(master,b'\x7f'*len('DISCONNECT_REPLAY_UNSENT_DRAFT')+b'/runs\r');pump(4);screen('runs-panel')
 if 'Runs' not in screens['runs-panel']:raise RuntimeError('run panel did not open')
 os.write(master,b'\x1b');pump(2);screen('chat-panel')
 if 'Recovery required' not in screens['chat-panel']:raise RuntimeError('conversation did not reopen')
 os.write(master,b'DISCONNECT_REPLAY_UNSENT_DRAFT');pump(1)
 ep,daemon_pid=endpoint()
 before=Path(f'/proc/{daemon_pid}/stat').read_text().split();begin_cpu=int(before[13])+int(before[14]);begin=time.monotonic()
 pump(60);screen('sustained')
 after=Path(f'/proc/{daemon_pid}/stat').read_text().split();cpu=((int(after[13])+int(after[14])-begin_cpu)/os.sysconf('SC_CLK_TCK'))/(time.monotonic()-begin)*100
 if 'Offline' in screens['sustained']:raise RuntimeError('sustained session disconnected')
 os.kill(p.pid,signal.SIGSTOP);pump(17);os.kill(p.pid,signal.SIGCONT);pump(11);screen('delayed')
 if 'Offline' in screens['delayed']:raise RuntimeError('delayed session disconnected')
 r=subprocess.run([str(root/'bin/peritus'),'--endpoint',ep,'--timeout-seconds','15','shutdown','--wait'],env=env,capture_output=True,text=True,timeout=25)
 (root/'ui-shutdown.txt').write_text(r.stdout+r.stderr)
 if r.returncode:raise RuntimeError('owned daemon shutdown failed')
 pump(10);screen('persistent-failure')
 if 'Offline' not in screens['persistent-failure'] or 'early eof' not in screens['persistent-failure']:raise RuntimeError('disconnect reason disappeared after notice expired')
 os.write(master,b'\x12');pump(8);screen('restart')
 if 'DISCONNECT_REPLAY_UNSENT_DRAFT' not in screens['restart'] or 'Offline' in screens['restart']:raise RuntimeError('restart lost connection or draft')
 os.write(master,b'\x11');pump(2);p.wait(timeout=5)
 result={'status':'PASS','elapsed':time.monotonic()-start,'cpu_percent':cpu,'events':events,'tui_exit':p.returncode,'daemon_pid_before_restart':daemon_pid,'daemon_pid_after_restart':endpoint()[1]}
 print(json.dumps(result),flush=True);(root/'ui-result.json').write_text(json.dumps(result,indent=2))
finally:
 if p.poll() is None:p.kill();p.wait()
 os.close(master);(root/'ui-full.ansi').write_bytes(raw)

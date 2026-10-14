#!/usr/bin/env python3
"""Operate the real read-only Peritus composer in an owned disposable fixture."""
from pathlib import Path
import os,pty,fcntl,termios,struct,subprocess,select,time,codecs,json,hashlib
import pyte

fixture=os.environ.get('PERITUS_REPLAY_FIXTURE')
if not fixture:raise SystemExit('Set PERITUS_REPLAY_FIXTURE to an owned disposable fixture')
root=Path(fixture).resolve()
assert json.loads((root/'owned-fixture.json').read_text())['owned_disposable_fixture']
repo=root/'repo'
before_run_ids={p.stem for p in (root/'state/peritus/daemon/workbench-v1/runs').glob('*.json')}
env=os.environ.copy()
env.update({f'XDG_{n.upper()}_HOME':str(root/n) for n in ['config','state','cache']})
env['TERM']='xterm-256color'
master,slave=pty.openpty()
fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',42,160,0,0))
arguments=[os.environ.get('PERITUS_BINARY',str(Path.cwd()/'target/debug/peritus')),'open',str(repo)]
binary_sha256_at_launch=hashlib.sha256(Path(arguments[0]).read_bytes()).hexdigest()
proc=subprocess.Popen(arguments,stdin=slave,stdout=slave,stderr=slave,env=env,start_new_session=True)
screen=pyte.Screen(160,42);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf-8')('replace');raw=bytearray()
def pump(seconds=.2):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  if select.select([master],[],[],min(.05,max(.001,end-time.monotonic())))[0]:
   try:data=os.read(master,65536)
   except OSError:return
   raw.extend(data);stream.feed(decoder.decode(data))
   if b'\x1b[?u' in data:os.write(master,b'\x1b[?1;2c')
   if b'\x1b[6n' in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
def snapshot(name): (root/name).write_text('\n'.join(screen.display)+'\n')
def send(text):os.write(master,text.encode());pump(.3)
success=False
try:
 begin=time.monotonic();trusted=False
 while time.monotonic()-begin<35:
  pump(.3)
  if not trusted and any('Trust this workspace?' in s for s in screen.display):
   send('y\r');trusted=True
  if any('Ready' in s for s in screen.display):break
 snapshot('opened.screen.txt')
 assert any('Ready' in s for s in screen.display), 'Actual client not ready'
 mode=os.environ.get('PERITUS_PROMPT_MODE','plan')
 if mode not in ('plan','review'):raise SystemExit('Only read-only modes are allowed by this replay')
 send('/new\r');send('/'+mode+'\r')
 task=(os.environ.get('PERITUS_PROMPT_TASK') or 'In read-only planning mode, inspect only app.py and report its exact print string. This is Python work; Rust work is unrelated. Do not read or modify main.rs. Do not run commands, tests, Cargo, Clippy, rust_checks.py or any verification gates. Stay conversational and do not invoke run_pipeline.')
 send('\x1b[200~'+task+'\x1b[201~');send('\r')
 begin=time.monotonic();success=False;final_status=None
 while time.monotonic()-begin<int(os.environ.get('PERITUS_REPLAY_SECONDS','120')):
  pump(.5);snapshot('current.screen.txt')
  runfiles=list((root/'state/peritus/daemon/workbench-v1/runs').glob('*.json'))
  records=[json.loads(f.read_text()) for f in runfiles if f.stem not in before_run_ids]
  terminal=[v for v in records if v.get('status','').startswith(('Idle','Completed','Stopped','Recovery required'))]
  if terminal:
   final_status=terminal[-1]['status'];success=final_status.startswith(('Idle','Completed'));pump(1);snapshot('completed.screen.txt');break
  if proc.poll() is not None:break
 result={'conversation_finished':success,'handoff_status':final_status,'elapsed_seconds':round(time.monotonic()-begin,2),'binary_sha256':binary_sha256_at_launch}
 (root/'tui-result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
finally:
 if proc.poll() is None:
  if not success:
   send('\x03');pump(1)
  send('\x11')
  try:proc.wait(timeout=8)
  except subprocess.TimeoutExpired:proc.terminate();proc.wait(timeout=8)
 (root/'terminal.raw').write_bytes(raw)
 os.close(master);os.close(slave)

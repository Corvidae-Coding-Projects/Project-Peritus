#!/usr/bin/env python3
"""Actual agent working-state retention, distinct provenance, resolution and supersession."""
import argparse,codecs,fcntl,hashlib,json,os,pty,select,struct,subprocess,termios,time,signal
from pathlib import Path
import pyte
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--provider-state',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--runtime-data',type=Path);p.add_argument('--hold-provider',action='store_true');a=p.parse_args();a.model='gpt-6.1-sol';a.resume=None;b=a.output.resolve();b.mkdir(mode=0o700,parents=True,exist_ok=False);workspace=b/'project';workspace.mkdir();marker='DUPLICATE-WORDING-DISTINCT-PROVENANCE-829413'
(workspace/'alpha.txt').write_text(marker+'\n');(workspace/'beta.txt').write_text(marker+'\n')
env=dict(os.environ,TERM='xterm-256color',RAYON_NUM_THREADS='2',TOKIO_WORKER_THREADS='2');env.pop('NO_COLOR',None)
for d in ('config','state','cache','data'):env[f'XDG_{d.upper()}_HOME']=str(a.runtime_data.resolve() if d=='data' and a.runtime_data else b/d)
if not a.resume:
 state=b/'state/peritus/product-state';state.mkdir(parents=True);provider=json.loads(a.provider_state.read_text());provider['providers']['account_models']['codex-account']=a.model;(state/a.provider_state.name).write_text(json.dumps(provider))
binary=a.binary.resolve();screen=pyte.Screen(160,45);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf-8')('replace');raw=bytearray();master=None;client=None;run=None

def launch(resume=None):
 global master,client,screen,stream,decoder
 screen=pyte.Screen(160,45);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf-8')('replace');master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',45,160,0,0));cmd=[str(binary),'open',str(workspace)]+(['--run',resume] if resume else []);client=subprocess.Popen(cmd,cwd=workspace,env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
def view():return '\n'.join(screen.display)
def pump(seconds=.2):
 deadline=time.monotonic()+seconds
 while time.monotonic()<deadline:
  if select.select([master],[],[],min(.05,max(0,deadline-time.monotonic())))[0]:
   try:data=os.read(master,65536)
   except OSError:return
   raw.extend(data);stream.feed(decoder.decode(data));(b/'current.screen').write_text(view())
   if b'\x1b[?u' in data:os.write(master,b'\x1b[?1;2c')
   if b'\x1b[6n' in data:os.write(master,b'\x1b[1;1R')
def wait_for(condition,timeout):
 end=time.monotonic()+timeout
 while time.monotonic()<end and client.poll() is None:
  pump()
  if condition():return
 raise RuntimeError('Timed out or exited: '+view())
def ready():
 answered=False;end=time.monotonic()+100
 while time.monotonic()<end and client.poll() is None:
  pump()
  if 'Trust this workspace?' in view() and not answered:os.write(master,b'y\r');answered=True
  if 'Enter sends' in view() and any(marker in view() for marker in ('Ready','ready for your next message','Stopped','Recovery required')):return
 raise RuntimeError('Startup failed: '+view())
def submit(text):os.write(master,b'\x1b[200~'+text.encode()+b'\x1b[201~');pump(.15);os.write(master,b'\r')
def records():return [json.loads(p.read_text()) for p in (b/'state/peritus/daemon/workbench-v1/runs').glob('*.json')]
def owned_endpoint():
 p=b/'state/peritus/daemon/daemon.instance';identity=dict(l.split('=',1) for l in p.read_text().splitlines());pid=int(identity['pid']);endpoint=next(l.split()[4] for l in subprocess.check_output(['ss','-xlpH'],text=True).splitlines() if f'pid={pid},' in l);return endpoint,pid
def close_client():
 if client and client.poll() is None:os.write(master,b'\x11');pump(.5);client.wait(timeout=10)
 if master is not None:os.close(master)
def shutdown():
 try:ep,pid=owned_endpoint()
 except (OSError,StopIteration):return
 r=subprocess.run([str(binary),'--endpoint',ep,'shutdown','--wait'],env=env,capture_output=True,text=True,timeout=30);assert r.returncode==0,(r.stdout,r.stderr)
stopped_child=None;resume_at=None;heartbeat_seen=False
try:
 launch();ready();os.write(master,b'/effort low\r');pump(.3)
 task="This is a discussion-only display check. Do not read or modify files, run tools or start a pipeline. Explain how a command-line text editor can safely save a document, in eight short paragraphs with Markdown headings and bullet points where helpful. End with exactly PROGRESS-LIVE-DONE."
 submit(task);started=time.monotonic();seen=set();screens={};observed=[]
 def complete():
  global stopped_child,resume_at,heartbeat_seen
  if stopped_child is not None and resume_at is not None and time.monotonic()>=resume_at:
   os.killpg(stopped_child,signal.SIGCONT);resume_at=None
  rows=records()
  if not rows:return False
  row=rows[0]
  for x in row['interaction']['activities']:
   for stage,marker in [('accepted','Provider runtime accepted'),('reasoning','Provider is reasoning'),('generating','Receiving structured response:')]:
    if marker in x[2] and stage not in seen:
     seen.add(stage);screens[stage]=view()
     if stage=='reasoning' and a.hold_provider and stopped_child is None:
      _,daemon_pid=owned_endpoint()
      for proc in Path('/proc').iterdir():
       if not proc.name.isdigit():continue
       try:
        stat=(proc/'status').read_text();args=(proc/'cmdline').read_bytes()
       except (FileNotFoundError,PermissionError,ProcessLookupError):continue
       if f'PPid:\t{daemon_pid}\n' in stat and b'app-server' in args and b'stdio://' in args:
        stopped_child=int(proc.name);assert os.getpgid(stopped_child)==stopped_child,'native process group is not isolated';os.killpg(stopped_child,signal.SIGSTOP);resume_at=time.monotonic()+26;break
      assert stopped_child is not None,'owned native provider child not found'
     observed.append({'stage':stage,'elapsed':time.monotonic()-started,'file_exists':(workspace/'result.txt').exists(),'text':x[2]})
  heartbeat_seen |= any('Provider is reasoning' in x[2] and any(f'({n}s elapsed)' in x[2] for n in range(20,40)) for x in row['interaction']['activities'])
  return row['phase'] in (7,8,9,10,11) and any(x[1]==2 and 'PROGRESS-LIVE-DONE' in x[2] for x in row['interaction']['activities'])
 wait_for(complete,180)
 for stage,text in screens.items():(b/(stage+'.screen')).write_text(text)
 assert not (workspace/'result.txt').exists()
 (b/'observations.json').write_text(json.dumps({'seen':sorted(seen),'observations':observed},indent=2))
 if a.hold_provider:assert heartbeat_seen,'reasoning heartbeat did not retain observed phase'
 assert 'reasoning' in seen and 'generating' in seen,seen
 assert any(not x['file_exists'] and x['stage'] in ['reasoning','generating'] for x in observed),observed
 assert (workspace/'alpha.txt').read_text()==marker+'\n'
 assert (workspace/'beta.txt').read_text()==marker+'\n'
 result={'passed':True,'elapsed_seconds':time.monotonic()-started,'seen':sorted(seen),'observations':observed,'daemon_sha256':hashlib.sha256(binary.with_name('peritusd').read_bytes()).hexdigest(),'source_files_unchanged':True,'discussion_only':True,'held_provider':a.hold_provider,'heartbeat_seen':heartbeat_seen}
 (b/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result,indent=2))
finally:
 if stopped_child is not None:
  try:os.killpg(stopped_child,signal.SIGCONT)
  except ProcessLookupError:pass
 (b/'tui.ansi').write_bytes(raw)
 try:close_client()
 except Exception:pass
 try:shutdown()
 except Exception:pass

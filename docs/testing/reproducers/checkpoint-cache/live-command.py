#!/usr/bin/env python3
"""Real CLI/TUI→daemon→account provider→host commands, checkpoint readback and rewind."""
import argparse,codecs,fcntl,hashlib,json,os,pty,random,select,shutil,struct,subprocess,termios,time
from pathlib import Path
import pyte
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--probe',type=Path,required=True);p.add_argument('--provider-state',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--model',default='gpt-6-luna');p.add_argument('--resume');a=p.parse_args();b=a.output.resolve();workspace=b/'project';runtime=workspace/'.prescient/runtime';rng=random.Random(8419)
if not a.resume:
 b.mkdir(mode=0o700,parents=True,exist_ok=False);workspace.mkdir();(workspace/'source').mkdir();runtime.mkdir(parents=True)
 for i in range(256):(runtime/f'data-{i:04}.bin').write_bytes(rng.randbytes(256*1024))
 for i in range(10):(workspace/f'source/value-{i}.txt').write_text(f'BEFORE-{i}\n')
 (workspace/'show.py').write_text('from pathlib import Path\nfor i in range(10):\n    value=(Path("source")/f"value-{i}.txt").read_text()\n    if value != f"AFTER-{i}\\n": raise RuntimeError(f"unexpected source {i}: {value!r}")\nprint("CHECKPOINT-ORACLE-PASS")\n')
 (workspace/'AGENTS.md').write_text('# Disposable file-backed command application\nsource/value-*.txt are its ten values; show.py runs the actual application and reports success only when all ten expected values are present. .prescient/runtime contains unrelated local service data. Follow the exact user operations, use one grounding pass, preserve runtime bytes. No unit tests or unrelated gates.\n')
env=dict(os.environ,TERM='xterm-256color',RAYON_NUM_THREADS='2',TOKIO_WORKER_THREADS='2');env.pop('NO_COLOR',None)
for d in ('config','state','cache','data'):env[f'XDG_{d.upper()}_HOME']=str(b/d)
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
try:
 launch(a.resume);ready()
 if not a.resume:os.write(master,b'/effort low\r');pump(.4)
 task='Perform these exact file-backed application operations directly, one initial grounding check only. Read source/value-0.txt through source/value-9.txt and show.py. Use workspace_scope to declare .prescient/runtime before any command; retain that directory unchanged. Use workspace_write or workspace_patch to set each source/value-i.txt to exactly AFTER-i followed by a newline, for i=0..9. Then use run_command program python3 args ["show.py"] purpose verification twice, with no changes between the two runs. Both must print CHECKPOINT-ORACLE-PASS. Report those two observed outputs and finish. Do not create tests, run linters, run other commands, or initiate a design pipeline.'
 if not a.resume:submit(task)
 started=time.monotonic()
 def completed():
  rows=records()
  return bool(rows and rows[0]['phase'] in (7,8,9,10,11) and all((workspace/f'source/value-{i}.txt').read_text()==f'AFTER-{i}\n' for i in range(10)) and any(x[1]==2 and 'CHECKPOINT-ORACLE-PASS' in x[2] for x in rows[0]['interaction']['activities']))
 wait_for(completed,420);elapsed=time.monotonic()-started;record=records()[0];run=record['run_id'];(b/'completed.screen').write_text(view());trace=b/f'state/peritus/daemon/product-runs/{run}.trace'
 observations=[]
 with trace.open('rb') as f:
  while head:=f.read(9):
   size=int.from_bytes(head[1:],'little')
   if head[0]==7:observations.append(json.loads(f.read(size)))
   else:f.seek(size,1)
 commands=[o for o in observations if o['name']=='run_command'];assert len(commands)==2,[(o['name'],o['arguments']) for o in observations];assert all(not o['is_error'] and json.loads(o['arguments'])['purpose']=='verification' for o in commands)
 for o in commands:assert 'CHECKPOINT-ORACLE-PASS' in o['output']
 groups={}
 for line in trace.with_suffix('.efficiency.jsonl').read_text().splitlines():
  x=json.loads(line)
  if x.get('event')!='provider-accounting':continue
  key=(x['invocation'],x['model_requests']);g=groups.setdefault(key,{})
  if x.get('response_usage') is None:g.setdefault('start',x['at_unix_ms'])
  else:g.setdefault('end',x['at_unix_ms']);g.setdefault('usage',x['response_usage'])
 calls=sorted((g for g in groups.values() if 'start'in g),key=lambda g:g['start']);elapsed=(max(g.get('end',g['start']) for g in calls)-calls[0]['start'])/1000;gaps=[(nxt['start']-old['end'])/1000 for old,nxt in zip(calls,calls[1:]) if 'end'in old];assert not gaps or max(gaps)<30,('slow local handoff',gaps)
 rng=random.Random(8419)
 for i in range(256):assert hashlib.sha256((runtime/f'data-{i:04}.bin').read_bytes()).digest()==hashlib.sha256(rng.randbytes(256*1024)).digest()
 ids=[]
 for i in range(10):
  digest=bytearray(hashlib.sha256(b'peritus-workbench-automatic-checkpoint-v1\0'+bytes.fromhex(run)+b'\1'+f'source/value-{i}.txt'.encode()).digest()[:16]);digest[0]|=1;ids.append(digest.hex())
 (b/'checkpoint-ids.json').write_text(json.dumps(ids));close_client();shutdown();launch(run);ready()
 ep,pid=owned_endpoint();probe=subprocess.run([str(a.probe.resolve()),ep,run,str(b/'checkpoint-ids.json'),'restore-first'],env=env,capture_output=True,text=True,timeout=60);(b/'probe.stdout').write_text(probe.stdout);(b/'probe.stderr').write_text(probe.stderr);assert probe.returncode==0,(probe.returncode,probe.stderr);saved=json.loads(probe.stdout)
 assert len(saved['checkpoints'])==10
 for i,entry in enumerate(saved['checkpoints']):
  assert len(entry['paths'])==1;v=entry['paths'][0];assert v['path']==f'source/value-{i}.txt';assert v['before']['digest']==hashlib.sha256(f'BEFORE-{i}\n'.encode()).hexdigest();assert v['after']['digest']==hashlib.sha256(f'AFTER-{i}\n'.encode()).hexdigest()
 assert saved['restore']['status']=='Applied' and saved['restore']['restored']==['source/value-0.txt'];assert (workspace/'source/value-0.txt').read_text()=='BEFORE-0\n';assert all((workspace/f'source/value-{i}.txt').read_text()==f'AFTER-{i}\n' for i in range(1,10))
 summary={'passed':True,'seed':8419,'provider_model':a.model,'runtime_payload_bytes':67108864,'elapsed_seconds':elapsed,'qualification_boundary':record.get('interruption_cause'),'provider_calls':len(calls),'local_handoff_seconds':gaps,'runtime_all_256_hashes_unchanged':True,'source_checkpoints_verified':10,'fresh_daemon_restart':True,'selected_file_rewind_applied':True,'daemon_sha256':hashlib.sha256(binary.with_name('peritusd').read_bytes()).hexdigest()};(b/'result.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))
finally:
 (b/'tui.ansi').write_bytes(raw)
 try:close_client()
 except Exception:pass
 try:shutdown()
 except Exception:pass

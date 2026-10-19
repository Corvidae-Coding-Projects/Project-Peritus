#!/usr/bin/env python3
"""Actual agent working-state retention, distinct provenance, resolution and supersession."""
import argparse,codecs,fcntl,hashlib,json,os,pty,select,struct,subprocess,termios,time
from pathlib import Path
import pyte
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--provider-state',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.model='gpt-6.1-sol';a.resume=None;b=a.output.resolve();b.mkdir(mode=0o700,parents=True,exist_ok=False);workspace=b/'project';workspace.mkdir();marker='DUPLICATE-WORDING-DISTINCT-PROVENANCE-829413'
(workspace/'alpha.txt').write_text(marker+'\n');(workspace/'beta.txt').write_text(marker+'\n')
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
 launch();ready();os.write(master,b'/effort low\r');pump(.3)
 task="""This is a small working-memory operation, not a code change. Do not modify workspace files or start a coding pipeline. Do one workspace_list, then read alpha.txt and beta.txt. Use their actual observation handles with context_update to do exactly this:
1. In one update at the current base_revision create two distinct open observation entries: alpha_marker bound with validity=files to alpha.txt and its own supporting read; beta_marker bound with validity=files to beta.txt and its own supporting read. Give BOTH the EXACT SAME text: "The shared source marker is DUPLICATE-WORDING-DISTINCT-PROVENANCE-829413." Their provenance is different and both should be retained.
2. In that same update create open plan old_plan with exact text RESOLVED-OLD-PLAN-114997, validity=task, and open hypothesis old_hypothesis with exact text SUPERSEDED-OLD-HYPOTHESIS-229981, validity=task. Support these with the alpha read and leave files empty for task validity.
3. In a SECOND context_update using the returned current base_revision update old_plan to status=resolved, preserving its text/supports/task validity. Add open hypothesis current_hypothesis with exact text CURRENT-REPLACEMENT-441117, validity=task and supersedes=old_hypothesis, supported by the alpha read. Keep the two marker observations open and separate.
4. Use context_read with observation_ids=[], query=null, cursor=null, offset=0,max_bytes=65536 to inspect the retained state. The old entries must still be durable with resolved/superseded statuses. End with exactly MEMORY-ORACLE-DONE. Do not perform any other work. Populate all required context_update fields from the real schema and actual source handles."""
 submit(task);started=time.monotonic()
 def complete():
  rows=records()
  return bool(rows and rows[0]['phase'] in (7,8,9,10,11) and any(x[1]==2 and 'MEMORY-ORACLE-DONE' in x[2] for x in rows[0]['interaction']['activities']))
 wait_for(complete,300);record=records()[0];run=record['run_id'];trace=b/f'state/peritus/daemon/product-runs/{run}.trace'
 inspection=subprocess.run([str(binary.with_name('peritusd')),'context-inspect','--trace',str(trace),'--run',run,'--workspace',record['workspace_id'],'--role','writer'],capture_output=True,text=True,timeout=30);(b/'context-inspect.json').write_text(inspection.stdout);(b/'context-inspect.stderr').write_text(inspection.stderr);assert inspection.returncode==0,inspection.stderr;view_data=json.loads(inspection.stdout)
 bodies=[block['text'] for message in view_data['readable_messages'] for block in message['content'] if block.get('kind')=='text' and block.get('text','').startswith('LOCAL WORKING STATE')];assert len(bodies)==1,bodies;body=bodies[0];entries=[json.loads(line) for line in body.splitlines() if line.startswith('{')]
 same=[entry for entry in entries if entry.get('text')=='The shared source marker is '+marker+'.'];assert len(same)==2,same;assert same[0]['supports']!=same[1]['supports'],same
 assert 'RESOLVED-OLD-PLAN-114997' not in body and 'SUPERSEDED-OLD-HYPOTHESIS-229981' not in body and body.count('CURRENT-REPLACEMENT-441117')==1
 observations=[]
 with trace.open('rb') as f:
  while head:=f.read(9):
   size=int.from_bytes(head[1:],'little')
   if head[0]==7:observations.append(json.loads(f.read(size)))
   else:f.seek(size,1)
 updates=[o for o in observations if o['name']=='context_update'];reads=[o for o in observations if o['name']=='context_read'];assert len(updates)>=2 and all(not o['is_error'] for o in updates),updates;assert reads and all(not o['is_error'] for o in reads)
 final=reads[-1]['output'];assert 'RESOLVED-OLD-PLAN-114997' in final and 'SUPERSEDED-OLD-HYPOTHESIS-229981' in final and 'resolved' in final and 'superseded' in final
 assert (workspace/'alpha.txt').read_text()==marker+'\n' and (workspace/'beta.txt').read_text()==marker+'\n'
 summary={'passed':True,'elapsed_seconds':time.monotonic()-started,'distinct_source_entries_preserved':True,'terminal_entries_omitted_from_active_view':True,'terminal_entries_remain_exactly_retrievable':True,'context_updates':len(updates),'source_files_unchanged':True,'daemon_sha256':hashlib.sha256(binary.with_name('peritusd').read_bytes()).hexdigest()};(b/'result.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))
finally:
 (b/'tui.ansi').write_bytes(raw)
 try:close_client()
 except Exception:pass
 try:shutdown()
 except Exception:pass

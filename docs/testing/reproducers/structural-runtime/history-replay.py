#!/usr/bin/env python3
"""Restart real daemons against copied governed history, disposable indexes, and damaged artifacts."""
import argparse,hashlib,json,os,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('root',type=Path);p.add_argument('binaries',type=Path);a=p.parse_args();root=a.root.resolve();base=a.binaries.resolve();info=json.loads((root/'instance.json').read_text());config=Path(info['config']);state=root/'state/peritus/daemon';runs=state/'workbench-v1/runs';run=info['run'];record=runs/(run+'.json');saved=json.loads((root/'original-run.json').read_text());pointer=saved['resume_artifact'];artifact=state/'workbench-v1/continuations'/(bytes(pointer['digest']).hex()+'.bin');raw=artifact.read_bytes();assert hashlib.sha256(raw).digest()==bytes(pointer['digest']);results=[]
env=os.environ.copy();env.update({f'XDG_{n.upper()}_HOME':str(root/n)for n in ('config','state','cache')})
def metrics(pid):
 out={}
 for line in Path(f'/proc/{pid}/status').read_text().splitlines():
  if line.startswith(('VmRSS:','VmHWM:')):out[line.split(':')[0]]=int(line.split()[1])*1024
 out['io']=dict((x.split(':')[0],int(x.split(':')[1]))for x in Path(f'/proc/{pid}/io').read_text().splitlines());return out
def start(binary,label):
 log=(root/(label+'-daemon.log')).open('w');proc=subprocess.Popen([str(binary),'serve','--config',str(config)],env=env,stdout=log,stderr=subprocess.STDOUT);started=time.monotonic();endpoint=None
 while time.monotonic()-started<25:
  if proc.poll()is not None:raise RuntimeError((root/(label+'-daemon.log')).read_text()[-5000:])
  endpoint=next((line.split()[4]for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines()if f'pid={proc.pid},'in line),None)
  if endpoint:
   test=subprocess.run([str(base/'candidate-bin/peritus'),'--endpoint',endpoint,'status'],env=env,capture_output=True,timeout=10)
   if test.returncode==0:break
  time.sleep(.02)
 else:raise RuntimeError('daemon never ready')
 return proc,log,endpoint,time.monotonic()-started
def show(endpoint):
 out=subprocess.run([str(base/'candidate-bin/peritus'),'--endpoint',endpoint,'--json','runs','show','--run',run],env=env,capture_output=True,text=True,timeout=25);return out
index=runs/'.index'/(run+'.json');original=None
for label,variant in [('baseline','baseline'),('candidate-cold','candidate'),('candidate-warm','candidate'),('broken-index','candidate'),('semantic-index','candidate'),('stale-index','candidate'),('corrupt-continuation','candidate'),('missing-continuation','candidate')]:
 record.parent.mkdir(exist_ok=True);v=dict(saved);v['finding_state']='Retained bounded load fixture. '* (512*1024);desired=json.dumps(v).encode();
 if not record.exists()or record.read_bytes()!=desired:record.write_bytes(desired)
 artifact.write_bytes(raw)
 # Restore the same source bytes without rewriting its inode for warm/index-only cases.
 if label=='baseline': original=record.read_bytes()
 if label in ('baseline','candidate-cold'):index.unlink(missing_ok=True)
 elif label=='broken-index':index.write_text('{BROKEN')
 elif label=='semantic-index':
  indexed=json.loads(index.read_text());indexed['hint']['workspace_id']='0'*32;index.write_text(json.dumps(indexed))
 elif label=='stale-index':
  indexed=json.loads(index.read_text());indexed['stamp']['length']+=1;index.write_text(json.dumps(indexed))
 elif label=='corrupt-continuation':
  bad=bytearray(raw);bad[len(bad)//2]^=1;artifact.write_bytes(bad)
 elif label=='missing-continuation':artifact.unlink()
 proc,log,endpoint,elapsed=start(base/(variant+'-bin/peritusd'),label)
 try:
  before=metrics(proc.pid);now=time.monotonic();shown=show(endpoint);duration=time.monotonic()-now;(root/(label+'-show.json')).write_text(shown.stdout);(root/(label+'-show.stderr')).write_text(shown.stderr)
  damaged=label.endswith('continuation')
  if damaged:
   assert shown.returncode!=0 and not record.exists(),(label,shown.stdout,shown.stderr)
   # A repaired load is retryable within this same process, without restarting or effects replay.
   artifact.write_bytes(raw);record.write_bytes(original);repaired=show(endpoint);assert repaired.returncode==0,(label,repaired.stderr)
  else:
   assert shown.returncode==0,(label,shown.stdout,shown.stderr)
   result=json.loads(shown.stdout)['result'];assert result['run_id']==run and result['summary']==saved['summary']
   assert record.exists(),label
  results.append({'case':label,'startup_seconds':elapsed,'first_show_seconds':duration,'at_ready':before,'after_show':metrics(proc.pid),'show_exit':shown.returncode,'index_bytes':index.stat().st_size if index.exists()else None,'status':'PASS'})
 finally:
  stop=subprocess.run([str(base/'candidate-bin/peritus'),'--endpoint',endpoint,'shutdown','--wait'],env=env,capture_output=True,text=True,timeout=30);assert stop.returncode==0,stop.stderr;proc.wait(timeout=10);log.close()
 (root/'history-results.json').write_text(json.dumps(results,indent=2))
 print(label,'PASS',flush=True)
# Restore only the owned fixture's correct state for later performance review.
artifact.write_bytes(raw);record.write_text(json.dumps(saved))

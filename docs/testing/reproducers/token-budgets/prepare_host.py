#!/usr/bin/env python3
"""Copy a CLOSED backup into a private host replay; never dispatch effects or inference."""
import argparse,hashlib,json,sqlite3,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('backup',type=Path);p.add_argument('host',type=Path);p.add_argument('--run',required=True);p.add_argument('--workspace-copy',type=Path,required=True)
a=p.parse_args();backup=a.backup.resolve();host=a.host.resolve();workspace=a.workspace_copy.resolve()
if host.exists():raise SystemExit('Use a fresh host directory')
if not workspace.is_dir():raise SystemExit('First prepare a disposable complete workspace copy')
record=json.loads((backup/'daemon/workbench-v1/runs'/f'{a.run}.json').read_bytes())
source=backup/'daemon/product-runs'/f'{a.run}.context/writer'
root=host/'replay.context/writer';root.mkdir(parents=True)
# SQLite backup preserves a standalone journal, independent of any source-side WAL.
with sqlite3.connect(f'file:{source}/journal.sqlite3?mode=ro',uri=True)as old,sqlite3.connect(root/'journal.sqlite3')as new:old.backup(new)
subprocess.run(['cp','-a','--reflink=auto',str(source/'artifacts'),str(root/'artifacts')],check=True)
subprocess.run(['cp','-a','--reflink=auto',str(backup/'daemon/product-runs'/f'{a.run}.trace'),str(host/'replay.trace')],check=True)
configs=sorted((backup/'config').glob('peritus-*.toml'))
config=tomllib.loads(configs[-1].read_text())
provider=next(v for v in config['providers']if v['kind']=='codex-runtime')
settings={k:provider[k]for k in ('kind','executable','profile')}
settings.update({'run_id':record['run_id'],'workspace_id':record['workspace_id'],'workspace':str(workspace)})
(host/'settings.json').write_text(json.dumps(settings))
with sqlite3.connect(f'file:{root}/journal.sqlite3?mode=ro',uri=True)as c:
 checkpoint=json.loads(c.execute('SELECT value FROM state_records WHERE namespace=3401').fetchone()[0])
 def artifact(desc):
  digest=bytes(desc['digest']).hex();data=(root/'artifacts/objects/sha256'/digest[:2]/digest).read_bytes()
  assert len(data)==desc['bytes']and hashlib.sha256(data).hexdigest()==digest
  return data
 sources=json.loads(artifact(checkpoint['source_index']));events=[]
 for sequence,frame in c.execute('SELECT sequence,frame FROM events WHERE sequence>? ORDER BY sequence',(checkpoint['through_event'],)):
  assert frame[:4]==b'PRTS'and int.from_bytes(frame[12:16],'big')==len(frame)-16
  event=json.loads(frame[16:]);events.append(event)
  if 'observation'in event:sources.append(event['observation']['observation'])
 (host/'snapshot.json').write_text(json.dumps({'checkpoint':checkpoint,'events':events,'sources':sources}))
print(json.dumps({'host':str(host),'workspace':str(workspace),'sources':len(sources),'tail_events':len(events),'provider_requests':0,'effects_dispatched':0}))

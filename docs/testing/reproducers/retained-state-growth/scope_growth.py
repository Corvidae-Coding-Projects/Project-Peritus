#!/usr/bin/env python3
"""Actual ScopedBaseline public consumer, bounded original/final fixtures; no app mocks."""
import argparse,hashlib,json,shutil,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--consumer',required=True,type=Path);p.add_argument('--output',required=True,type=Path);p.add_argument('--final',action='store_true');a=p.parse_args()
r=a.output.resolve();r.mkdir(parents=True,exist_ok=False);consumer=a.consumer.resolve();records=[]
(r/'binary.json').write_text(json.dumps({'path':str(consumer),'sha256':hashlib.sha256(consumer.read_bytes()).hexdigest()}))
def call(case,*args):
 command=[str(consumer),*map(str,args)];t=time.monotonic();out=subprocess.run(command,capture_output=True,text=True,timeout=120)
 try:v=json.loads(out.stdout)
 except ValueError:v={'ok':False,'stdout':out.stdout,'stderr':out.stderr}
 event={'command':command,'seconds':round(time.monotonic()-t,3),'exit':out.returncode,'result':v}
 with(case/'operations.jsonl').open('a')as f:f.write(json.dumps(event)+'\n')
 return v
for name in ('5000-files','66-levels','previews-over-64mib'):
 case=r/name;root=case/'workspace';root.mkdir(parents=True);state=case/'state';state.mkdir();task=root/'task';task.mkdir();outside=root/'unrelated.txt';outside.write_text('UNRELATED MUST REMAIN\n');outside_sha=hashlib.sha256(outside.read_bytes()).hexdigest();declared=[]
 if name=='66-levels':
  leaf=task
  for n in range(66):leaf=leaf/f'd{n:02}';leaf.mkdir()
  target=leaf/'last.txt';target.write_text('EARLIEST BASELINE LAST\n');declared=[str(v.relative_to(root))for v in task.rglob('*')]
 else:
  count=5000 if name=='5000-files'else 4090
  for n in range(count):
   target=task/f'f{n:04}.txt';prefix=f'EARLIEST BASELINE FILE {n:04}\n'.encode();target.write_bytes(prefix+(b'x'*(16384-len(prefix))if name=='previews-over-64mib'else b''));declared.append(str(target.relative_to(root)))
  target=task/f'f{count-1:04}.txt'
 descriptor=state/'descriptor.json';journal=state/'journal.jsonl'
 assert call(case,'new',root,journal,73,'[]',descriptor).get('ok')
 out=call(case,'enroll',descriptor,'task');satisfied=False;detail={'enrollment':out,'files':sum(v.is_file()for v in task.rglob('*')),'max_depth':max(len(v.relative_to(task).parts)for v in task.rglob('*')),'input_bytes':sum(v.stat().st_size for v in task.rglob('*')if v.is_file())}
 if out.get('ok'):
  observed=call(case,'inspect',descriptor);assert observed.get('ok'),observed
  assert set(declared).issubset(set(observed['paths']));assert not observed['changed_paths']
  original=target.read_bytes();target.write_bytes(b'CHANGED AFTER ADMISSION\n'+original)
  reopened=state/'reopened.json';assert call(case,'serialize',descriptor,reopened).get('ok')
  observed=call(case,'inspect',reopened);assert observed.get('ok'),observed;assert set(observed['changed_paths'])=={str(target.relative_to(root))}
  assert str(target.relative_to(root))in observed['diff']and'EARLIEST BASELINE'in observed['diff']
  target.write_bytes(original);restored=call(case,'inspect',reopened);assert restored.get('ok')and not restored['changed_paths']
  detail.update({'tracked_paths':len(observed['paths']),'journal_bytes':journal.stat().st_size,'change_detected_after_reopen':True,'exact_restoration_unchanged':True});satisfied=True
 assert hashlib.sha256(outside.read_bytes()).hexdigest()==outside_sha
 row={'case':name,'contract_satisfied':satisfied,'mode':'final'if a.final else'original','detail':detail};records.append(row);(case/'result.json').write_text(json.dumps(row,indent=2));print(json.dumps(row),flush=True)
summary={'cases':len(records),'satisfied':sum(v['contract_satisfied']for v in records),'expected_original_failures':0 if a.final else 3};(r/'summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary),flush=True)
if a.final and summary['satisfied']!=3:raise SystemExit(1)
if not a.final and summary['satisfied']!=0:raise SystemExit('An original boundary did not reproduce; inspect raw evidence')

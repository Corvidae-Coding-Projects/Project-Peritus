#!/usr/bin/env python3
"""Independent fixture/hash oracles driving the actual Rust host-tool/context consumer."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import tempfile

p=argparse.ArgumentParser();p.add_argument('--consumer',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
consumer=a.consumer.resolve();out=a.output.resolve();out.mkdir(parents=True,exist_ok=True);receipts=[]
rng=random.Random(0xC0DE)

def invoke(label,spec,mode='probe'):
 path=out/(label+'.json');path.write_text(json.dumps(spec))
 result=subprocess.run([str(consumer),mode,str(path)],capture_output=True,text=True,timeout=60)
 (out/(label+'.stdout')).write_text(result.stdout);(out/(label+'.stderr')).write_text(result.stderr)
 assert result.returncode==0,(label,result.returncode,result.stderr)
 data=json.loads(result.stdout);receipts.append({'case':label,'mode':mode,'exit':result.returncode})
 (out/'results.json').write_text(json.dumps(receipts,indent=2));return data

with tempfile.TemporaryDirectory(prefix='peritus-grounding-real-') as directory:
 base=Path(directory);root=base/'workspace';root.mkdir();state=base/'private';state.mkdir()
 (root/'alpha.txt').write_text('ALPHA-KEEP-17\n');(root/'beta.txt').write_text('BETA-FIRST-29\n')
 spec={'root':str(root),'trace':str(state/'run.trace'),'task':'Inspect alpha.txt and beta.txt only.'}
 spec['operations']=[['workspace_list',{}],['workspace_read',{'path':'alpha.txt'}],['workspace_read',{'path':'beta.txt'}]]
 data=invoke('initial-observations',spec)
 assert data['before']['required_tool']=='workspace_list'
 for item in data['outputs'][1:]:
  assert not item['is_error'];value=item['output'];path=root/value['path']
  assert value['source_sha256']==hashlib.sha256(path.read_bytes()).hexdigest()
 spec['operations']=[];data=invoke('fresh-process-reuse',spec)
 assert data['before']['required_tool'] is None and data['before']['completion_blocker'] is None
 assert 'revalidated 2 ' in data['before']['guidance']
 beta=root/'beta.txt';before=beta.stat();beta.write_text('BETA-OTHER-37\n');os.utime(beta,ns=(before.st_atime_ns,before.st_mtime_ns))
 spec['operations']=[['workspace_read',{'path':'beta.txt'}]];data=invoke('same-size-same-mtime-refresh',spec)
 assert 'beta.txt' in data['before']['guidance'] and 'revalidated 1 ' in data['before']['guidance']
 assert 'BETA-OTHER-37' in data['outputs'][0]['output']['content']
 assert data['outputs'][0]['output']['source_sha256']==hashlib.sha256(beta.read_bytes()).hexdigest()
 spec['private_paths']=['alpha.txt'];spec['operations']=[['workspace_read',{'path':'alpha.txt'}]]
 data=invoke('current-policy-revocation',spec);assert data['outputs'][0]['is_error']
 assert 'ALPHA-KEEP-17' not in json.dumps(data) and 'revalidated 1 ' in data['before']['guidance']
 spec.pop('private_paths');spec['operations']=[];data=invoke('restore-policy',spec);assert data['before']['required_tool'] is None
 other=dict(spec,workspace=99);data=invoke('different-workspace-scope',other);assert data['before']['required_tool']=='workspace_list'
 other_root=base/'other-workspace';other_root.mkdir();(other_root/'alpha.txt').write_text('OTHER-WORKSPACE\n')
 other=dict(spec,root=str(other_root),command_state=str(base/'other-process'))
 data=invoke('different-root',other);assert data['before']['required_tool']=='workspace_list'
 beta.unlink();spec['operations']=[];data=invoke('deleted-source',spec);assert 'beta.txt' in data['before']['guidance']
 beta.write_text('RECREATED-43\n');spec['operations']=[['workspace_read',{'path':'beta.txt'}]]
 data=invoke('recreated-source',spec);assert 'RECREATED-43' in data['outputs'][0]['output']['content']
 outside=base/'outside.txt';outside.write_text('OUTSIDE-PRIVATE-89\n');beta.unlink();beta.symlink_to(outside)
 data=invoke('symlink-replacement',spec);assert data['outputs'][0]['is_error'] and 'OUTSIDE-PRIVATE-89' not in json.dumps(data)
 beta.unlink();beta.write_text('BETA-RESTORED-53\n')
 spec['operations']=[['workspace_write',{'path':'alpha.txt','content':'MUST-NOT-WRITE'}]]
 data=invoke('cached-read-does-not-grant-write',spec);assert data['outputs'][0]['is_error']
 assert (root/'alpha.txt').read_text()=='ALPHA-KEEP-17\n'
 skills=['engineering-workflow','real-testing','computer-automation','developer','reviewer','architect','rust-reliability']
 spec['operations']=[['skill_read',{'skill':name,'path':'SKILL.md','max_bytes':524288}] for name in skills]
 data=invoke('on-demand-guidance',spec)
 for item in data['outputs']:
  assert not item['is_error'];value=item['output'];body=value['bytes'].encode()
  assert value['next_offset'] is None and value['total_bytes']==len(body)
  assert value['sha256']==hashlib.sha256(body).hexdigest()
 # Generate distinct source names and same-size mutations; each second process reuses the
 # first file while only the changed target is read through the actual tool interface.
 for i,name in enumerate(['name space.txt','café.txt','nested/target.txt','brackets[2].txt']):
  file=root/name;file.parent.mkdir(parents=True,exist_ok=True);first=f'INPUT-{rng.randrange(10000,99999)}\n';second=f'OTHER-{rng.randrange(10000,99999)}\n';file.write_text(first)
  spec['operations']=[['workspace_read',{'path':name}]];invoke(f'generated-{i}-read',spec)
  stamp=file.stat();file.write_text(second);os.utime(file,ns=(stamp.st_atime_ns,stamp.st_mtime_ns))
  data=invoke(f'generated-{i}-refresh',spec)
  assert second.strip() in data['outputs'][0]['output']['content']
 # A root observed empty must lose that credit as soon as a nonempty listing is observed.
 empty=base/'empty';empty.mkdir();empty_spec=dict(spec,root=str(empty),trace=str(state/'empty.trace'),command_state=str(base/'empty-process'),operations=[['workspace_list',{}]])
 data=invoke('empty-root',empty_spec);assert data['after']['required_tool'] is None
 (empty/'new.txt').write_text('NEW-CURRENT-INPUT\n')
 data=invoke('nonempty-root-refresh',empty_spec);assert data['after']['required_tool']=='workspace_read'
 empty_spec['operations']=[];data=invoke('nonempty-root-fresh-process',empty_spec);assert data['before']['required_tool']=='workspace_read'
 # Force only the public context library's declared view target, without an inference call.
 # The hosted provider object remains real; this case makes no claims about its actual window.
 (root/'focus.txt').write_text('CRITICAL-TASK-FACT-7391\n')
 for i in range(9):(root/f'noise-{i}.txt').write_text((f'old observation {i} '+('x'*110)+'\n')*90)
 context_spec=dict(spec,trace=str(state/'pressure.trace'),task='Use focus.txt as the current task input. Retain its exact fact and relevant real-testing guidance.',capacity=18000,generation=1)
 context_spec['operations']=[['workspace_list',{}],['workspace_read',{'path':'focus.txt'}],['skill_read',{'skill':'real-testing','path':'SKILL.md'}]]
 context_spec['operations'] += [['workspace_read',{'path':f'noise-{i}.txt'}] for i in range(9)]
 context_spec['operations'] += [['workspace_read',{'path':'focus.txt'}],['workspace_read',{'path':'focus.txt'}]]
 data=invoke('context-pressure-and-duplicate-reads',context_spec,'context')
 last=data['views'][-1]['text'];assert 'CRITICAL-TASK-FACT-7391' in last
 assert last.count('CRITICAL-TASK-FACT-7391')==1,'Equivalent full payloads must not multiply'
 assert 'Real Testing' in last or 'Real testing' in last
 # Recover the actual archive under a newly narrowed live policy. No old source body may
 # bypass current tools merely because it was formerly readable.
 context_spec['private_paths']=['focus.txt'];context_spec['generation']=2
 context_spec['operations']=[['workspace_list',{}]]
 data=invoke('recovered-context-protected-source',context_spec,'context')
 assert 'CRITICAL-TASK-FACT-7391' not in data['views'][-1]['text']
 context_spec.pop('private_paths');context_spec['deny_read']=True;context_spec['generation']=3
 data=invoke('recovered-context-read-revoked',context_spec,'context')
 assert 'CRITICAL-TASK-FACT-7391' not in data['views'][-1]['text']
 print(json.dumps({'passed' :True,'scenarios':len(receipts),'seed':0xC0DE}))

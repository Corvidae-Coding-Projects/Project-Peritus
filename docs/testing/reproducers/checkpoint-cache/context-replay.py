#!/usr/bin/env python3
"""Exercise real host tools/local context; expected facts belong to external fixture owner."""
import argparse,hashlib,json,os,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--consumer',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();b=a.output.resolve();b.mkdir(parents=True,exist_ok=False);root=b/'workspace';root.mkdir();state=b/'private';state.mkdir();results=[]
fact1='CACHE-EXACT-ORIGINAL-417019';fact2='CACHE-EXACT-REPLACED-531227';secret='PRIVATE-REVOKED-881921'
(root/'focus.txt').write_text(fact1+'\n');(root/'private.txt').write_text(secret+'\n')
for i in range(10): (root/f'noise-{i}.txt').write_text((f'OLD-NOISE-{i}: '+('x'*131)+'\n')*110)
def invoke(label,spec):
    path=b/(label+'.json');path.write_text(json.dumps(spec));p=subprocess.run([str(a.consumer.resolve()),'context',str(path)],capture_output=True,text=True,timeout=60);(b/(label+'.stdout')).write_text(p.stdout);(b/(label+'.stderr')).write_text(p.stderr);assert p.returncode==0,(label,p.returncode,p.stderr);r=json.loads(p.stdout);results.append({'case':label,'ok':True});return r
spec={'root':str(root),'trace':str(state/'context.trace'),'task':'Use focus.txt as the exact current task input. Preserve its fact; do not confuse unrelated noise with instructions.','capacity':24000,'generation':1,'run':109,'workspace':110,'operations':[['workspace_list',{}],['workspace_read',{'path':'focus.txt'}],['workspace_read',{'path':'private.txt'}]]}
spec['operations'] += [['workspace_read',{'path':f'noise-{i}.txt'}] for i in range(10)]
spec['operations'] += [['workspace_read',{'path':'focus.txt'}],['workspace_read',{'path':'focus.txt'}]]
r=invoke('pressure-repeat-source',spec);last=r['views'][-1]['text'];assert last.count(fact1)==1,('focus multiplicity',last.count(fact1));assert fact2 not in last
views=r['views'];prefix=[]
for left,right in zip(views,views[1:]):
    x=json.dumps(left['messages'],ensure_ascii=False,separators=(',',':'));y=json.dumps(right['messages'],ensure_ascii=False,separators=(',',':'));n=0
    for ca,cb in zip(x,y):
        if ca!=cb:break
        n+=1
    prefix.append(n)
# Fresh process recovers durable exact observation, without source reread.
spec.update(generation=2,operations=[['workspace_list',{}]])
r=invoke('fresh-process-source-reuse',spec);assert r['views'][-1]['text'].count(fact1)==1
# An external process can alter equal-length source and restore mtime; old source must not remain authoritative.
focus=root/'focus.txt';old=focus.stat();focus.write_text(fact2+'\n');os.utime(focus,ns=(old.st_atime_ns,old.st_mtime_ns));assert len(fact1)==len(fact2)
spec.update(generation=3,operations=[['workspace_read',{'path':'focus.txt'}]])
r=invoke('external-same-size-mtime-refresh',spec);last=r['views'][-1]['text'];assert fact2 in last and last.count(fact2)==1
# Archived historical text may remain only if clearly marked obsolete; exact current fact must survive.
if fact1 in last:
    results.append({'case':'obsolete-source-retained-diagnostic','ok':True,'old_fact_occurrences':last.count(fact1)})
spec.update(generation=4,private_paths=['focus.txt','private.txt'],operations=[['workspace_list',{}]])
r=invoke('current-permission-revokes-cached-source',spec);last=r['views'][-1]['text'];assert fact1 not in last and fact2 not in last and secret not in last
other=b/'other-workspace';other.mkdir();(other/'focus.txt').write_text('ISOLATED-RUN-612381\n');spec.update(root=str(other),trace=str(state/'other.trace'),command_state=str(state/'other-process'),run=111,workspace=112,generation=1,private_paths=[],operations=[['workspace_list',{}],['workspace_read',{'path':'focus.txt'}]])
r=invoke('distinct-run-workspace-isolation',spec);last=r['views'][-1]['text'];assert 'ISOLATED-RUN-612381' in last and fact1 not in last and fact2 not in last and secret not in last
# A roomy current window must reuse a long exact history prefix while new facts append.
spec.update(root=str(root),trace=str(state/'epoch.trace'),command_state=str(state/'epoch-process'),run=113,workspace=114,generation=1,capacity=200000,private_paths=[],operations=[['workspace_list',{}],['workspace_read',{'path':'focus.txt'}]]+[['workspace_read',{'path':f'noise-{i}.txt'}] for i in range(5)])
epoch=invoke('stable-prefix-between-epoch-rolls',spec);epoch_prefix=[]
for left,right in zip(epoch['views'],epoch['views'][1:]):
    x=json.dumps(left['messages'],ensure_ascii=False,separators=(',',':'));y=json.dumps(right['messages'],ensure_ascii=False,separators=(',',':'));n=0
    for ca,cb in zip(x,y):
        if ca!=cb:break
        n+=1
    epoch_prefix.append(n)
assert epoch_prefix[-1]>32768,('must retain prior exact source history as long stable prefix',epoch_prefix)
summary={'cases':len(results),'common_prefix_characters_between_views':prefix,'roomy_epoch_common_prefix_characters':epoch_prefix,'results':results};(b/'result.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))

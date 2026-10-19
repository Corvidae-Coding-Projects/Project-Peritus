#!/usr/bin/env python3
"""Archive only this real provider probe's own anchor; ordinary requests must recover."""
import argparse,os,json,shutil,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--consumer',type=Path,required=True);p.add_argument('--corpus',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();base=a.output.resolve();base.mkdir(mode=0o700,parents=True,exist_ok=False);wire=base/'wire';wire.mkdir();env=dict(os.environ,CODEX_ROLLOUT_TRACE_ROOT=str(wire));err=(base/'archive-stderr.log').open('w');p=subprocess.Popen([str(a.consumer.resolve()),str(a.corpus.resolve()),'archive'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True,env=env)
results=[]
try:
 first=json.loads(p.stdout.readline());results.append(first);assert first['marker_correct']
 anchors=set()
 for file in wire.rglob('*.json'):
  try:entry=json.loads(file.read_text())
  except (ValueError,OSError):continue
  if 'input'in entry and entry.get('prompt_cache_key'):anchors.add(entry['prompt_cache_key'])
 assert len(anchors)==1,anchors
 anchor=anchors.pop();rpc=subprocess.Popen([shutil.which('codex') or 'codex','app-server'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True)
 try:
  for entry in [{'id':1,'method':'initialize','params':{'clientInfo':{'name':'peritus-cache-replay','version':'1'},'capabilities':{'experimentalApi':True}}},{'method':'initialized','params':{}},{'id':2,'method':'thread/archive','params':{'threadId':anchor}}]:rpc.stdin.write(json.dumps(entry)+'\n');rpc.stdin.flush()
  for line in rpc.stdout:
   entry=json.loads(line)
   if entry.get('id')==2:assert 'result'in entry,entry;break
 finally:rpc.stdin.close();rpc.wait(timeout=10)
 p.stdin.write('continue\n');p.stdin.flush()
 for line in p.stdout:results.append(json.loads(line))
 p.wait(timeout=90);assert p.returncode==0
finally:
 if p.poll()is None:p.kill();p.wait()
 err.close()
assert len(results)==3 and all(x['marker_correct']for x in results),results
assert (base/'archive-stderr.log').read_text().count('cache routing unavailable')==1
(base/'archive-result.json').write_text(json.dumps({'pass':True,'first_anchor_archived':anchor,'responses':results,'fallback_only_before_model_submission':True,'subsequent_request_uses_working_isolated_route':True},indent=2))
print(json.dumps({'pass':True,'real_requests':len(results),'fallbacks':1}))

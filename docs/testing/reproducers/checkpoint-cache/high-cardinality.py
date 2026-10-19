#!/usr/bin/env python3
"""Read legacy NUL-bearing database-page baseline stamps using actual public API."""
import argparse,json,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--baseline-consumer',type=Path,required=True);p.add_argument('--consumer',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();b=a.output.resolve();root=b/'workspace';runtime=root/'.prescient/runtime';private=b/'private';journal=private/'scope.jsonl';data=bytes(4096)
if not b.exists():
 b.mkdir(parents=True);runtime.mkdir(parents=True);private.mkdir()
 for i in range(16384):(runtime/f'page-{i:05}.bin').write_bytes(data)
def operate(binary,enroll):
 proc=subprocess.Popen([str(binary.resolve()),str(root),str(journal)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True);assert json.loads(proc.stdout.readline())['ready'];results=[]
 try:
  for command in ([{'op':'enroll','path':'.prescient'}] if enroll else [])+[{'op':'observe'},{'op':'observe','repeat':8}]:
   proc.stdin.write(json.dumps(command)+'\n');proc.stdin.flush();result=json.loads(proc.stdout.readline());assert result['ok'],result
   if command['op']=='observe':assert result['value']['paths']==[],result
   results.append({'command':command,**result})
 finally:
  proc.stdin.close();proc.wait(timeout=10);assert proc.returncode==0,proc.stderr.read()
 return results
if (b/'before.json').exists():before=json.loads((b/'before.json').read_text())
else:
 before=operate(a.baseline_consumer,True);(b/'before.json').write_text(json.dumps(before,indent=2))
after=operate(a.consumer,False);(b/'after.json').write_text(json.dumps(after,indent=2));summary={'files':16384,'payload_bytes':67108864,'legacy_index_bytes':Path(str(journal)+'.sqlite3').stat().st_size,'old_repeated_read_bytes':before[-1]['logical_read_bytes'],'new_repeated_read_bytes':after[-1]['logical_read_bytes'],'old_repeated_ms':before[-1]['elapsed_ms'],'new_repeated_ms':after[-1]['elapsed_ms']};(b/'result.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))

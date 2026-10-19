#!/usr/bin/env python3
"""Real old-library enrollment followed by cold new-library comparison of unchanged files."""
import argparse,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--baseline-consumer',type=Path,required=True);p.add_argument('--consumer',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();b=a.output.resolve();root=b/'workspace';journal=b/'private/scope.jsonl'
def operate(binary,operations):
    proc=subprocess.Popen([str(binary.resolve()),str(root),str(journal)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    assert json.loads(proc.stdout.readline())['ready'];results=[]
    for op in operations:
        proc.stdin.write(json.dumps(op)+'\n');proc.stdin.flush();value=json.loads(proc.stdout.readline());assert value['ok'],value;results.append(value)
    proc.stdin.close();proc.wait(timeout=10);assert proc.returncode==0,proc.stderr.read();return results
if not b.exists():
    (root/'data').mkdir(parents=True);journal.parent.mkdir();(root/'data/nul.bin').write_bytes(bytes(65536));(root/'data/text.txt').write_text('unchanged-source-12983\n');(root/'data/split-utf8.txt').write_text('a'*16383+'λ'*12)
    first=operate(a.baseline_consumer,[{'op':'enroll','path':'data'},{'op':'observe'}]);assert first[-1]['value']['paths']==[]; (b/'before.json').write_text(json.dumps(first,indent=2))
if a.consumer:
    after=operate(a.consumer,[{'op':'observe'},{'op':'diff'}]);(b/'after.json').write_text(json.dumps(after,indent=2));assert after[0]['value']['paths']==[],after[0];assert '\n--- before/' not in after[1]['value']['diff'];print(json.dumps({'old_to_new_unchanged_source_preserved':True}))
else: print(json.dumps({'baseline_prepared':True,'fixture':str(root)}))

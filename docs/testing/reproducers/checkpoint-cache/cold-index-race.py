#!/usr/bin/env python3
"""Append owned legacy evidence during real cold index read; cache must not bless it."""
import argparse,json,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--consumer',type=Path,required=True);p.add_argument('--fixture',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();b=a.output.resolve();b.mkdir(parents=True,exist_ok=False);fixture=a.fixture.resolve();root=fixture/'workspace';journal=fixture/'private/scope.jsonl';original=journal.read_bytes();proc=subprocess.Popen([str(a.consumer.resolve()),str(root),str(journal)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True);assert json.loads(proc.stdout.readline())['ready'];injected=False

def counter():return int(next(line.split(':',1)[1] for line in Path(f'/proc/{proc.pid}/io').read_text().splitlines() if line.startswith('rchar:')))
def call():
 proc.stdin.write(json.dumps({'op':'observe'})+'\n');proc.stdin.flush();return json.loads(proc.stdout.readline())
try:
 initial=counter();proc.stdin.write(json.dumps({'op':'observe'})+'\n');proc.stdin.flush();end=time.monotonic()+10
 while time.monotonic()<end and proc.poll() is None:
  if counter()-initial>4*1024*1024:
   with journal.open('ab') as f:f.write(b'OWNED-CONCURRENT-LEGACY-CHANGE\n')
   injected=True;break
  time.sleep(.001)
 assert injected,'Did not observe a substantive cold index read'
 first=json.loads(proc.stdout.readline());second=call();result={'injected_after_cold_read_bytes':counter()-initial,'first':first,'second':second,'passed':not second['ok']};(b/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result,indent=2));assert result['passed'],'Warm cache hid invalid legacy evidence appended during its cold load'
finally:
 proc.stdin.close();proc.wait(timeout=10);journal.write_bytes(original);assert proc.returncode==0,proc.stderr.read()

#!/usr/bin/env python3
"""A second real consumer enrolls new paths while the first keeps a warm scope index."""
import argparse,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--consumer',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();b=a.output.resolve();b.mkdir(parents=True,exist_ok=False);root=b/'workspace';root.mkdir();private=b/'private';private.mkdir();journal=private/'scope.jsonl';(root/'first.txt').write_text('FIRST-BEFORE\n');(root/'later.txt').write_text('LATER-BEFORE\n');results=[]
class Consumer:
 def __init__(self):
  self.p=subprocess.Popen([str(a.consumer.resolve()),str(root),str(journal)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True);assert json.loads(self.p.stdout.readline())['ready']
 def call(self,op,**fields):
  self.p.stdin.write(json.dumps({'op':op,**fields})+'\n');self.p.stdin.flush();return json.loads(self.p.stdout.readline())
 def close(self):self.p.stdin.close();self.p.wait(timeout=10);assert self.p.returncode==0,self.p.stderr.read()
c=Consumer()
try:
 assert c.call('enroll',path='first.txt')['ok'];assert c.call('observe')['value']['paths']==[]
 other=Consumer()
 try:assert other.call('enroll',path='later.txt')['ok']
 finally:other.close()
 (root/'later.txt').write_text('LATER-AFTER!\n');r=c.call('observe');assert r['ok'] and r['value']['paths']==['later.txt'],r;results.append({'case':'other-process-enrollment-invalidates-warm-index','passed':True})
 other=Consumer()
 try:assert other.call('enroll',path='new.txt')['ok']
 finally:other.close()
 (root/'new.txt').write_text('NEW-AFTER\n');r=c.call('observe');assert r['ok'] and set(r['value']['paths'])=={'later.txt','new.txt'},r;results.append({'case':'external-absent-path-enrollment-and-creation','passed':True})
 with journal.open('ab') as f:f.write(b'changed legacy journal\n')
 r=c.call('observe');assert not r['ok'],r;results.append({'case':'changed-private-legacy-journal-is-not-hidden-by-cache','passed':True,'error':r['error']})
 (b/'result.json').write_text(json.dumps(results,indent=2));print(json.dumps(results,indent=2))
finally:c.close()

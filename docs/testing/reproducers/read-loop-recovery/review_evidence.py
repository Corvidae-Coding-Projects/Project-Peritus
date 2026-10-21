#!/usr/bin/env python3
"""Use the real public historical reader and final-context callback to check durable review coverage."""
import argparse,hashlib,json
from pathlib import Path
from independent_archive_policy import Session,require
p=argparse.ArgumentParser();p.add_argument('--consumer',required=True,type=Path);p.add_argument('--output',required=True,type=Path);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
s=Session(a.consumer,a.output,'review-evidence');cases=[]
def begin():s.ask({'action':'begin','text':s.spec['task']})
def flat():return json.dumps(s.view())
def evidence(path,offset=0):return s.ask({'action':'evidence','section':'transcript','path':str(path),'offset':offset,'max_bytes':1024})
try:
 s.ask({'action':'role','role':'reviewer'})
 path=s.path/'historical.txt';body=('REVIEW-SOURCE-ORACLE Ω中\n'*257).encode();path.write_bytes(body);digest=hashlib.sha256(body).hexdigest()
 evidence(path);s.view();begin();require('Historical review evidence already delivered' not in flat(),'raw unsent tool output got delivery credit');cases.append('undelivered raw archive is not delivery proof')
 offset=0;reconstructed=b'';pages=0
 while True:
  page=evidence(path,offset);require(page['sha256']==digest and page['total_bytes']==len(body),'reader source identity differs from fixture')
  data=page['bytes'].encode();require(data==body[offset:offset+len(data)],'reader page differs from independent fixture bytes');reconstructed+=data;pages+=1
  s.view();s.ask({'action':'present_view'})
  if page['next_offset'] is None:break
  require(page['next_offset']>offset,'reader did not progress');offset=page['next_offset']
 require(reconstructed==body,'reader did not reconstruct exact UTF-8 fixture')
 begin();view=flat();require(digest in view and 'complete=true' in view,'same-review rollover lost delivered coverage');cases.append('complete immutable source coverage survives invocation rollover')
 s.stop();s.start();s.ask({'action':'role','role':'reviewer'});view=flat();require(digest in view and 'complete=true' in view,'cold review reopen lost coverage');cases.append('review delivery survives cold reopen')
 s.ask({'action':'role','role':'writer'});require(digest not in flat(),'writer inherited reviewer evidence');s.ask({'action':'role','role':'reviewer'});cases.append('role-isolated delivery ledger')
 other=s.path/'undelivered-other.txt';other.write_text('OTHER-UNDELIVERED-ORACLE\n'*100);other_digest=hashlib.sha256(other.read_bytes()).hexdigest();evidence(other);s.view();begin();view=flat();require(other_digest not in view,'new raw source inherited old source delivery credit');cases.append('new immutable identity does not inherit old coverage')
 page=evidence(other);s.view();s.ask({'action':'present_view'});begin();view=flat();require(other_digest in view and digest in view,'independent immutable identities were not retained');cases.append('different digest ranges remain separate')
 result={'status':'PASS','consumer_sha256':hashlib.sha256(a.consumer.read_bytes()).hexdigest(),'source_sha256':digest,'source_bytes':len(body),'reader_pages':pages,'actions':s.actions,'processes':s.processes,'cases':cases};(a.output/'results.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)
finally:s.stop()

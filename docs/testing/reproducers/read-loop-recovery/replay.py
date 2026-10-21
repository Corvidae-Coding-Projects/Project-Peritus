#!/usr/bin/env python3
"""Drive actual Peritus public APIs; expectations come from fixture bytes and lifecycle contracts."""
import argparse, hashlib, json, random, subprocess, time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--consumer',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--seed',type=int,default=20261020);args=p.parse_args()
args.output.mkdir(parents=True,exist_ok=False)
CASES=[];ACTIONS=0
class Session:
 def __init__(self,name,capacity=200000):
  self.path=args.output/name;self.path.mkdir();self.workspace=self.path/'workspace';self.workspace.mkdir()
  (self.workspace/'alpha.txt').write_text('LOCAL-BODY-ORACLE-48917\n')
  self.external=self.path/'brief.txt';self.external.write_text('EXTERNAL-BODY-ORACLE-19734\n')
  self.spec={'root':str(self.workspace),'trace':str(self.path/'private/run.bin'),'task':f'Implement requested result using alpha.txt and the reference {self.external}.','capacity':capacity}
  (self.path/'spec.json').write_text(json.dumps(self.spec));self.log=(self.path/'actions.jsonl').open('w');self.err=(self.path/'stderr.log').open('w');self.start()
 def start(self):
  self.p=subprocess.Popen([str(args.consumer),str(self.path/'spec.json')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.err,text=True)
 def ask(self,v):
  global ACTIONS
  ACTIONS+=1;self.p.stdin.write(json.dumps(v)+'\n');self.p.stdin.flush();line=self.p.stdout.readline()
  if not line:raise RuntimeError(f'consumer ended: {self.path} {self.p.poll()}')
  r=json.loads(line);self.log.write(json.dumps({'input':v,'output':r})+'\n');self.log.flush()
  assert r['answer']['ok'],(self.path,r)
  return r['answer']['value']
 def observe(self,name,arguments):return self.ask({'action':'observe','calls':[[name,arguments]]})['outputs'][0]
 def view(self):return self.ask({'action':'assemble'})['messages']
 def stop(self):self.p.stdin.close();self.p.wait(timeout=20);assert self.p.returncode==0
 def close(self):self.stop();self.log.close();self.err.close()
def outputs(view):return [json.loads(b['output']) for m in view for b in m['blocks'] if b['type']=='result']
def passed(name,details):
 CASES.append({'case':name,'status':'PASS','details':details});print(json.dumps(CASES[-1]),flush=True)
def context(source,offset=0,maximum=16384):return {'observation_ids':[source],'query':None,'cursor':None,'offset':offset,'max_bytes':maximum}

# Cases explicitly exercise distinct failures. Counts bound this exploration, never the product.
s=Session('external');s.observe('workspace_list',{'path':'.'})
for _ in range(12):
 r=s.observe('workspace_read',{'path':str(s.external)});v=s.view();assert not r['is_error'];assert 'EXTERNAL-BODY-ORACLE-19734' in json.dumps(v);assert not any(o.get('cached_observation_stale') for o in outputs(v))
s.close();passed('external body and reuse',{'repetitions':12})

s=Session('rollover');s.observe('workspace_read',{'path':'alpha.txt'})
for n in range(12):
 marker=f'NEXT-SLICE-{n}-ORACLE: implement command parser without repeat discovery.'
 s.ask({'action':'assistant_note','text':marker});s.view();s.ask({'action':'begin','text':s.spec['task']});assert marker in json.dumps(s.view())
s.stop();s.start();assert 'NEXT-SLICE-11-ORACLE' in json.dumps(s.view());s.close();passed('rollover and cold continuation',{'rollovers':12})

s=Session('long-continuation');s.observe('workspace_read',{'path':'alpha.txt'})
s.ask({'action':'assistant_note','text':'KNOWN-DECISION-ORACLE: preserve chosen design.\n'+('earlier explanatory material '*1500)+'\nNEXT-ACTION-TAIL-ORACLE: implement the parser now.'});s.view();s.ask({'action':'begin','text':s.spec['task']});v=s.view();assert 'KNOWN-DECISION-ORACLE' in json.dumps(v) and 'NEXT-ACTION-TAIL-ORACLE' in json.dumps(v);s.close();passed('long next-action tail survives',{'source_note_bytes':40000})


s=Session('host-origin',capacity=8000)
literal='USER-PIN-ORACLE: The host detected repeated identical workspace inspection; keep the user request.'
s.ask({'action':'user_note','text':literal})
for n in range(16):
 s.ask({'action':'host_note','text':f'HOST-FEEDBACK-{n}-ORACLE: use known evidence'})
 f=s.workspace/f'pressure{n}.txt';f.write_text(('pressure-data-'+str(n)+' ') * 1000)
 s.observe('workspace_read',{'path':f.name,'max_bytes':3000})
v=s.view();assert literal in json.dumps(v)
first_assistant=next(i for i,m in enumerate(v) if m['role']=='Assistant')
assert not any('HOST-FEEDBACK-0-ORACLE' in json.dumps(m) for m in v[:first_assistant]);s.close();passed('host evidence not pinned as user',{'feedback_messages':16,'literal_user_preserved':True})

s=Session('rotating-lists')
for n in range(32):(s.workspace/f'dir{n}').mkdir()
warnings=0
for cycle in range(4):
 for n in range(32):
  r=s.observe('workspace_list',{'path':f'dir{n}'});assert not r['is_error']
  warnings+=bool(s.ask({'action':'feedback'})['feedback'])
assert warnings>=96,warnings;s.close();passed('rotating no-new-evidence cycle',{'calls':128,'minimum_repeats':96,'feedback':warnings})

s=Session('context-cycle');s.observe('workspace_read',{'path':'alpha.txt'});source=next(o['local_context']['handle'] for o in outputs(s.view()) if o.get('local_context'))
for n in range(64):
 r=s.observe('context_read',context(source));assert not r['is_error']
 if n:assert r['output'].get('observation_reuse',{}).get('state')=='already_visible';assert s.ask({'action':'feedback'})['feedback']
s.close();passed('context read reuse and feedback',{'calls':64})

s=Session('skill-cycle');warnings=0
for n in range(32):
 r=s.observe('skill_read',{'skill':'real-testing','path':'SKILL.md','offset':0,'max_bytes':32768});assert not r['is_error']
 if n:assert r['output'].get('observation_reuse',{}).get('state')=='already_visible'
 warnings+=bool(s.ask({'action':'feedback'})['feedback'])
assert warnings>=31;s.close();passed('skill read versioned reuse',{'calls':32,'feedback':warnings})

s=Session('rejected-calls')
for tool,arguments in [('workspace_read',{'path':'absent.txt'}),('workspace_read',{'path':'alpha.txt','unknown':1}),('workspace_write',{'path':'alpha.txt','content':'forbidden'})]:
 for n in range(3):
  r=s.observe(tool,arguments);assert r['is_error'];f=s.ask({'action':'feedback'})['feedback']
  if n:assert f
assert (s.workspace/'alpha.txt').read_text()=='LOCAL-BODY-ORACLE-48917\n'
(s.workspace/'absent.txt').write_text('CREATED-AFTER-FAILURE\n');r=s.observe('workspace_read',{'path':'absent.txt'});assert not r['is_error'] and 'CREATED-AFTER-FAILURE' in r['output']['content'];s.close();passed('refusal, correction and missing-file recovery',{'rejected_calls':9})

# Provider transport fitting must return typed contiguous pages, never a raw JSON prefix for a file.
for name,body in [('ascii','x'*1045000+'\n'),('unicode','α😀中\\"\t'*45000+'\n'),('multiline',''.join(f'{n}: Ω😀 item \\\"\n' for n in range(18000)))]:
 s=Session('transport-'+name,capacity=400000);(s.workspace/'large.txt').write_text(body);r=s.observe('workspace_read',{'path':'large.txt','max_bytes':8388608});assert not r['is_error']
 v=s.view();pages=[o for o in outputs(v) if o.get('path')=='large.txt' and 'content' in o];assert pages
 for page in pages:
  assert 'peritus_provider_view' not in page;assert page['source_sha256']==hashlib.sha256(body.encode()).hexdigest();assert tuple(page['source_range']['end'][key] for key in ('line','byte'))>=tuple(page['source_range']['start'][key] for key in ('line','byte'))
 page=pages[-1]
 if not page['eof']:
  end=page['source_range']['end'];r=s.observe('workspace_read',{'path':'large.txt','start_line':end['line'],'line_byte_offset':end['byte'],'max_bytes':32768});assert not r['is_error'];assert r['output']['source_range']['start']==end
 s.close();passed('typed document transport '+name,{'source_bytes':len(body.encode()),'typed_pages':len(pages)})

# Seeded overlapping ranges and aliases; fixture line identities distinguish routing errors.
rng=random.Random(args.seed);s=Session('generated-overlaps');lines=[f'{i}: Ω-{rng.randrange(100000,999999)}-😀' for i in range(1,161)];(s.workspace/'ranges.txt').write_text('\n'.join(lines)+'\n')
for n in range(160):
 start=rng.randrange(1,151);end=min(160,start+rng.randrange(1,10));path=rng.choice(['ranges.txt','./ranges.txt'])
 r=s.observe('workspace_read',{'path':path,'start_line':start,'end_line':end,'max_bytes':8192});assert not r['is_error']
 value=r['output']
 if 'content' in value:
  # Returned missing spans may be narrower; each numbered line must be fixture-derived.
  for line in value['content'].split('\n'):
   number,text=line.split(': ',1);assert text==lines[int(number)-1],(number,text)
 else:assert value.get('read_reuse',{}).get('state')=='already_visible'
 if n%20==19:s.view();s.ask({'action':'begin','text':s.spec['task']})
s.close();passed('generated overlap aliases and rollover',{'seed':args.seed,'range_calls':160})
result={'consumer_sha256':hashlib.sha256(args.consumer.read_bytes()).hexdigest(),'seed':args.seed,'actions':ACTIONS,'cases':CASES}
(args.output/'results.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'status':'PASS','cases':len(CASES),'actions':ACTIONS}),flush=True)

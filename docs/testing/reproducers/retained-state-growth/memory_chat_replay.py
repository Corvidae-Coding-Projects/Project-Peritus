#!/usr/bin/env python3
"""Actual TUI/daemon/Codex memory journey; independent raw-tool/state oracle after shutdown."""
import argparse,codecs,fcntl,hashlib,json,os,pty,select,signal,sqlite3,struct,subprocess,termios,time
from pathlib import Path
import pyte
p=argparse.ArgumentParser();p.add_argument('--fixture',required=True,type=Path);p.add_argument('--seconds',type=int,default=300);p.add_argument('--run-provider',action='store_true',required=True);a=p.parse_args();root=a.fixture.resolve();manifest=json.loads((root/'owned-fixture.json').read_text());assert manifest['owned_disposable_fixture'];project=root/'project';binary=root/'bin/peritus';expected=json.loads((root/'expected.json').read_text())
env=os.environ.copy();env.update({f'XDG_{key.upper()}_HOME':str(root/key)for key in('config','state','cache')});env['TERM']='xterm-256color'

def endpoint():
 instance=root/'state/peritus/daemon/daemon.instance'
 if not instance.exists():return None
 try:
  data=dict(line.split('=',1)for line in instance.read_text().splitlines()if'='in line);process=Path('/proc')/data['pid'];cmd=(process/'cmdline').read_bytes().split(b'\0')
  if cmd[0]!=str(root/'bin/peritusd').encode()or not any(str(root/'config').encode()in part for part in cmd):return None
  inodes=set()
  for fd in(process/'fd').iterdir():
   try:target=os.readlink(fd)
   except OSError:continue
   if target.startswith('socket:['):inodes.add(target[8:-1])
  for line in Path('/proc/net/unix').read_text().splitlines()[1:]:
   fields=line.split()
   if len(fields)>=8 and fields[6]in inodes and fields[7].endswith('/daemon.sock'):return fields[7]
 except(OSError,KeyError,ValueError):return None
 return None

def artifact(context,descriptor):
 digest=bytes(descriptor['digest']).hex();data=(context/'artifacts/objects/sha256'/digest[:2]/digest).read_bytes();assert len(data)==descriptor['bytes']and hashlib.sha256(data).hexdigest()==digest;return data

def raw_oracle(record):
 trace=root/'state/peritus/daemon/product-runs'/f"{record['run_id']}.trace";tools=[];provider_frames=0
 with trace.open('rb')as f:
  while header:=f.read(9):
   assert len(header)==9;size=int.from_bytes(header[1:],'little')
   if header[0]==7:
    value=json.loads(f.read(size));value['arguments_decoded']=json.loads(value['arguments']);value['output_decoded']=json.loads(value['output']);tools.append(value)
   else:
    if header[0]==1:provider_frames+=1
    f.seek(size,1)
 contexts=list(trace.with_suffix('.context').glob('*/journal.sqlite3'));assert contexts,'No actual memory journal'
 sources=[]
 for database in contexts:
  with sqlite3.connect(f'file:{database}?mode=ro',uri=True)as c:row=c.execute('SELECT value FROM state_records WHERE namespace=3401').fetchone()
  if not row:continue
  checkpoint=json.loads(row[0]);context=database.parent;data=json.loads(artifact(context,checkpoint['source_index']))
  if isinstance(data,list):sources.extend(data)
  else:
   assert data['kind']=='sources'and data['version']==1
   for page in data['pages']:sources.extend(json.loads(artifact(context,page)))
 by_call={source['call']['id']:source for source in sources if source.get('call')};handle_paths={};read_paths=set()
 for tool in tools:
  name=tool['name'];args=tool['arguments_decoded']
  if name=='workspace_read'and not tool['is_error']and str(args.get('path','')).startswith('sources/source-'):
   source=by_call.get(tool['call_id']);assert source,'Actual read lacks durable source index';scope=bytes(tool['scope']).hex();handle=f"obs:{scope}:{source['sequence']:06}";handle_paths[handle]=args['path'];read_paths.add(args['path'])
 assert read_paths=={f'sources/source-{n:02}.txt'for n in range(40)},f'Expected40 actual observed source files; found{len(read_paths)}'
 wanted={op['id']:op for op in expected['operations']};accepted=[]
 for tool in tools:
  args=tool['arguments_decoded'];out=tool['output_decoded']
  if tool['name']=='context_update'and not tool['is_error']and len(args.get('operations',[]))==33 and len(out.get('entries',[]))==33:
   actual={op['id']:op for op in args['operations']}
   if set(actual)!=set(wanted):continue
   for key,request in wanted.items():
    operation=actual[key]
    for field in request:
     if field!='supports':assert operation[field]==request[field],f'Atomic update altered{key}:{field}'
    assert len(set(operation['supports']))==len(operation['supports'])
    for handle in operation['supports']:assert handle in handle_paths,'Update cites a fabricated/non-original source handle'
   first=actual[expected['operations'][0]['id']];assert len(first['supports'])==40 and{handle_paths[h]for h in first['supports']}==read_paths
   accepted.append(tool)
 assert accepted,'No successful exact33-operation atomic update'
 complete={};text_fragments={};json_fragments={};issued=set();fragment_pages=0;state_pages=0
 for tool in tools:
  if tool['name']!='context_read'or tool['is_error']:continue
  args=tool['arguments_decoded'];out=tool['output_decoded']
  if args.get('observation_ids')!=[]or args.get('query')is not None:continue
  assert args['max_bytes']==2048
  if args.get('cursor')is not None:assert args['cursor']in issued,'Read did not use an issued state cursor'
  assert len(tool['output'].encode())<=2048,'Actual context_read result exceeded requested2048B'
  if out.get('cursor')is not None:issued.add(out['cursor'])
  state_pages+=1
  if item:=out.get('entry_json_fragment'):
   fragment_pages+=1;index=item['entry_index'];buffer=json_fragments.setdefault(index,bytearray());data=item['data'].encode();assert item['encoding']=='utf8'and item['offset']<=len(buffer)
   if item['offset']<len(buffer):assert buffer[item['offset']:item['offset']+len(data)]==data,'Repeated JSON fragment changed'
   else:buffer.extend(data)
   assert len(buffer)<=item['total_bytes']
   if len(buffer)==item['total_bytes']:
    value=json.loads(buffer);complete[value['id']]=value
  for value in out.get('entries',[]):
   if'text_offset'in value:
    buffer=text_fragments.setdefault(value['id'],bytearray());assert value['text_offset']==len(buffer);buffer.extend(value['text'].encode());assert len(buffer)<=value['text_total_bytes']
    if len(buffer)==value['text_total_bytes']:value=dict(value);value['text']=buffer.decode();complete[value['id']]=value
   else:complete[value['id']]=value
 assert set(wanted).issubset(complete),f'Exact state pages omitted{set(wanted)-set(complete)}'
 for key,request in wanted.items():assert complete[key]['text']==request['text']and complete[key]['authority']=='none'
 first=complete[expected['operations'][0]['id']];assert len(first['text'].encode())==expected['large_text_bytes']and len(first['supports'])==40
 assert fragment_pages>=2,'Large reference metadata did not cross JSON fragment pages'
 assert state_pages and tools and provider_frames
 return{'actual_read_sources':len(read_paths),'accepted_atomic33_updates':len(accepted),'exact_retrieved_entries':len(set(wanted)&set(complete)),'large_unicode_bytes':expected['large_text_bytes'],'large_entry_actual_supports':40,'state_read_pages':state_pages,'json_fragment_pages':fragment_pages,'provider_event_frames':provider_frames,'actual_tools':len(tools),'exact_unicode_and_entry_data':True}

master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',42,160,0,0));proc=subprocess.Popen([str(binary),'open',str(project)],cwd=project,env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave);screen=pyte.Screen(160,42);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf8')('replace');raw=bytearray();started=time.monotonic();finished=False;record=None;result={'status':'BLOCKED','failure':None};shutdown=None

def pump(seconds=.2):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  if select.select([master],[],[],min(.05,max(.001,end-time.monotonic())))[0]:
   try:data=os.read(master,65536)
   except OSError:return
   raw.extend(data);stream.feed(decoder.decode(data))
   if b'\x1b[?u'in data:os.write(master,b'\x1b[?1;2c')
   if b'\x1b[6n'in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
def send(text):os.write(master,text.encode());pump(.2)
try:
 trusted=False
 while time.monotonic()-started<40 and proc.poll()is None:
  pump();text='\n'.join(screen.display)
  if not trusted and'Trust this workspace?'in text:send('y\r');trusted=True
  if'Peritus ·'in text and'Ready'in text:break
 else:raise RuntimeError('Real TUI did not become ready')
 (root/'opened.screen.txt').write_text('\n'.join(screen.display));send('/new\r');send('/chat\r')
 task='Read memory_request.md and execute its explicitly authorized memory operations exactly. Work only in this disposable project; do not run commands or change files. Use the actual host tools, then report the requested completion marker.'
 send('\x1b[200~'+task+'\x1b[201~');send('\r');run_started=time.monotonic()
 while time.monotonic()-run_started<a.seconds and proc.poll()is None:
  pump(.5);(root/'current.screen.txt').write_text('\n'.join(screen.display))
  paths=list((root/'state/peritus/daemon/workbench-v1/runs').glob('*.json'))
  for path in paths:
   try:candidate=json.loads(path.read_bytes())
   except(OSError,ValueError):continue
   record=candidate
   terminal=candidate.get('status','').startswith(('Idle','Completed','Stopped','Recovery required'))
   if terminal and candidate.get('progress',{}).get('model_requests',0)>0:finished=True;break
  if finished:break
 if not finished:result['failure']='Bounded5minute conversation did not complete; inspect real progress/trace'
finally:
 (root/'terminal.raw').write_bytes(raw);(root/'terminal.screen.txt').write_text('\n'.join(screen.display))
 if proc.poll()is None:
  if not finished:send('\x03');pump(1)
  send('\x11')
  try:proc.wait(timeout=10)
  except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGTERM);proc.wait(timeout=10)
 os.close(master);socket=endpoint()
 if socket:
  shutdown=subprocess.run([str(binary),'--endpoint',socket,'shutdown','--wait'],capture_output=True,text=True,env=env,timeout=30);(root/'shutdown.txt').write_text(shutdown.stdout+shutdown.stderr)
 result['conversation_finished']=finished;result['seconds']=round(time.monotonic()-started,3);result['shutdown_exit']=shutdown.returncode if shutdown else None
 after={str(f.relative_to(project)):hashlib.sha256(f.read_bytes()).hexdigest()for f in project.rglob('*')if f.is_file()};assert after==manifest['project_files_sha256'],'Project files changed';result['project_unchanged']=True
 if record:
  (root/'final-record.json').write_text(json.dumps(record,indent=2));result['run_id']=record['run_id'];result['run_status']=record['status']
  try:result['oracle']=raw_oracle(record);result['operation_contract_passed']=True;result['status']='PASS'if finished else'BLOCKED'
  except Exception as error:result['failure']=repr(error);result['status']='FAIL'if finished else'BLOCKED'
 (root/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
if result['status']!='PASS':raise SystemExit(1)

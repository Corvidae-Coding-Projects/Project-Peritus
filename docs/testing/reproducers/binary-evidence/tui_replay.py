#!/usr/bin/env python3
"""Real Peritus/Codex stable binary-data replay through writer and independent review."""
import argparse,codecs,fcntl,hashlib,json,os,pty,re,select,signal,struct,subprocess,termios,time
from pathlib import Path
import pyte
p=argparse.ArgumentParser();p.add_argument('--root',type=Path,required=True);p.add_argument('--binary',type=Path,required=True);p.add_argument('--seconds',type=int,default=360);p.add_argument('--model',default='gpt-6.1-sol');a=p.parse_args()
root=a.root.resolve();project=root/'project';binary=a.binary.resolve();original_guide=hashlib.sha256((project/'AGENTS.md').read_bytes()).hexdigest();original_manifest=hashlib.sha256((project/'pyproject.toml').read_bytes()).hexdigest()
if json.loads((root/'owned-fixture.json').read_text()).get('kind')!='stable-nul-page-source-pipeline':raise RuntimeError('Expected a fresh owned stable binary-page fixture')
binary_page=project/'.prescient/page.bin'
expected_binary=bytes([0,65,0,66])*2048
phase_observations=[]
env=os.environ.copy();env.update({f'XDG_{s.upper()}_HOME':str(root/s) for s in ('config','state','cache')});env['TERM']='xterm-256color';env['PATH']=str(Path.home()/'.local/bin')+':'+env['PATH']
master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',44,160,0,0))
process=subprocess.Popen([str(binary),'open',str(project)],cwd=project,env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
screen=pyte.Screen(160,44);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf8')('replace');raw=bytearray();result={}
def display():return '\n'.join(''.join(screen.buffer[y][x].data for x in range(screen.columns)) for y in range(screen.lines))
def pump(seconds=.1):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  if select.select([master],[],[],min(.05,max(.001,end-time.monotonic())))[0]:
   try:data=os.read(master,65536)
   except OSError:return
   raw.extend(data);stream.feed(decoder.decode(data))
   if b'\x1b[?u' in data:os.write(master,b'\x1b[?1;2c')
   if b'\x1b[6n' in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
def save(label):
 (root/f'{label}.screen.txt').write_text(display());(root/'terminal.raw').write_bytes(raw)
def records():
 paths=list((root/'state/peritus/daemon/workbench-v1/runs').glob('*.json'))
 return [json.loads(path.read_text()) for path in paths]
def send(text):os.write(master,text.encode());pump(.4)
def verified_tool_output():
 for path in (root/'state/peritus/daemon/product-runs').glob('*.trace'):
  payload=path.read_bytes();offset=0
  while offset+9<=len(payload):
   kind=payload[offset];size=int.from_bytes(payload[offset+1:offset+9],'little');body=payload[offset+9:offset+9+size];offset+=9+size
   if len(body)!=size:break
   if kind!=7:continue
   try:
    event=json.loads(body)
    if event.get('name')!='run_command' or event.get('is_error'):continue
    args=json.loads(event.get('arguments','{}'));out=json.loads(event.get('output','{}'))
    if args.get('program')=='python3' and args.get('args')==['main.py'] and out.get('stdout')=='SOURCE-REPAIRED\n' and out.get('exit_code')==0:return True
   except (ValueError,TypeError):pass
 return False
try:
 answered=set();end=time.monotonic()+90
 while time.monotonic()<end and process.poll() is None:
  pump();text=display()
  for prompt,reply in [('Trust this workspace?','y'),('Providers (comma-separated','1'),('Choose a model number','manual '+a.model)]:
   if prompt in text and prompt not in answered:answered.add(prompt);send(reply+'\r')
  if 'Peritus ·' in text and 'Ready ·' in text:break
 else:raise RuntimeError('not ready: '+display())
 save('opened')
 task='Use the normal run_pipeline to implement and fully review this precise two-file task in place. Declare write_paths ["main.py", ".prescient"]. Run the actual Python command: python3 -c "from pathlib import Path; Path(\'main.py\').write_text(\'print(\'+repr(\'SOURCE-REPAIRED\')+\')\'+chr(10)); Path(\'.prescient/page.bin\').write_bytes(bytes([0,65,0,66])*2048)". The binary page must remain exactly 8192 bytes matching bytes([0,65,0,66])*2048, including its 4096 intentional NULs. Use actual host tools to read required existing targets, make both requested changes, then run python3 main.py and an actual binary-file readback to confirm exact bytes. The CLI stdout must be SOURCE-REPAIRED and one newline, exit0, empty stderr. Finish the normal host pipeline past writer completion through independent review. This is the entire task: no additional feature work, unit tests, mocks, Clippy, lint, formal verification, Git initialization, new packages or test frameworks. No material choices need another question.'
 (root/'task.txt').write_text(task);os.write(master,b'\x1b[200~'+task.encode()+b'\x1b[201~');pump(.5);send('\r')
 begin=time.monotonic();terminal=None;last_observation=None
 while time.monotonic()-begin<a.seconds and process.poll() is None:
  pump(.5);save('current');values=records()
  if values:
   observation={key:values[-1].get(key) for key in ('run_id','status','phase','checkpoint','interruption_cause','remaining_work')}
   phase_observations.append({'elapsed':round(time.monotonic()-begin,3),'phase':values[-1].get('phase'),'requests':values[-1].get('progress',{}).get('model_requests',0)})
   if observation != last_observation:
    with (root/'candidate-observations.jsonl').open('a') as f:f.write(json.dumps({'elapsed':round(time.monotonic()-begin,3),**observation})+'\n')
    last_observation=observation
  if values and values[-1].get('status','').startswith(('Completed','Idle','Stopped','Recovery required','Cancelled')):
   terminal=values[-1];break
 if terminal is None:terminal=records()[-1] if records() else None
 result={'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'elapsed_seconds':round(time.monotonic()-begin,3),'source':(project/'main.py').read_text(),'record':terminal,'screen':display(),'oracle':'Stable binary data must stay tracked and exact, requested source output must work, and valid host text must reach a real post-writer provider/reviewer request without invalidtext fatal.'}
 check=subprocess.run(['python3',str(project/'main.py')],cwd=project,capture_output=True,text=True,timeout=5)
 result['independent_cli_readback']={'exit_code':check.returncode,'stdout':check.stdout,'stderr':check.stderr}
 result['source_oracle_satisfied']=check.returncode==0 and check.stdout=='SOURCE-REPAIRED\n'
 result['invalid_text_failure']='model text is empty, contains NUL, or exceeds its byte bound' in json.dumps(terminal)
 result['manifest_unchanged']=hashlib.sha256((project/'pyproject.toml').read_bytes()).hexdigest()==original_manifest
 result['guide_unchanged']=hashlib.sha256((project/'AGENTS.md').read_bytes()).hexdigest()==original_guide
 result['unexpected_files']=[str(path.relative_to(project)) for path in project.rglob('*') if path.is_file() and str(path.relative_to(project)) not in ('main.py','AGENTS.md','pyproject.toml','.prescient/page.bin')]
 result['actual_followup_command_observed']=verified_tool_output()
 result['binary_exact']=binary_page.read_bytes()==expected_binary
 result['binary_bytes']=binary_page.stat().st_size
 result['binary_nuls']=binary_page.read_bytes().count(0)
 entries=[]
 for journal in (root/'state/peritus/daemon/product-runs').glob('*.files-*.jsonl'):
  entries.extend(json.loads(line) for line in journal.read_text().splitlines())
 result['binary_page_tracked']=any(entry.get('path')=='.prescient/page.bin' or '.prescient/page.bin' in entry.get('descendants',{}) for entry in entries)
 result['workflow_finished']=bool(terminal and terminal.get('phase')==7)
 result['candidate_qualified']=bool(terminal and (terminal.get('checkpoint') or {}).get('stage')==6)
 writing=[v['requests'] for v in phase_observations if v['phase']==2]
 # Checking may finish between polls. The first real reviewer request must advance the
 # request counter beyond the preceding writer, regardless of sampling that fast phase.
 result['review_provider_request_observed']=bool(writing and any(v['phase']==4 and v['requests']>max(writing) for v in phase_observations))
 result['phase_observations']=phase_observations
 result['operation_oracle_satisfied']=all((result['source_oracle_satisfied'],result['actual_followup_command_observed'],result['binary_exact'],result['binary_page_tracked'],result['guide_unchanged'],result['manifest_unchanged'],not result['unexpected_files']))
 result['post_writer_boundary_oracle_satisfied']=all((result['operation_oracle_satisfied'],result['review_provider_request_observed'],not result['invalid_text_failure']))
 save('final');(root/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps({k:v for k,v in result.items() if k not in ('record','screen','phase_observations')},indent=2));print('STATUS',terminal.get('status') if terminal else None)
finally:
 if process.poll() is None:
  send('\x11')
  try:process.wait(timeout=8)
  except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=8)
 os.close(master)
 sockets=list((root/'state/peritus/daemon').glob('*.sock'))
 marker=root/'state/peritus/daemon/daemon.instance'
 if not sockets and marker.exists():
  identity=dict(line.split('=',1) for line in marker.read_text().splitlines() if '=' in line)
  listings=subprocess.check_output(['ss','-xlpn'],text=True)
  matching=[line for line in listings.splitlines() if f"pid={identity['pid']}," in line]
  sockets=[Path(re.search(r'(/\S+\.sock)',matching[0]).group(1))] if matching else []
 if sockets:
  stop=subprocess.run([str(binary),'--endpoint',str(sockets[0]),'shutdown','--wait'],env=env,capture_output=True,text=True,timeout=45)
  (root/'shutdown.txt').write_text(stop.stdout+stop.stderr)
 (root/'terminal.raw').write_bytes(raw)
raise SystemExit(0 if result.get('post_writer_boundary_oracle_satisfied') and result.get('workflow_finished') and result.get('candidate_qualified') else 1)

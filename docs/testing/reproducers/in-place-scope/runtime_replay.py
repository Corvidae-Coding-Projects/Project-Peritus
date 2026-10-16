#!/usr/bin/env python3
"""Real Peritus/Codex TUI repro; external file writer is disposable workload input."""
import argparse,codecs,fcntl,hashlib,json,os,pty,re,select,signal,struct,subprocess,termios,time
from pathlib import Path
import pyte
p=argparse.ArgumentParser();p.add_argument('--root',type=Path,required=True);p.add_argument('--binary',type=Path,required=True);p.add_argument('--seconds',type=int,default=360);p.add_argument('--writer-after-effect',action='store_true');p.add_argument('--stop-after-operations',action='store_true');a=p.parse_args()
root=a.root.resolve();project=root/'project';binary=a.binary.resolve();original_guide=hashlib.sha256((project/'AGENTS.md').read_bytes()).hexdigest();assert json.loads((root/'owned-fixture.json').read_text())['owned_disposable_fixture']
runtime=project/'.prescient/postgres/state'
with runtime.open('rb') as f:initial_counter=f.read(8)
env=os.environ.copy();env.update({f'XDG_{s.upper()}_HOME':str(root/s) for s in ('config','state','cache')});env['TERM']='xterm-256color';env['PATH']=str(Path.home()/'.local/bin')+':'+env['PATH']
master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',44,160,0,0))
process=subprocess.Popen([str(binary),'open',str(project)],cwd=project,env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
screen=pyte.Screen(160,44);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf8')('replace');raw=bytearray();writer=None;result={}
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
  for prompt,reply in [('Trust this workspace?','y'),('Providers (comma-separated','1'),('Choose a model number','manual gpt-6.1-sol')]:
   if prompt in text and prompt not in answered:answered.add(prompt);send(reply+'\r')
  if 'Peritus ·' in text and 'Ready ·' in text:break
 else:raise RuntimeError('not ready: '+display())
 save('opened')
 writer=subprocess.Popen(['python3','-u','-c','from pathlib import Path; import sys,time; p=Path(sys.argv[1]); f=p.open("r+b",buffering=0); n=0\nwhile len(sys.argv)>2 and not Path(sys.argv[2]).exists():time.sleep(.001)\nwhile True:\n f.seek(0);f.write(n.to_bytes(8,"little"));n+=1;time.sleep(.001)',str(project/'.prescient/postgres/state'),*([str(project/'.prescient/created.txt')] if a.writer_after_effect else [])],cwd=project,stdout=subprocess.DEVNULL,stderr=(root/'writer.stderr').open('wb'),start_new_session=True)
 task='This is a disposable real-interface repair exercise. Use the actual host tools and work in place. Run one command with declared write_paths ["main.py", ".prescient"]: python3 -c "from pathlib import Path; Path(\'main.py\').write_text(\'print(\'+repr(\'SOURCE-REPAIRED\')+\')\'+chr(10)); Path(\'.prescient/created.txt\').write_text(\'created by task\'+chr(10))". The .prescient/postgres/state file is live runtime data written by an already running external process. Do not stop it, read its contents, or alter its writer. Then run python3 main.py and report its exact output. Do not create or run unit tests, formal verification, Cargo, Clippy, lint gates . Use run_pipeline to perform this authorized implementation with the command tool. No further approval or questions are needed. The only deliverable source is main.py and .prescient/created.txt. Continue through the real tool operations until done.'
 (root/'task.txt').write_text(task);os.write(master,b'\x1b[200~'+task.encode()+b'\x1b[201~');pump(.5);send('\r')
 begin=time.monotonic();terminal=None;last_observation=None;stop_after_operations_sent=False
 while time.monotonic()-begin<a.seconds and process.poll() is None:
  pump(.5);save('current');values=records()
  if values:
   observation={key:values[-1].get(key) for key in ('run_id','status','phase','checkpoint','interruption_cause','remaining_work')}
   if observation != last_observation:
    with (root/'candidate-observations.jsonl').open('a') as f:f.write(json.dumps({'elapsed':round(time.monotonic()-begin,3),**observation})+'\n')
    last_observation=observation
  if a.stop_after_operations and not stop_after_operations_sent and verified_tool_output():
   send('/stop\r');stop_after_operations_sent=True
  if values and values[-1].get('status','').startswith(('Completed','Idle','Stopped','Recovery required','Cancelled')):
   terminal=values[-1];break
 if terminal is None:terminal=records()[-1] if records() else None
 result={'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'elapsed_seconds':round(time.monotonic()-begin,3),'writer_alive':writer.poll() is None,'source':(project/'main.py').read_text(),'created':(project/'.prescient/created.txt').read_text() if (project/'.prescient/created.txt').exists() else None,'record':terminal,'screen':display(),'oracle':'Source update plus command execution must finish while unrelated declared runtime data changes. Unstable evidence must remain conservative without interrupting the whole task.'}
 check=subprocess.run(['python3',str(project/'main.py')],cwd=project,capture_output=True,text=True,timeout=5)
 result['independent_cli_readback']={'exit_code':check.returncode,'stdout':check.stdout,'stderr':check.stderr}
 result['source_oracle_satisfied']=check.returncode==0 and check.stdout=='SOURCE-REPAIRED\n' and result['created']=='created by task\n'
 result['candidate_observation_failure']='workspace changed repeatedly while capturing content and repository context' in json.dumps(terminal)
 result['guide_unchanged']=hashlib.sha256((project/'AGENTS.md').read_bytes()).hexdigest()==original_guide
 result['unexpected_files']=[str(path.relative_to(project)) for path in project.rglob('*') if path.is_file() and str(path.relative_to(project)) not in ('main.py','AGENTS.md','.prescient/postgres/state','.prescient/created.txt')]
 result['actual_followup_command_observed']=verified_tool_output()
 # The affected boundary must actually include the changing runtime subtree. Merely
 # scoping the two deliverable files could pass their output oracle without exercising it.
 entries=[]
 for journal in (root/'state/peritus/daemon/product-runs').glob('*.files-*.jsonl'):
  entries.extend(json.loads(line) for line in journal.read_text().splitlines())
 result['runtime_subtree_enrolled']=any(entry.get('path')=='.prescient' and '.prescient/postgres/state' in entry.get('descendants',{}) for entry in entries)
 with runtime.open('rb') as f:final_counter=f.read(8)
 result['runtime_writer_counter']=int.from_bytes(final_counter,'little')
 result['runtime_writer_progress']=len(final_counter)==8 and final_counter!=initial_counter and result['runtime_writer_counter']>0
 result['exercise_complete_cancel_requested']=stop_after_operations_sent
 result['workflow_finished']=bool(terminal and terminal.get('status','').startswith(('Idle','Completed')))
 result['operation_oracle_satisfied']=all((result['source_oracle_satisfied'],result['actual_followup_command_observed'],result['writer_alive'],result['runtime_subtree_enrolled'],result['runtime_writer_progress'],result['guide_unchanged'],not result['unexpected_files'],not result['candidate_observation_failure']))
 save('final');(root/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps({k:v for k,v in result.items() if k not in ('record','screen')},indent=2));print('STATUS',terminal.get('status') if terminal else None)
finally:
 if writer is not None:
  writer.terminate();writer.wait(timeout=5)
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
raise SystemExit(0 if result.get('operation_oracle_satisfied') and (a.stop_after_operations or result.get('workflow_finished')) else 1)

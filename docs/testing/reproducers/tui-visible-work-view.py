#!/usr/bin/env python3
"""Replay an owned real-provider run through a fresh terminal, resizing and reconnecting."""
import argparse, codecs, fcntl, hashlib, json, os, pathlib, pty, random, select, signal, struct, subprocess, termios, time
import pyte

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--instance',type=pathlib.Path,required=True,help='Disposable instance created by tui-feedback.py')
parser.add_argument('--binary',type=pathlib.Path,required=True)
parser.add_argument('--output',type=pathlib.Path,required=True)
args=parser.parse_args()
root=args.instance.resolve();out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
binary=args.binary.resolve()
record_path=next((root/'state/peritus/daemon/workbench-v1/runs').glob('*.json'))
record=lambda:json.loads(record_path.read_text())
env=dict(os.environ,TERM='xterm-256color',TOKIO_WORKER_THREADS='2')
env.pop('NO_COLOR',None)
for name in ('config','state','cache','data'):env[f'XDG_{name.upper()}_HOME']=str(root/name)
master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',48,160,0,0))
process=subprocess.Popen([str(binary),'open',str(root/'project'),'--run',record()['run_id']],cwd=root/'project',env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
screen=pyte.Screen(160,48);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf-8')('replace');raw=bytearray();results=[]
def display():return '\n'.join(''.join(screen.buffer[y][x].data for x in range(screen.columns)) for y in range(screen.lines))
def pump(seconds=.15):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  if select.select([master],[],[],max(0,min(.05,end-time.monotonic())))[0]:
   try:data=os.read(master,65536)
   except OSError:return
   raw.extend(data);stream.feed(decoder.decode(data))
   if b'\x1b[?u' in data:os.write(master,b'\x1b[?1;2c')
   if b'\x1b[6n' in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
def wait(predicate,seconds=60):
 end=time.monotonic()+seconds
 while time.monotonic()<end and process.poll() is None:
  pump()
  if predicate():return
 raise AssertionError(display())
def save(label):
 (out/f'{label}.txt').write_text(display())
 (out/f'{label}.json').write_text(json.dumps([{'x':x,'y':y,'text':c.data,'fg':c.fg,'bg':c.bg,'bold':c.bold,'underline':c.underscore} for y,row in screen.buffer.items() for x,c in row.items()],ensure_ascii=False))
def resize(w,h):
 screen.resize(lines=h,columns=w);fcntl.ioctl(master,termios.TIOCSWINSZ,struct.pack('HHHH',h,w,0,0));os.kill(process.pid,signal.SIGWINCH);pump(.5)

def find(marker,label):
 os.write(master,b'\x1b[6~'*30);pump(.4)
 for _ in range(120):
  if marker in display():save(label);return
  os.write(master,b'\x1b[5~');pump(.05)
 raise AssertionError('Retained output not scrollable: '+marker)
def endpoint():
 identity=dict(line.split('=',1) for line in (root/'state/peritus/daemon/daemon.instance').read_text().splitlines())
 return next(line.split()[4] for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines() if f"pid={identity['pid']}," in line)
try:
 wait(lambda:'Steering:' in display())
 find('OBSERVATION-047','full-read-middle')
 row=next(row for row in screen.buffer.values() if 'OBSERVATION-047' in ''.join(row[x].data for x in range(screen.columns)))
 assert all(c.fg in ('default','white') for c in row.values() if c.data.strip()),'Read body remains faded'
 find('observations.txt:48:','search-hit-source')
 find('-# OLD-LINE-23','removed-hunk-middle')
 find('+# NEW-LINE-23','added-hunk-middle')
 find('Build it. Run it.','readable-skill')
 draft='UNSENT café 雪 — kept across sizes'
 os.write(master,b'\x1b[200~'+draft.encode()+b'\x1b[201~');pump(.3)
 rng=random.Random(20261019)
 dimensions=[(36,14),(48,20),(80,24),(140,40),(160,48)]+[(rng.randint(42,180),rng.randint(18,52)) for _ in range(8)]
 for i,(w,h) in enumerate(dimensions):
  resize(w,h);assert process.poll() is None
  assert 'Steering:' in display() and 'UNSENT' in display()
  save(f'size-{i}-{w}x{h}')
 resize(160,48);os.write(master,b'\x12');wait(lambda:'Steering:' in display() and draft in display())
 save('reconnected-draft')
 # Deliberately replace the unsent test draft before submitting one known authorized tiny edit.
 os.write(master,b'\x01\x0b');pump(.2)
 efficiency=next((root/'state/peritus/daemon/product-runs').glob('*.efficiency.jsonl'))
 previous=len(efficiency.read_text().splitlines());before_sequence=max(a[0] for a in record()['interaction']['activities'])
 instruction='Continue the existing task. Apply the known one-line edit with workspace_patch path patch.py old CONTINUED-CLI new FINAL-CLI, then run python3 patch.py. Use the retained plan and host-verified unchanged observations; no new discovery, file reads or design phase. Report the actual output. Do not run unit tests or unrelated gates.'
 os.write(master,b'\x1b[200~'+instruction.encode()+b'\x1b[201~\r')
 started=time.monotonic()
 wait(lambda:'FINAL-CLI' in (root/'project/patch.py').read_text(),240)
 elapsed=time.monotonic()-started
 events=[json.loads(line) for line in efficiency.read_text().splitlines()[previous:]]
 assert any(e.get('event')=='design-reused' for e in events)
 assert not any(e.get('event')=='model-view' and '-designer-' in e.get('invocation','') for e in events)
 tools=[a for a in record()['interaction']['activities'] if a[0]>before_sequence and a[1]==3]
 assert tools and 'run_pipeline' in tools[0][2],tools
 assert not any('workspace_read' in a[2] or 'workspace_list' in a[2] or 'workspace_search' in a[2] for a in tools),tools
 assert subprocess.check_output(['python3',str(root/'project/patch.py')],text=True).strip()=='FINAL-CLI'
 # Wait for the agent's actual command observation, then stop only this disposable task.
 wait(lambda:any(a[0]>before_sequence and a[1]==3 and 'Ran python3' in a[2] and 'FINAL-CLI' in a[3] for a in record()['interaction']['activities']),120)
 save('known-edit-and-real-output')
 subprocess.run([str(binary),'--endpoint',endpoint(),'runs','cancel','--run',record()['run_id']],env=env,check=True,capture_output=True,timeout=30)
 wait(lambda:record()['phase'] in (7,8,9,10,11),40)
 result={'status':'PASS','full_source_search_and_hunks_scrollable':True,'source_body_normal_foreground':True,'generated_sizes':len(dimensions),'reconnect_retains_draft':True,'continue_first_tool_is_handoff':True,'continue_has_no_inspection_or_designer_call':True,'continue_first_edit_seconds':round(elapsed,3),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}
 (out/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result),flush=True)
finally:
 if process.poll() is None:
  os.write(master,b'\x11')
  try:process.wait(timeout=12)
  except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=12)
 (out/'terminal.ansi').write_bytes(raw);(out/'last.txt').write_text(display());os.close(master)
 subprocess.run([str(binary),'--endpoint',endpoint(),'shutdown','--wait'],env=env,capture_output=True,timeout=40)

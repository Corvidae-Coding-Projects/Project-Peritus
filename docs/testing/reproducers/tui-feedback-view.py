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
try:
 wait(lambda:'Steering:' in display())
 # Find the real provider's report through the actual user's scrolling path.
 for i in range(80):
  if 'Results' in display() and 'STEER-ACCEPTED' in display() and 'Observed greeting' in display() and any(symbol in display() for symbol in ('┬','┼')):break
  os.write(master,b'\x1b[5~');pump(.1)
 else:raise AssertionError('Report not reachable through PageUp')
 save('formatted-report')
 assert any(symbol in display() for symbol in ('┬','┼'))
 assert '**Success' not in display() and '```python' not in display()
 heading_rows=[row for row in screen.buffer.values() if 'Results' in ''.join(row[x].data for x in range(screen.columns)) and 'STEER-ACCEPTED' in ''.join(row[x].data for x in range(screen.columns))]
 assert any(c.bold and c.fg!='default' for row in heading_rows for c in row.values()), 'Heading style absent from actual terminal cells'
 assert '"sha256":' not in display(), 'Routine tool JSON is still filling the conversation'
 results.append('actual-provider-report-rendered-and-scrollable')
 os.write(master,b'/details\r');pump(.8)
 original_hash=hashlib.sha256(b'def greet():\n    return "Before"\n\nif __name__ == "__main__":\n    print(greet())\n').hexdigest()
 for _ in range(60):
  if original_hash in display() or 'Structured response (raw JSON)' in display():break
  os.write(master,b'\x1b[5~');pump(.08)
 else:raise AssertionError('Full retained details are unreachable')
 save('expanded-details');results.append('compact-results-retain-expandable-original-details')
 os.write(master,b'\x1b');pump(.2)
 os.write(master,b'\x1b[<0;8;4M\x1b[<32;34;5M\x1b[<0;34;5m');pump(.2);save('selected-output')
 os.write(master,b'\x1b');pump(.1)
 draft='UNSENT café 雪 — draft survives'
 os.write(master,b'\x1b[200~'+draft.encode()+b'\x1b[201~');pump(.3)
 assert draft in display()
 # Generate dimensions independently from layout calculations.
 rng=random.Random(20261019)
 dimensions=[(36,14),(48,20),(80,24),(140,40),(160,48)]+[(rng.randint(42,180),rng.randint(18,52)) for _ in range(8)]
 for i,(w,h) in enumerate(dimensions):
  resize(w,h);assert process.poll() is None
  assert 'Steering:' in display(),display()
  assert 'UNSENT' in display(),display()
  save(f'size-{i}-{w}x{h}')
 results.append({'generated-size-transitions':len(dimensions),'seed':20261019})
 resize(160,48)
 os.write(master,b'\x12');wait(lambda:'Steering:' in display() and draft in display() and 'disconnected' not in display())
 save('reconnected-draft');results.append('real-reconnect-retains-unsent-draft-and-receipts')
 # Do not submit the draft or alter the user's clipboard.
 (out/'result.json').write_text(json.dumps({'status':'PASS','checks':results,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()},indent=2))
 print(json.dumps({'status':'PASS','checks':results}),flush=True)
finally:
 if process.poll() is None:
  os.write(master,b'\x11')
  try:process.wait(timeout=12)
  except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=12)
 (out/'terminal.ansi').write_bytes(raw);(out/'last.txt').write_text(display());os.close(master)

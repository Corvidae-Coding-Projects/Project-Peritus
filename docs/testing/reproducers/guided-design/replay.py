#!/usr/bin/env python3
"""Real PTY interview and optional implementation; no mocked model or application."""
import argparse, codecs, fcntl, hashlib, json, os, pty, random, re, select, struct, subprocess, termios, time
from pathlib import Path
import pyte

p=argparse.ArgumentParser()
p.add_argument('--binary',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--seed',type=int,default=20261015)
p.add_argument('--build',action='store_true')
p.add_argument('--read-only',action='store_true')
p.add_argument('--resume-after-show',action='store_true',help='Continue this owned replay after its Show step, preserving prior evidence')
a=p.parse_args()
if a.build and a.read_only:p.error('--build requires a writable fixture')
root=a.output.resolve();root.mkdir(parents=True,exist_ok=a.resume_after_show)
binary=a.binary.resolve();project=root/'project';project.mkdir(exist_ok=a.resume_after_show)
baseline='import argparse\np=argparse.ArgumentParser()\np.add_argument("name")\na=p.parse_args()\nprint("Hello, " + a.name + "!")\n'
if not a.resume_after_show:
 (project/'greet.py').write_text(baseline)
 (project/'AGENTS.md').write_text('# Greeting CLI\ngreet.py owns the CLI. Preserve ordinary output. Keep a single Python file, no dependencies. Exercise actual CLI operations; do not add or run unit tests, formal verification, or lint gates.\n')
env=os.environ.copy();env.update({f'XDG_{key.upper()}_HOME':str(root/key) for key in ('config','state','cache')});env['TERM']='xterm-256color'
flag=random.Random(a.seed).choice(['--uppercase','--loud','--caps'])
runs=root/'state/peritus/daemon/workbench-v1/runs'
artifacts=root/'state/peritus/daemon/product-runs'
results={'seed':a.seed,'replacement_flag':flag,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'steps':[]}
if a.resume_after_show:
 previous=json.loads((root/'result.json').read_text())
 assert previous['seed']==a.seed and previous['binary_sha256']==results['binary_sha256']
 assert previous['steps'][-1]['label']=='03-show'
 results=previous
assert (project/'greet.py').read_text()==baseline
project_versions=lambda:{str(path.relative_to(project)):hashlib.sha256(path.read_bytes()).hexdigest() for path in project.rglob('*') if path.is_file()}
original_project=project_versions()

class Terminal:
 def __init__(self,run=None):
  self.master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',42,160,0,0))
  self.process=subprocess.Popen([str(binary),'open',str(project),*(['--run',run] if run else [])],cwd=project,env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
  self.screen=pyte.Screen(160,42);self.stream=pyte.Stream(self.screen);self.decoder=codecs.getincrementaldecoder('utf-8')('replace');self.raw=bytearray();answered=set();deadline=time.monotonic()+60
  while time.monotonic()<deadline and self.process.poll() is None:
   self.pump();text=self.display()
   for prompt,answer in [('Trust this workspace?',('n' if a.read_only else 'y')+'\r'),('Providers (comma-separated','1\r'),('Choose a model number','1\r')]:
    if prompt in text and prompt not in answered:answered.add(prompt);os.write(self.master,answer.encode())
   attached=('Peritus · Design ·' in text) if run else ('Peritus ·' in text)
   if attached and ('Ready ·' in text or 'Idle — ready' in text):break
  else:raise RuntimeError('Terminal did not become ready: '+self.display())
 def display(self):return '\n'.join(''.join(self.screen.buffer[y][x].data for x in range(self.screen.columns)) for y in range(self.screen.lines))
 def pump(self):
  if select.select([self.master],[],[],.1)[0]:
   try:data=os.read(self.master,65536)
   except OSError:return
   self.raw.extend(data);self.stream.feed(self.decoder.decode(data))
   if b'\x1b[?u' in data:os.write(self.master,b'\x1b[?1;2c')
   if b'\x1b[6n' in data:os.write(self.master,f'\x1b[{self.screen.cursor.y+1};{self.screen.cursor.x+1}R'.encode())
 def save(self,label):
  (root/f'{label}.screen.txt').write_text(self.display());(root/f'{label}.ansi').write_bytes(self.raw)
 def close(self):
  if self.process.poll() is None:
   os.write(self.master,b'\x11')
   try:self.process.wait(timeout=5)
   except subprocess.TimeoutExpired:self.process.terminate();self.process.wait(timeout=5)
  os.close(self.master)

def records():return [json.loads(path.read_text()) for path in runs.glob('*.json')]
def current():
 values=records();return values[0] if values else None
def draft():
 path=next(artifacts.glob('*.guided-design.md'));text=path.read_text()
 header,markdown=text.split('\n',1)
 metadata=json.loads(header.removeprefix('<!-- peritus-guided-design-v1 ').removesuffix(' -->'))
 return path,metadata,markdown
def send(label,text,timeout=180):
 before=current();inc=before['interaction']['incorporated'] if before else 0
 received=before['interaction']['public_input_count'] if before else 0
 raw_start=len(terminal.raw);binding_enter=False
 (root/f'{label}.input.txt').write_text(text)
 os.write(terminal.master,b'\x01\x0b')  # Replace any single-line draft retained by an interrupted replay.
 # Command names are deliberate keyboard input; bracketed-pasted slash commands are non-executable.
 os.write(terminal.master,text.encode() if text.startswith('/') else b'\x1b[200~'+text.encode()+b'\x1b[201~')
 # Observe the paste/type event before a separate deliberate Enter, including after reattach.
 for _ in range(3):terminal.pump()
 os.write(terminal.master,b'\r')
 started=time.monotonic();deadline=started+timeout
 while time.monotonic()<deadline:
  terminal.pump();value=current()
  binding_notice=b"Loading this run's conversation; draft retained. Send again after it opens." in terminal.raw[raw_start:]
  retained_draft=time.monotonic()-started>2 and text[:60] in terminal.display()
  if not binding_enter and value and value['interaction']['public_input_count']==received and (binding_notice or retained_draft):
   # The real UI explicitly asks for a new Enter after authenticating a reopened destination.
   # Its durable input count proves the first Enter did not submit an operation.
   for _ in range(8):terminal.pump()
   os.write(terminal.master,b'\r');binding_enter=True
  if value and value['interaction']['incorporated']>inc and value['status'].startswith(('Idle','Completed','Stopped','Recovery required')):
   terminal.save(label);results['steps'].append({'label':label,'status':value['status']});print(label,value['status'],flush=True)
   if not value['status'].startswith(('Idle','Completed')):raise RuntimeError(value.get('summary','')+' '+value['status'])
   return value
 terminal.save(label+'-timeout');raise RuntimeError(label+' did not finish')
def shutdown():
 marker=root/'state/peritus/daemon/daemon.instance'
 if not marker.exists():return
 identity=dict(line.split('=',1) for line in marker.read_text().splitlines() if '=' in line)
 line=next(line for line in subprocess.check_output(['ss','-xlpn'],text=True).splitlines() if f"pid={identity['pid']}," in line)
 endpoint=re.search(r'(/\S+\.sock)',line).group(1)
 stopped=subprocess.run([str(binary),'--endpoint',endpoint,'--json','shutdown','--wait'],env=env,capture_output=True,text=True,timeout=45)
 if stopped.returncode:raise RuntimeError('Owned fixture shutdown failed: '+stopped.stderr)

terminal=None
try:
 if not a.resume_after_show:
  terminal=Terminal()
  send('01-interview','/design Improve the existing greeting CLI for command-line beginners. Add --shout: python3 greet.py Ada --shout must print exactly HELLO, ADA! plus one newline. Plain python3 greet.py Ada must remain Hello, Ada! plus one newline. Unicode names such as café 雪 must work. Keep the single Python file and no dependencies, unit tests, formal verification, or lint gates. Inspect greet.py, save a first draft, then ask one useful question. Do not implement anything yet.')
  path,first,markdown=draft();assert '--shout' in markdown and 'Ada' in markdown and 'café' in markdown
  assert project_versions()==original_project
  send('02-revise',f'Change the flag to {flag}; replace --shout in all requirements and acceptance commands. --shout is out of scope. Include a short help example. I am not sure about help formatting; recommend the simplest existing argparse approach. Keep plain output and Unicode examples. Save the revised design and present a short final review. Do not implement yet.')
  path,revised,markdown=draft();assert revised['version']>first['version'] and flag in markdown and 'CAFÉ' in markdown
  send('03-show','/design show')
 path,revised,markdown=draft();run=path.name.removesuffix('.guided-design.md')
 # This fixture's prose/inline-code design may gain presentation blank lines in a reply.
 # Require all nonblank saved lines, unchanged and in order, rather than blank-line identity.
 content_lines=lambda text:'\n'.join(line.rstrip() for line in text.splitlines() if line.strip())
 assert content_lines(markdown) in content_lines(current().get('summary',''))
 saved=path.read_bytes()
 if terminal is not None:terminal.close();terminal=None;shutdown()
 terminal=Terminal(run)
 assert path.read_bytes()==saved
 results['draft_survives_daemon_restart']=True
 send('04-ready',f'The {flag} design is ready. Save it as ready and show its path. Do not implement until I use the separate implementation action.')
 path,ready,markdown=draft();assert ready['stage']=='ready' and project_versions()==original_project
 results['approval_preserves_project']=True;(root/'approved-design.md').write_bytes(path.read_bytes())
 if a.build:
  send('05-build','/design build',timeout=600)
  cases=[(['Ada'],'Hello, Ada!\n'),(['Ada',flag],'HELLO, ADA!\n'),(['café 雪'],'Hello, café 雪!\n'),(['café 雪',flag],'HELLO, CAFÉ 雪!\n'),(['Zoë 🚲',flag],'HELLO, ZOË 🚲!\n'),(['two words'],'Hello, two words!\n')]
  outcomes=[]
  for args,expected in cases:
   observed=subprocess.run(['python3',str(project/'greet.py'),*args],cwd=project,capture_output=True,text=True,timeout=5)
   outcomes.append({'args':args,'stdout':observed.stdout,'exit':observed.returncode});assert observed.returncode==0 and observed.stdout==expected
  help_result=subprocess.run(['python3',str(project/'greet.py'),'--help'],capture_output=True,text=True,timeout=5);assert flag in help_result.stdout
  assert '--shout' not in (project/'greet.py').read_text()
  generated=list(artifacts.glob('*.design.md'));assert generated and flag in generated[0].read_text()
  results['cli_cases']=outcomes;results['implementation_design_uses_corrected_flag']=True
 results['passed']=True
finally:
 if terminal is not None:terminal.save('final');terminal.close()
 shutdown();(root/'result.json').write_text(json.dumps(results,indent=2,ensure_ascii=False))

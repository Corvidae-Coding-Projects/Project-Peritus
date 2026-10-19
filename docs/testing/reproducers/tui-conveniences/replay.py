#!/usr/bin/env python3
"""Drive the actual launcher/TUI/daemon through a PTY; no simulated product/provider.
Uses a disposable XDG tree and isolated X11 clipboard. Requires pyte and Xvfb/xclip.
"""
import argparse,base64,codecs,fcntl,hashlib,json,os,pty,random,select,shutil,signal,struct,subprocess,termios,time,zlib
from pathlib import Path
import pyte
from wcwidth import wcwidth
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--daemon',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--provider',action='store_true');p.add_argument('--reuse-runtime',type=Path);p.add_argument('--resume-response',type=Path);p.add_argument('--local-only',action='store_true');a=p.parse_args()
root=a.output.resolve();root.mkdir(parents=True,mode=0o700,exist_ok=False)
fixture=a.resume_response.resolve()if a.resume_response else root
if a.resume_response:
 if not json.loads((fixture/'owned-fixture.json').read_text()).get('owned_disposable_fixture'):raise SystemExit('Only an owned disposable fixture may resume')
 project=fixture/'project'
else:
 project=root/'project';project.mkdir();(root/'owned-fixture.json').write_text('{"owned_disposable_fixture":true}')
 (project/'notes.md').write_text('# Notes\nTRANSCRIPT-NEEDLE-ORCHID\n');(project/'src').mkdir();(project/'src'/'alpha café.rs').write_text('fn alpha() { println!("ORCHID"); }\n');(project/'src'/'beta Ω.rs').write_text('fn beta() {}\n');(project/'target').mkdir();(project/'target'/'ignored.rs').write_text('hidden');(project/'.gitignore').write_text('target/\n');subprocess.run(['git','init','-q',str(project)],check=True);subprocess.run(['git','-C',str(project),'add','.'],check=True);subprocess.run(['git','-C',str(project),'-c','user.name=Real testing','-c','user.email=real-testing@localhost','-c','core.hooksPath=/dev/null','commit','-qm','Disposable fixture'],check=True)
(root/'bin').mkdir();binary=root/'bin/peritus';shutil.copy2(a.binary,binary);shutil.copy2(a.daemon,root/'bin/peritusd')
editor=root/'editor with spaces';editor.write_text('#!/usr/bin/python3\nimport sys\nfrom pathlib import Path\np=Path(sys.argv[-1])\np.write_text(p.read_text()+"\\nEDITOR-RESTORED-雪")\n');editor.chmod(0o700)
env=dict(os.environ,TERM='xterm-256color',VISUAL="'"+str(editor)+"'");env.update({f'XDG_{k.upper()}_HOME':str(fixture/k) for k in ('config','state','cache','data')});env.pop('WAYLAND_DISPLAY',None);env.pop('NO_COLOR',None);env['CLICOLOR_FORCE']='1';env['COLORTERM']='truecolor'
if a.reuse_runtime:env['XDG_DATA_HOME']=str(a.reuse_runtime.resolve()/'data');env['PLAYWRIGHT_BROWSERS_PATH']=str(a.reuse_runtime.resolve()/'cache/ms-playwright')
for d in range(180,230):
 if not Path(f'/tmp/.X11-unix/X{d}').exists():break
xvfb=subprocess.Popen(['Xvfb',f':{d}','-screen','0','640x480x24','-nolisten','tcp'],stdout=subprocess.DEVNULL,stderr=(root/'xvfb.log').open('wb'));env['DISPLAY']=f':{d}'
master=slave=None;child=None;clipboard=None;raw=bytearray();results=[];screen=stream=decoder=None

def start(extra=None,run=None):
 global master,slave,child,screen,stream,decoder
 master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',42,140,0,0));screen=pyte.Screen(140,42);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf8')('replace');child=subprocess.Popen([str(binary),'open',str(project)]+(['--run',run]if run else[]),cwd=project,env=dict(env,**(extra or {})),stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
def text():
 rows=[]
 for y in range(screen.lines):
  row=[];skip=False
  for x in range(screen.columns):
   if skip:skip=False;continue
   value=screen.buffer[y][x].data or ' ';row.append(value);skip=wcwidth(value[0])==2
  rows.append(''.join(row))
 return '\n'.join(rows)
def pump(seconds=.15):
 until=time.monotonic()+seconds
 while time.monotonic()<until:
  if select.select([master],[],[],min(.03,max(.001,until-time.monotonic())))[0]:
   try:data=os.read(master,65536)
   except OSError:return
   raw.extend(data);stream.feed(decoder.decode(data))
   if b'\x1b[?u'in data:os.write(master,b'\x1b[?1;2c')
   if b'\x1b[6n'in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
 (root/'live.screen').write_text(text())
def send(data,delay=.2):os.write(master,data);pump(delay)
def wait(needle,seconds=15):
 end=time.monotonic()+seconds
 while time.monotonic()<end and child.poll()is None:
  pump(.1)
  if (needle() if callable(needle)else needle in text()):return
 raise AssertionError('Timed out waiting for '+str(needle)+'\n'+text())
def capture(name): (root/(name+'.screen')).write_text(text());(root/'terminal.ansi').write_bytes(raw)
def check(name,condition):
 passed=bool(condition);results.append({'scenario':name,'pass':passed});capture(name)
 if not passed:raise AssertionError(name+'\n'+text())
def paste(value):send(b'\x1b[200~'+value.encode()+b'\x1b[201~')
def clear():send(b'\x1b[1;5H\x1b[1;6F\x7f')
def command(value):clear();send(value.encode()+b'\r',.4)
def draft():
 lines=text().splitlines();top=max((i for i,s in enumerate(lines)if s.startswith('┌')and('Chat' in s or 'Message' in s or 'steer' in s or 'Plan' in s)),default=-1)
 if top<0:top=max((i for i,s in enumerate(lines)if s.startswith('┌')),default=-1)
 return '\n'.join(s[1:s.rfind('│')].rstrip()for s in lines[top+1:]if s.startswith('│'))
def bootstrap():
 seen=set();until=time.monotonic()+75
 while time.monotonic()<until and child.poll()is None:
  pump(.1);view=text()
  for prompt,reply in [('Trust this workspace?','y'),('Providers (comma-separated','1'if a.provider else'0'),('Choose a model number','manual gpt-6.1-sol'),('Reviewer provider [','0'),('Default [','1')]:
   if prompt in view and prompt not in seen:seen.add(prompt);send((reply+'\r').encode(),.1)
  if 'Peritus ·'in view and ('Ready ·'in view or 'Offline ·'in view or 'WaitingForUser ·'in view):pump(.5);return
 raise AssertionError('No ready UI\n'+text())
def close_client():
 global master
 send(b'\x11');child.wait(timeout=15);os.close(master);master=None

def png():
 def chunk(tag,value):return struct.pack('!I',len(value))+tag+value+struct.pack('!I',zlib.crc32(tag+value)&0xffffffff)
 return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('!2I5B',2,2,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0\xff\0\0\0\xff\0'*2))+chunk(b'IEND',b'')
try:
 if a.resume_response:
  record=json.loads(next((fixture/'state/peritus/daemon/workbench-v1/runs').glob('*.json')).read_text())
  start(run=record['run_id']);bootstrap()
  if 'Sessions ·'in text():send(b'\x1b')
 else:
  start();bootstrap();capture('00-ready')
  managed=next((fixture/'state/peritus/daemon/workspaces').glob('workspace_*'))
  (managed/'target').mkdir(exist_ok=True);(managed/'target/ignored.rs').write_text('ignored private fixture')
  send(b'\x0f');wait('notes.md');check('00b-ignore-rules','ignored.rs'not in text());send(b'\x1b')
  paste('UNSENT café 雪');send(b'\x0f');wait('Attach a project file');paste('alpcaf');wait('alpha café.rs');check('01-fuzzy-files','ignored.rs'not in text());send(b'\x1b');check('02-picker-draft-retained',draft()=='UNSENT café 雪')
  send(b'\x07',1);wait('EDITOR-RESTORED-雪');check('03-editor-roundtrip',draft()=='UNSENT café 雪\nEDITOR-RESTORED-雪')
  send(b'\x1b[20~');wait('Keyboard shortcuts');send(b'\r');send(b'\x1bx');wait('alt-x');send(b'\x1b');send(b'\x1bx');wait('Find in saved transcript');check('04-remap-live','Open a run'in text());send(b'\x1b');check('05-remap-draft-retained','EDITOR-RESTORED-雪'in draft())
  command('/keys find ctrl-q');wait('reserved');check('06-reserved-shortcut-rejected','reserved'in text());send(b'\x1b');command('/keys reset');send(b'\x1b');
  command('/theme');wait('Syntax theme');send(b'\x1b[B\r');wait('Syntax theme: light');check('07-theme-persisted',json.loads((fixture/'config/peritus/tui.json').read_text())['theme']=='light');send(b'\x1b');
  external=root/'external note.txt';external.write_text('Explicit external snapshot.\n');command('/new');send(b'@');wait('Attach a project file');paste(str(external));pump(.4);send(b'\r');wait('Preview only',20);check('07a-absolute-at-preserved',draft()=='');send(b'\x1b')
  command('/new');send(b'@');wait('Attach a project file');paste('beta');wait('beta Ω.rs');send(b'\r');wait('Preview only',20);check('07b-bare-at-first-attachment',draft()=='');send(b'\x1b');command('/new')
  # New-conversation file picker and explicit preview, not automatic attachment.
  clear();paste('KEEP-MY-DRAFT');send(b'\x0f');paste('alpha');wait('alpha café.rs');send(b'\r',1);wait('Files · explicit');check('08-new-conversation-file-draft',draft()=='KEEP-MY-DRAFT');wait('Preview only',20);check('09-file-preview-consent','KEEP-MY-DRAFT'in draft());send(b'\x1b');
  # Separate clipboard service with real PNG bytes. It never touches the desktop clipboard.
  end=time.monotonic()+5
  while not Path(f'/tmp/.X11-unix/X{d}').exists()and time.monotonic()<end:time.sleep(.05)
  clipboard=subprocess.Popen(['xclip','-selection','clipboard','-target','image/png','-in','-quiet'],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=(root/'clipboard.log').open('wb'),env=env,start_new_session=True);clipboard.stdin.write(png());clipboard.stdin.close();pump(.2)
  send(b'\x16',1);wait('Attach image');check('10-image-clipboard-preview','KEEP-MY-DRAFT'in draft());send(b'\x1b')
  command('/queue add QUEUE-ALPHA');wait('queued');send(b'\x1b');command('/queue add QUEUE-BETA');wait('QUEUE-BETA');send(b'\x1b');clear();paste('MAIN-DRAFT-RETAINS');send(b'\x1bOS');wait('Queue ·');send(b'e');wait('Editing saved input');paste(' amended 雪');send(b'\r',.8);wait('amended 雪');check('11-queue-edit-receipt',draft()=='MAIN-DRAFT-RETAINS');send(b'h',.8);wait('held');check('12-queue-hold-receipt',draft()=='MAIN-DRAFT-RETAINS');send(b'h',.8);wait('queued');send(b'\x1b[B');send(b'\x1b[1;3A',.8);check('13-queue-move',text().find('QUEUE-BETA')<text().find('QUEUE-ALPHA'));send(b'\x1b[3~');wait('Withdraw');send(b'\x1b');check('14-withdraw-cancel','QUEUE-BETA'in text());send(b'\x1b[3~');wait('Withdraw');send(b'\r',.8);wait(lambda:'QUEUE-BETA'not in text());check('14b-withdraw-receipt',draft()=='MAIN-DRAFT-RETAINS');send(b'\x1b[3~');wait('Withdraw');send(b'\r',.8);wait(lambda:'QUEUE-ALPHA'not in text());send(b'\x1b');
 if a.provider and not a.local_only:
  if not a.resume_response:command('/plan Read notes.md once with workspace_read. Then reply with the heading "ORCHID REPORT" and one fenced rust code block containing exactly this line: fn main() { println!("CODE-ORCHID"); }. Do not change files or run commands.');wait('CODE-ORCHID',90);wait(lambda:'Working ·'not in text(),120);capture('15-live-response')
  wait('CODE-ORCHID',20)
  def code_colors():
   row=next(i for i,line in enumerate(text().splitlines())if 'fn main()'in line)
   return {screen.buffer[row][x].fg for x in range(screen.columns)if screen.buffer[row][x].data.strip()}
  check('15a-code-syntax-colors',len(code_colors())>1)
  command('/theme plain');check('15b-plain-code',len(code_colors())==1)
  command('/theme dark');check('15c-dark-code',len(code_colors())>1)
  command('/theme light');send(b'\x1bc',.6)
  copied=subprocess.check_output(['xclip','-selection','clipboard','-out'],env=env,timeout=5).decode();check('16-copy-answer','ORCHID REPORT'in copied and 'CODE-ORCHID'in copied)
  send(b'\x1by');wait('Copy a code block');send(b'\r',.5);copied=subprocess.check_output(['xclip','-selection','clipboard','-out'],env=env,timeout=5).decode();check('17-copy-code','fn main()'in copied and '```'not in copied);send(b'\x1b');send(b'\x06');paste('TRANSCRIPT-NEEDLE-ORCHID');wait('tool',20);check('18-transcript-tool-search','tool'in text());send(b'\r');wait('Full result');check('19-search-body','TRANSCRIPT-NEEDLE-ORCHID'in text());send(b'\x1b');send(b'\x1b');
 # Bounded generated local workflows, each must retain unsent draft, never submit.
 randomizer=random.Random(20261019)
 for n in range(12):
  value='draft-'+str(n)+' '+randomizer.choice(['café','雪','Ω','[a-z].* -- -n']);clear();paste(value)
  for key,title in randomizer.sample([(b'\x0f','Attach a project file'),(b'\x1b[20~','Keyboard shortcuts'),(b'\x1b[21~','Syntax theme')],3):send(key);wait(title);send(b'\x1b');check(f'generated-{n}-{title[:3]}',draft()==value)
 run_paths=list((fixture/'state/peritus/daemon/workbench-v1/runs').glob('*.json'));run_record=json.loads(run_paths[0].read_text())if run_paths else None
 before_requests=run_record['progress']['model_requests']if run_record else 0
 run_id=run_record['run_id']if run_record else None
 close_client();start(extra={'VISUAL':str(root/'missing-editor')},run=run_id);bootstrap()
 if 'Sessions ·'in text():send(b'\x1b')
 command('/theme');wait('Current: light');check('20-theme-restart',True);send(b'\x1b')
 paste('FAILURE-DRAFT-雪');send(b'\x07',.5);wait('Cannot start external editor');check('21-editor-error-retains-draft',draft()=='FAILURE-DRAFT-雪')
 if a.provider and not a.local_only:
  send(b'\x06');paste('TRANSCRIPT-NEEDLE-ORCHID');wait('tool');check('22-search-after-restart','tool'in text());send(b'\x1b')
  after=json.loads(run_paths[0].read_text());check('23-local-controls-no-inference',after['progress']['model_requests']==before_requests)
 close_client()
 (root/'results.json').write_text(json.dumps({'passed':True,'seed':20261019,'scenarios':results,'binary_sha256':hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()},indent=2));print(json.dumps({'passed':True,'scenarios':len(results),'output':str(root)}))
finally:
 if master is not None:
  try:capture('final')
  except Exception:pass
  if child and child.poll()is None:
   try:send(b'\x11');child.wait(timeout=12)
   except Exception:child.terminate();child.wait(timeout=5)
  os.close(master)
 instance=fixture/'state/peritus/daemon/daemon.instance'
 if instance.exists():
  identity=dict(line.split('=',1)for line in instance.read_text().splitlines()if'='in line);pid=int(identity['pid'])
  try:owned=Path(os.readlink(f'/proc/{pid}/exe'))==binary.with_name('peritusd')
  except OSError:owned=False
  if owned:
   # The instance endpoint is a catalog identity, not the actual Unix socket path.
   listeners=subprocess.check_output(['ss','-xlpnH'],text=True).splitlines()
   for line in listeners:
    if f'pid={pid},'in line:
     stopped=subprocess.run([str(binary),'--endpoint',line.split()[4],'shutdown','--wait'],env=env,capture_output=True,timeout=35)
     if stopped.returncode:raise RuntimeError('Owned disposable daemon did not shut down: '+stopped.stderr.decode())
 if clipboard and clipboard.poll()is None:os.killpg(clipboard.pid,signal.SIGTERM);clipboard.wait(timeout=5)
 xvfb.terminate();xvfb.wait(timeout=5)
 if not(root/'results.json').exists():(root/'results.json').write_text(json.dumps({'passed':False,'scenarios':results},indent=2))

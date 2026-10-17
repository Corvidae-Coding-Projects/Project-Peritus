#!/usr/bin/env python3
"""Real terminal module steering, resume and ordinary chat journey."""
from pathlib import Path
import os,time,pexpect,pyte,json,subprocess,argparse,shutil,struct
parser=argparse.ArgumentParser(description='Real Peritus terminal/daemon built-in search journey; uses a signed-in provider')
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--provider-state',type=Path,required=True,help='Existing nonsecret product-state JSON with a working provider selection')
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
BASE=args.output.resolve();BASE.mkdir(parents=True,exist_ok=False);ROOT=BASE/'instance';BIN=args.binary.resolve()
(BASE/'workspace').mkdir();(BASE/'workspace/README.md').write_text('Disposable native-search workspace.\n')
state=ROOT/'state/peritus/product-state';state.mkdir(parents=True);provider=json.loads(args.provider_state.read_text());provider['providers']['account_models']['codex-account']='gpt-6.1-sol';(state/args.provider_state.name).write_text(json.dumps(provider))

e=dict(os.environ);e.update({f'XDG_{key}_HOME':str(ROOT/folder) for key,folder in [('CONFIG','config'),('STATE','state'),('CACHE','cache'),('DATA','data')]})
e.pop('PERITUS_WEB_SEARCH_URL',None)
screen=pyte.Screen(180,48);stream=pyte.Stream(screen);transcript=''
p=pexpect.spawn(str(BIN),['open',str(BASE/'workspace')],cwd=str(BASE/'workspace'),env=e,encoding='utf-8',timeout=1,dimensions=(48,180));raw=(BASE/'tui-watchdog.ansi').open('w')
def pump(seconds=.3):
 global transcript
 try:
  t=p.read_nonblocking(65536,timeout=seconds);raw.write(t);raw.flush();transcript+=t;stream.feed(t)
  if '\x1b[6n' in t:p.send('\x1b[1;1R')
  if '\x1b[c' in t:p.send('\x1b[?1;2c')
 except pexpect.TIMEOUT:pass
 except pexpect.EOF:raise RuntimeError('TUI exited: '+transcript[-2400:])
 view='\n'.join(screen.display);(BASE/'tui-watchdog-screen.txt').write_text(view);return view
def submit(text):
 p.send(text);time.sleep(.15);p.send('\r')

try:
 trusted=False;end=time.monotonic()+120
 while time.monotonic()<end:
  view=pump()
  if not trusted and 'Trust this' in transcript and '[Y/n]' in transcript:p.sendline('y');trusted=True
  if 'Enter sends' in view and 'Ready' in view:break
 else:raise TimeoutError(view)
 print('TUI connected',flush=True)
 fixture=BASE/'steering-package';fixture.mkdir();(fixture/'module.toml').write_text('schema=1\nid="local.steering"\nversion="1.0.0"\nname="Steering journey"\ndescription="An interruptible pinned stage"\n[[roles]]\nid="worker"\nmodel="writer"\ninstructions="Return exactly ORIGINAL-MODULE-RESULT. Do not use tools."\ntools=[]\n[[stages]]\nid="work"\nrole="worker"\n')
 for arguments in [('install',str(fixture)),('enable','local.steering')]:subprocess.run([str(BIN),'modules',*arguments],env=e,check=True,capture_output=True)
 def records():return [json.loads(p.read_text()) for p in (ROOT/'state/peritus/daemon/workbench-v1/runs').glob('*.json')]
 def wait(predicate,timeout=180):
  end=time.monotonic()+timeout
  while time.monotonic()<end:
   view=pump()
   if predicate(view):return view
  raise TimeoutError(view)
 submit('/module local.steering Exercise host-owned steering.')
 wait(lambda v:any(r['progress']['provider_started_unix_millis'] is not None for r in records()))
 submit('Pause the module and answer only MODULE STEERING RECEIVED.')
 view=wait(lambda v:'MODULE STEERING RECEIVED' in v and any(r['phase']==11 and r['interaction']['incorporated']>=2 for r in records()))
 job=next(ROOT.rglob('*.modules/*/job.json'));before=json.loads(job.read_text());assert before['completed']==[],before
 (BASE/'steering-before-resume.json').write_text(json.dumps(before,indent=2))
 submit('/module resume')
 wait(lambda v:'Module Steering journey completed.' in v and any(r['phase']==11 and r['interaction']['incorporated']>=3 for r in records()))
 after=json.loads(job.read_text());assert len(after['completed'])==1,after
 submit('/chat Reply exactly CHAT STILL WORKS.')
 wait(lambda v:'CHAT STILL WORKS' in v and any(r['phase']==11 and r['interaction']['incorporated']>=4 for r in records()))
 (BASE/'steering-evidence.json').write_text(json.dumps({'before':before,'after':after,'records':records()},indent=2))
 print('steering stayed pending until normal chat handled it, superseded stage uncommitted, explicit resume and ordinary chat passed',flush=True)

finally:
 if p.isalive():
  p.sendcontrol('q')
  try:p.expect(pexpect.EOF,timeout=8)
  except pexpect.TIMEOUT:p.terminate(force=True)
 p.close();raw.close();print('TUI closed',p.exitstatus,p.signalstatus,flush=True)

 # Stop only this disposable instance, using its exact persisted PID/socket identity.
 instance=ROOT/'state/peritus/daemon/daemon.instance'
 if instance.exists():
  info=dict(line.split('=',1) for line in instance.read_text().splitlines())
  for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines():
   if f"pid={info['pid']}," in line:
    subprocess.run([str(BIN),'--endpoint',line.split()[4],'shutdown','--wait'],check=True,timeout=35)
    break

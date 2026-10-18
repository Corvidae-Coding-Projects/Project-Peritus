#!/usr/bin/env python3
"""Real terminal module registry, four roles, interruption and daemon restart journey."""
from pathlib import Path
import os,time,pexpect,pyte,json,subprocess,argparse,shutil,struct
parser=argparse.ArgumentParser(description='Real Peritus terminal/daemon built-in search journey; uses a signed-in provider')
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--provider-state',type=Path,required=True,help='Existing nonsecret product-state JSON with a working provider selection')
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--port',type=int,default=4243)
parser.add_argument('--no-browser',action='store_true')
parser.add_argument('--managed',action='store_true',help='Use a real Git source checkout and a managed writable worktree')
args=parser.parse_args()
BASE=args.output.resolve();BASE.mkdir(parents=True,exist_ok=False);ROOT=BASE/'instance';BIN=args.binary.resolve()
(BASE/'workspace').mkdir();(BASE/'workspace/README.md').write_text('Disposable native-search workspace.\n')
state=ROOT/'state/peritus/product-state';state.mkdir(parents=True);provider=json.loads(args.provider_state.read_text());provider['providers']['account_models']['codex-account']='gpt-6.1-sol';(state/args.provider_state.name).write_text(json.dumps(provider))

if args.managed:
 for command in [['init','-b','main'],['add','README.md'],['-c','user.name=Module Replay','-c','user.email=module-replay@example.invalid','commit','-m','Disposable source']]:subprocess.run(['git','-C',str(BASE/'workspace'),*command],check=True,capture_output=True)

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
 web=None
 trusted=False;end=time.monotonic()+120
 while time.monotonic()<end:
  view=pump()
  if not trusted and 'Trust this' in transcript and '[Y/n]' in transcript:p.sendline('y');trusted=True
  if 'Enter sends' in view and 'Ready' in view:break
 else:raise TimeoutError(view)
 print('TUI connected',flush=True)
 fixture=BASE/'resource-package';fixture.mkdir();(fixture/'notes.txt').write_text('MODULE RESOURCE ORACLE: café 🦀.\n')
 (fixture/'module.toml').write_text('schema=1\nid="local.resource"\nversion="1.0.0"\nname="Resource reader"\ndescription="Reads declared immutable package data"\nresources=["notes.txt"]\n[[roles]]\nid="reader"\nmodel="writer"\ninstructions="Use module_resource_read to read notes.txt, then quote its single line exactly. Do not use other tools."\ntools=[]\n[[stages]]\nid="read"\nrole="reader"\n')
 for arguments in [('install',str(fixture)),('enable','local.resource')]:subprocess.run([str(BIN),'modules',*arguments],env=e,check=True,capture_output=True)
 web_log=(BASE/'web.log').open('w')
 if not args.no_browser:
  web=subprocess.Popen([str(BIN.with_name('peritus-web')),'--port',str(args.port),'--root',str(BASE/'workspace'),'--assets',str(Path.cwd()/'webui/dist'),'--cli',str(BIN)],env=e,stdout=web_log,stderr=subprocess.STDOUT)
 (BASE/'web-started').touch()
 if args.no_browser:(BASE/'browser-done').touch()
 def ask(message, predicate, timeout=300):
  submit(message);end=time.monotonic()+timeout;start=time.monotonic()
  while time.monotonic()<end:
   view=pump()
   if time.monotonic()-start>1 and predicate(view):
    for record_path in (ROOT/'state/peritus/daemon/workbench-v1/runs').glob('*.json'):
     record=json.loads(record_path.read_text());options=record['interaction'];users=[a[2] for a in options['activities'] if a[1]==1]
     if users and users[-1]==message and record['phase']==11 and options['incorporated']>=options['public_input_count']:return view
  raise TimeoutError(view)
 view=ask('/modules',lambda v:'peritus.source-brief' in v and 'ready for your next message' in v,40)
 print('registry shown through TUI',flush=True)
 view=ask('/module peritus.source-brief Read https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html and produce a very short brief: does Rust enforce ownership rules at compile time, and is the claim that Rust ownership requires garbage collection supported? Use only that official page, do not search. Keep all four stage outputs under 150 words each.', lambda v:'Module Source brief completed.' in v and 'ready for your next message' in v,420)
 (BASE/'module-report-screen.txt').write_text(view)
 jobs=list(ROOT.rglob('*.modules/*/job.json'))
 jobs=[j for j in jobs if json.loads(j.read_text())['module']=='peritus.source-brief']
 assert len(jobs)==1,jobs
 state=json.loads(jobs[0].read_text());assert len(state['completed'])==4,state
 for item in state['completed']:
  path=jobs[0].parent/(item['stage']+'-'+item['sha256']+'.txt');assert path.exists()
 (BASE/'job-evidence.json').write_text(json.dumps(state,indent=2))
 view=ask('/module status',lambda v:'"stage_count": 4' in v and 'ready for your next message' in v,40)
 view=ask('/module resume',lambda v:'Module Source brief completed.' in v and 'ready for your next message' in v,40)
 print('four role workflow, status, completed-job resume passed; awaiting browser',flush=True)
 end=time.monotonic()+180
 while not (BASE/'browser-done').exists() and time.monotonic()<end:pump()
 if not (BASE/'browser-done').exists():raise TimeoutError('browser replay did not complete')
 resume_package=BASE/'resume-package';resume_package.mkdir();(resume_package/'module.toml').write_text('schema=1\nid="local.resume"\nversion="1.0.0"\nname="Resume journey"\ndescription="Durable two-stage journey"\n[[roles]]\nid="first"\nmodel="writer"\ninstructions="Return only RETAINED-FIRST."\ntools=[]\n[[roles]]\nid="second"\nmodel="reviewer"\ninstructions="Read the predecessor if needed and return RESUMED-FINAL followed by its exact marker."\ntools=[]\n[[stages]]\nid="first"\nrole="first"\n[[stages]]\nid="second"\nrole="second"\ninputs=["first"]\n')
 for arguments in [('install',str(resume_package)),('enable','local.resume')]:subprocess.run([str(BIN),'modules',*arguments],env=e,check=True,capture_output=True)
 submit('/module local.resume Complete this durable journey.');end=time.monotonic()+180;resume_job=None
 while time.monotonic()<end:
  pump()
  for candidate in ROOT.rglob('*.modules/*/job.json'):
   js=json.loads(candidate.read_text())
   if js['module']=='local.resume' and len(js['completed'])==1:resume_job=candidate;break
  if resume_job:break
 else:raise TimeoutError('could not observe the first committed stage')
 first=json.loads(resume_job.read_text())['completed'][0];p.sendcontrol('c')
 end=time.monotonic()+30
 while time.monotonic()<end:
  view=pump()
  if 'Stopped' in view or 'Recovery required' in view:break
 p.sendcontrol('q');p.expect(pexpect.EOF,timeout=8);p.close()
 instance=ROOT/'state/peritus/daemon/daemon.instance';info=dict(line.split('=',1) for line in instance.read_text().splitlines())
 for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines():
  if f"pid={info['pid']}," in line:subprocess.run([str(BIN),'--endpoint',line.split()[4],'shutdown','--wait'],check=True,timeout=35);break
 run_id=resume_job.parent.parent.name.removesuffix('.modules');screen=pyte.Screen(180,48);stream=pyte.Stream(screen);transcript=''
 p=pexpect.spawn(str(BIN),['open',str(BASE/'workspace'),'--run',run_id],cwd=str(BASE/'workspace'),env=e,encoding='utf-8',timeout=1,dimensions=(48,180));end=time.monotonic()+90
 while time.monotonic()<end:
  view=pump()
  if 'Peritus ·' in view and ('Ready' in view or 'Stopped' in view or 'Recovery required' in view):break
 else:raise TimeoutError(view)
 view=ask('/module resume',lambda v:'Module Resume journey completed.' in v and 'ready for your next message' in v,180)
 after=json.loads(resume_job.read_text());assert len(after['completed'])==2 and after['completed'][0]==first,after
 assert (resume_job.parent/('first-'+first['sha256']+'.txt')).read_text().strip()=='RETAINED-FIRST'
 final=after['completed'][-1];assert 'RESUMED-FINAL' in (resume_job.parent/('second-'+final['sha256']+'.txt')).read_text()
 contexts=list(ROOT.rglob('*.context/modules/*/checkpoint.json'))
 # Actual lineage directories are independently owned, regardless of checkpoint basename.
 lineage_dirs=[p for p in ROOT.rglob('*.context/modules/*') if p.is_dir()]
 assert len(lineage_dirs)>=(6 if args.no_browser else 7),lineage_dirs
 (BASE/'memory-lineages.json').write_text(json.dumps([str(p) for p in lineage_dirs],indent=2))
 (BASE/'resume-evidence.json').write_text(json.dumps(after,indent=2))
 if args.managed:assert not subprocess.check_output(['git','-C',str(BASE/'workspace'),'status','--porcelain'],text=True).strip()
 print('stop, daemon shutdown/restart, exact-run reattach, unfinished-stage resume and committed-stage reuse passed',flush=True)


finally:
 if 'web' in globals() and web is not None:
  web.terminate();web.wait(timeout=8);web_log.close()
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

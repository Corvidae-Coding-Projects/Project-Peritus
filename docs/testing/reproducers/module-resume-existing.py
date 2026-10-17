"""Resume the saved disposable module-daemon.py journey using one human submission."""
from pathlib import Path
import argparse,os,time,json,subprocess,pexpect,pyte
ap=argparse.ArgumentParser();ap.add_argument('--binary',type=Path,required=True);ap.add_argument('--fixture',type=Path,required=True);ap.add_argument('--allow-completed',action='store_true');a=ap.parse_args()
BASE=a.fixture.resolve();ROOT=BASE/'instance';BIN=a.binary.resolve()
jobs=[p for p in ROOT.rglob('*.modules/*/job.json') if json.loads(p.read_text())['module']=='local.resume'];assert len(jobs)==1,jobs
job=jobs[0];before=json.loads(job.read_text());assert len(before['completed'])==1 or (a.allow_completed and len(before['completed'])==2),before
first=before['completed'][0];assert (job.parent/f"first-{first['sha256']}.txt").read_text().strip()=='RETAINED-FIRST'
run_id=job.parent.parent.name.removesuffix('.modules')
env=dict(os.environ);env.update({f'XDG_{key}_HOME':str(ROOT/folder) for key,folder in [('CONFIG','config'),('STATE','state'),('CACHE','cache'),('DATA','data')]})
p=pexpect.spawn(str(BIN),['open',str(BASE/'workspace'),'--run',run_id],cwd=BASE/'workspace',env=env,encoding='utf-8',dimensions=(48,180),timeout=1)
screen=pyte.Screen(180,48);stream=pyte.Stream(screen);raw=(BASE/'resume-reattach.ansi').open('w');transcript=''
def pump():
 global transcript
 try:
  text=p.read_nonblocking(65536,timeout=.2);raw.write(text);raw.flush();transcript+=text;stream.feed(text)
  if '\x1b[6n' in text:p.send('\x1b[1;1R')
  if '\x1b[c' in text:p.send('\x1b[?1;2c')
 except pexpect.TIMEOUT:pass
 view='\n'.join(screen.display);(BASE/'resume-reattach-screen.txt').write_text(view);return view
def record():return json.loads((ROOT/'state/peritus/daemon/workbench-v1/runs'/f'{run_id}.json').read_text())
def wait(predicate,timeout=180):
 end=time.monotonic()+timeout
 while time.monotonic()<end:
  view=pump()
  if predicate(view):return view
 raise TimeoutError(view)
def submit(text):
 end=time.monotonic()+2.5
 while time.monotonic()<end:pump()
 p.send(text);time.sleep(.15);p.send('\r')
try:
 wait(lambda v:'Peritus ·' in v and 'Chat ·' in v,90)
 # Intentionally submit immediately at reattach, while the binding may still be loading.
 initial=record()['interaction']['incorporated'];submit('/module resume')
 wait(lambda v:'Module Resume journey completed.' in v and record()['phase']==11 and record()['interaction']['incorporated']>initial)
 after=json.loads(job.read_text());assert len(after['completed'])==2 and after['completed'][0]==first,after
 final=after['completed'][-1];assert 'RESUMED-FINAL' in (job.parent/f"second-{final['sha256']}.txt").read_text()
 requests=record()['progress']['model_requests'];initial=record()['interaction']['incorporated'];submit('/module resume')
 wait(lambda v:record()['phase']==11 and record()['interaction']['incorporated']>initial)
 assert record()['progress']['model_requests']==requests
 lineages=[str(p) for p in ROOT.rglob('*.context/modules/*') if p.is_dir()];assert len(lineages)>=6,lineages
 initial=record()['interaction']['incorporated'];submit('/plan Reply exactly PLAN MODE READY.')
 wait(lambda v:'PLAN MODE READY' in v and record()['phase']==11 and record()['interaction']['incorporated']>initial)
 p.sendcontrol('q');p.expect(pexpect.EOF,timeout=8);p.close()
 screen=pyte.Screen(180,48);stream=pyte.Stream(screen);transcript=''
 p=pexpect.spawn(str(BIN),['open',str(BASE/'workspace'),'--run',run_id],cwd=BASE/'workspace',env=env,encoding='utf-8',dimensions=(48,180),timeout=1)
 wait(lambda v:'Peritus ·' in v,90);initial=record()['interaction']['incorporated'];requests=record()['progress']['model_requests'];submit('/module resume')
 view=wait(lambda v:'Module Resume journey completed.' in v and record()['phase']==11 and record()['interaction']['incorporated']>initial)
 assert 'Peritus · Chat ·' in view,view
 assert record()['progress']['model_requests']==requests
 bad=BASE/'bad-tool-package';bad.mkdir(exist_ok=True);(bad/'module.toml').write_text('schema=1\nid="local.effectful"\nversion="1.0.0"\nname="Forbidden tool"\ndescription="Must be rejected before inference"\n[[roles]]\nid="writer"\nmodel="writer"\ninstructions="Do not change files."\ntools=["workspace_patch"]\n[[stages]]\nid="work"\nrole="writer"\n')
 for arguments in [('install',str(bad)),('enable','local.effectful')]:subprocess.run([str(BIN),'modules',*arguments],env=env,check=True,capture_output=True)
 initial=record()['interaction']['incorporated'];submit('/module local.effectful This must be rejected.')
 wait(lambda v:'workspace_patch' in v and 'workspace_patch' in record()['interruption_cause'] and record()['interaction']['incorporated']>initial)
 assert 'workspace_patch' in record()['interruption_cause'],record()
 assert record()['progress']['model_requests']==requests

 assert not subprocess.check_output(['git','-C',str(BASE/'workspace'),'status','--porcelain'],text=True).strip() if (BASE/'workspace/.git').exists() else True
 (BASE/'resume-evidence.json').write_text(json.dumps({'before':before,'after':after,'requests_after_completion':requests,'memory_lineages':lineages},indent=2))
 print('first-submit reattach, actual restart/resume, retained stage, isolated memories, no inference on completed resume, explicit mode across reattach, and effectful-tool rejection passed',flush=True)
finally:
 if p.isalive():
  p.sendcontrol('q')
  try:p.expect(pexpect.EOF,timeout=8)
  except pexpect.TIMEOUT:p.terminate(force=True)
 p.close();raw.close()
 instance=ROOT/'state/peritus/daemon/daemon.instance'
 if instance.exists():
  info=dict(line.split('=',1) for line in instance.read_text().splitlines())
  for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines():
   if f"pid={info['pid']}," in line:subprocess.run([str(BIN),'--endpoint',line.split()[4],'shutdown','--wait'],check=True,timeout=35);break

#!/usr/bin/env python3
"""Actual CLI/TUI market installation, live-provider research and retained restart replay."""
import argparse,json,os,subprocess,time,shutil
from pathlib import Path
import pexpect,pyte

ap=argparse.ArgumentParser();ap.add_argument('--binary',type=Path,required=True);ap.add_argument('--provider-state',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);ap.add_argument('--port',type=int,default=4267);ap.add_argument('--no-browser',action='store_true');ap.add_argument('--counter',type=Path);ap.add_argument('--resume-after-stop',action='store_true');ap.add_argument('--serve-existing',action='store_true');args=ap.parse_args();args.reuse=args.resume_after_stop or args.serve_existing
B=args.output.resolve();B.mkdir(parents=True,exist_ok=args.reuse);R=B/'instance';W=B/'workspace';W.mkdir(exist_ok=args.reuse)
if not args.reuse:(W/'README.md').write_text('Disposable ordinary folder. Research must not edit this file.\n')
else:assert (W/'README.md').read_text()=='Disposable ordinary folder. Research must not edit this file.\n'
BIN=args.binary.resolve();env={**os.environ,**{f'XDG_{k}_HOME':str(R/v) for k,v in [('CONFIG','config'),('STATE','state'),('CACHE','cache'),('DATA','data')]}};env.pop('PERITUS_WEB_SEARCH_URL',None)
state=R/'state/peritus/product-state';state.mkdir(parents=True,exist_ok=args.reuse);provider=json.loads(args.provider_state.read_text());provider['providers']['account_models']['codex-account']='gpt-6.1-sol';
if not args.reuse:(state/args.provider_state.name).write_text(json.dumps(provider))
p=None;web=None;raw=(B/'terminal.ansi').open('a' if args.reuse else 'w');screen=None;stream=None;transcript='';steps=[];origins=[]
def pump(t=.15):
 global transcript
 try:
  text=p.read_nonblocking(65536,timeout=t);raw.write(text);raw.flush();stream.feed(text);transcript+=text
  if '\x1b[6n' in text:p.send('\x1b[1;1R')
  if '\x1b[c' in text:p.send('\x1b[?1;2c')
 except pexpect.TIMEOUT:pass
 except pexpect.EOF:raise RuntimeError('Terminal exited: '+transcript[-1200:])
 # pyte 0.8's display iterator can index an empty wide-character continuation.
 # Preserve its actual parsed cells, and always retain raw terminal bytes separately.
 try:view='\n'.join(screen.display)
 except IndexError:view='\n'.join(''.join(screen.buffer[y][x].data for x in range(screen.columns))for y in range(screen.lines))
 (B/'terminal-screen.txt').write_text(view);return view
def wait(predicate,timeout=90):
 end=time.monotonic()+timeout
 while time.monotonic()<end:
  view=pump()
  if predicate(view):return view
 raise TimeoutError(view)
def launch(run=None):
 global p,screen,stream,transcript
 screen=pyte.Screen(190,52);stream=pyte.Stream(screen);transcript=''
 p=pexpect.spawn(str(BIN),['open',str(W),*(['--run',run]if run else[])],cwd=str(W),env=env,encoding='utf-8',timeout=1,dimensions=(52,190))
 trusted=False;end=time.monotonic()+120
 while time.monotonic()<end:
  view=pump()
  if not trusted and 'Trust this' in transcript and '[Y/n]'in transcript:p.sendline('y');trusted=True
  if 'Peritus ·'in view and ('Ready'in view or 'Idle'in view or 'Stopped'in view or 'Cancelled'in view or 'Recovery required'in view):
   import hashlib
   instance=R/'state/peritus/daemon/daemon.instance';info=dict(l.split('=',1)for l in instance.read_text().splitlines());origins.append({'client_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest(),'daemon_sha256':hashlib.sha256(Path('/proc/'+info['pid']+'/exe').read_bytes()).hexdigest(),'daemon_pid':info['pid']});return
 raise TimeoutError(view)
def records():
 out=[]
 for path in (R/'state/peritus/daemon/workbench-v1/runs').glob('*.json'):
  try:out.append(json.loads(path.read_text()))
  except (OSError,json.JSONDecodeError):pass
 return out
def ask(text,timeout=420,stored=None):
 p.send(text);time.sleep(.12);p.send('\r');end=time.monotonic()+timeout;last=0
 while time.monotonic()<end:
  view=pump()
  for record in records():
   i=record['interaction'];users=[a[2]for a in i['activities']if a[1]==1]
   if users and users[-1]==(text if stored is None else stored) and i['incorporated']>=i['public_input_count']:
    if record['phase']==11:
     for _ in range(10):pump(.05)
     steps.append({'command':text,'phase':record['phase'],'summary':record.get('summary','')[:120]});return record
    if record['phase']==8:raise RuntimeError('Real run failed: '+json.dumps(record)[-2400:])
  if time.monotonic()-last>25:
   last=time.monotonic();print('Waiting:',view[-750:].replace('\n',' '),flush=True)
 raise TimeoutError(view)
def jobs():return list(R.rglob('*.modules/native-*/state/jobs/*/episode.json'))
def latest_job():return max(jobs(),key=lambda p:p.stat().st_mtime_ns)
def research(job):return job.parent/'research'
def current_plan(job):return json.loads((research(job)/'plan.json').read_text())
def plan_digest(job):
 import hashlib
 plan=current_plan(job);ordered={k:plan[k]for k in ['question','questions','scope','period','strategy','output','clarifications']};ordered['clarifications']=[{k:q[k]for k in ['question','suggestions']}for q in ordered['clarifications']]
 return hashlib.sha256(json.dumps(ordered,ensure_ascii=False,separators=(',',':')).encode()).hexdigest()
def cli(*command):return json.loads(subprocess.check_output([str(BIN),'modules',*command],env=env,text=True))
def shutdown_owned():
 instance=R/'state/peritus/daemon/daemon.instance'
 if not instance.exists():return
 info=dict(l.split('=',1)for l in instance.read_text().splitlines())
 for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines():
  if f"pid={info['pid']},"in line:subprocess.run([str(BIN),'--endpoint',line.split()[4],'shutdown','--wait'],env=env,check=True,timeout=40);return
def verify_report(job,words):
 data=research(job);pointer=json.loads((data/'report.json').read_text());report=json.loads((data/f"reports/{pointer['digest']}.json").read_text());reviewpointer=json.loads((data/'review.json').read_text());review=json.loads((data/f"reviews/{reviewpointer['digest']}.json").read_text())
 assert review['approved'] and review['report_digest']==pointer['digest'];assert report['findings'];assert any(q['status'] in ('supported','contested')for q in report['questions']),report
 admissions=[json.loads((data/f"admissions/{f['admission']}.json").read_text())for f in report['findings']]
 claims=' '.join(a['claim']for a in admissions).lower();assert any(word in claims for word in words),claims
 assert json.loads(job.read_text())['phase']==5
 (B/(job.parent.name+'-report.json')).write_text(json.dumps({'report':report,'review':review,'admissions':admissions},indent=2));return pointer['digest']
try:
 if args.serve_existing:
  record_file=next((R/'state/peritus/daemon/workbench-v1/runs').glob('*.json'));assert json.loads(record_file.read_text())['phase']==11
  launch(record_file.stem)
 else:
  if not args.resume_after_stop:
   launch()
   if args.counter:
    installed=cli('install',str(args.counter));assert not installed['enabled'];cli('enable','example.counter')
    for text in ['café 🦀','é Tokyo','zero']:
     record=ask('/module example.counter count '+text,40);result=json.loads(record['summary']);assert result=={'unicode_scalars':len(text),'words':len(text.split()),'utf8_bytes':len(text.encode())},result
    for text in ['café 🦀','é Tokyo']:
     record=ask('/module example.counter ask '+text,180);serialized=json.dumps(record,ensure_ascii=False);assert 'text_count' in serialized,serialized[-2000:]
    print('Independent native Rust extension direct commands and live model-callable function passed without rebuilding Peritus',flush=True)
   p.send('/modules\r');wait(lambda v:'Deep Research'in v and 'Working…'not in v,90);p.send('\x1b[H');time.sleep(.1);
   if args.counter:p.send('\x1b[B');time.sleep(.1)
   p.send('i');wait(lambda v:any('Deep Research · installed'in l and '· disabled'in l for l in v.splitlines())and 'Working…'not in v)
   installed=next(m for m in cli('list')['modules']if m['id']=='peritus.deep-research');assert not installed['enabled'];p.send('e');wait(lambda v:any('Deep Research · installed'in l and '· enabled'in l for l in v.splitlines())and 'Working…'not in v)
   assert next(m for m in cli('list')['modules']if m['id']=='peritus.deep-research')['enabled'];p.send('\r');wait(lambda v:'/module peritus.deep-research 'in v)
   topic='According only to the official Rust Book at https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html, what happens to a value when its owner goes out of scope? Answer for a nontechnical reader in a very short report. Scope is current documentation, not measured behavior. No code changes; no additional preferences are needed.'
   p.send('\x7f'*len('/module peritus.deep-research '));ask('/module peritus.deep-research '+topic);job=latest_job();first=plan_digest(job);assert not current_plan(job)['clarifications'],current_plan(job)
   assert json.loads(job.read_text())['phase']==1 and not list(research(job).glob('sources/*.json'))
   ask('/module peritus.deep-research start '+'0'*64,40);assert not (research(job)/'approved.json').exists()
   ask('/module peritus.deep-research resume',40);assert not (research(job)/'approved.json').exists() and not list(research(job).glob('sources/*.json'))
   ask('/module peritus.deep-research revise Keep exactly that single question, and explicitly distinguish documented ownership behavior from a performance measurement. All other scope stays the same.')
   job=latest_job();digest=plan_digest(job);assert digest!=first;assert not current_plan(job)['clarifications'];ask('/module peritus.deep-research start '+first,40);assert not (research(job)/'approved.json').exists()
   print('Market install/enable/select, saved plan, no pre-Start gathering, stale Start and explicit revision passed',flush=True)
   ask('/module peritus.deep-research start '+digest,600);verify_report(job,['drop','scope']);ask('/module peritus.deep-research show',40);ask('/module peritus.deep-research use-in-design',40)
   attachments=list(R.rglob('*.module-attachment.json'));assert len(attachments)==1;metadata=json.loads(attachments[0].read_text());assert metadata['name']=='Deep Research report' and metadata['bytes']>0;assert list(R.rglob('*.module-attachments/*.md'))
   before=len(jobs());ask('/module peritus.deep-research sources 32',40);assert len(jobs())==before
   design='Create a short design for a one-page educational explainer based on the attached research. Save an initial draft and ask one material next question. Do not implement.';ask('/design '+design,240,stored=design);drafts=list(R.rglob('*.guided-design.md'));assert drafts and any('scope' in d.read_text().lower()or 'owner'in d.read_text().lower()for d in drafts)
   chat=ask('/chat Reply exactly ORDINARY-CHAT-INTACT.',150,stored='Reply exactly ORDINARY-CHAT-INTACT.');assert 'ORDINARY-CHAT-INTACT' in json.dumps(chat)
   print('Real writer/gatherer/reviewer/synthesis/final-check report and Design attachment passed',flush=True)
   # Second non-coding job: stop after evidence is committed, restart the daemon and resume.
   ask('/module peritus.deep-research According to NASA Earth facts at https://science.nasa.gov/earth/facts/, approximately how many days is one Earth year? Use native web search to locate an original NASA source, then check that page. Very short report for a general reader; no coding task, no other preferences needed.')
   second=latest_job();digest=plan_digest(second);assert not current_plan(second)['clarifications'];p.send('/module peritus.deep-research start '+digest+'\r')
   wait(lambda v:any(2<=json.loads(j.read_text())['phase']<5 for j in [second]),300)
   committed=json.loads(second.read_text())['phase'];p.sendcontrol('c');wait(lambda v:any(r['phase']in(9,10)for r in records())and('Stopped'in v or 'Cancelled'in v or 'Recovery required'in v),45)
   p.sendcontrol('q');p.expect(pexpect.EOF,timeout=12);p.close();p=None;shutdown_owned()
  else:
   second=latest_job();committed=json.loads(second.read_text())['phase'];assert 2<=committed<5
   assert any(record['phase']in(9,10)for record in records()),'retained run must be stopped before replay'
  run=second.parents[4].name.removesuffix('.modules');launch(run);ask('/module peritus.deep-research resume',600)
  after=json.loads(second.read_text())['phase'];assert after>=committed;verify_report(second,['365','year'])
  assert (W/'README.md').read_text()=='Disposable ordinary folder. Research must not edit this file.\n';assert {p.name for p in W.iterdir()}=={'README.md'}
  print('Non-coding native search, stop, daemon restart, exact-run reattach and checkpoint reuse passed',flush=True)
  (B/'tui-evidence.json').write_text(json.dumps({'reused_pre_stop':args.resume_after_stop,'origins':origins,'steps':steps,'jobs':[str(j)for j in jobs()],'second_job_retained_stages':committed},indent=2))
 if not args.no_browser:
  (B/'browser-done').unlink(missing_ok=True)
  log=(B/'web.log').open('w');web=subprocess.Popen([str(BIN.with_name('peritus-web')),'--port',str(args.port),'--root',str(W),'--assets',str(Path.cwd()/'webui/dist'),'--cli',str(BIN)],env=env,stdout=log,stderr=subprocess.STDOUT)
  (B/'browser-ready').write_text(str(args.port));print('Browser ready',args.port,flush=True)
  end=time.monotonic()+900
  while not (B/'browser-done').exists()and time.monotonic()<end:pump()
  if not (B/'browser-done').exists():raise TimeoutError('Browser replay did not finish')
finally:
 if p is not None and p.isalive():
  p.sendcontrol('q')
  try:p.expect(pexpect.EOF,timeout=10)
  except pexpect.TIMEOUT:p.terminate(force=True)
 if web is not None:web.terminate();web.wait(timeout=15)
 shutdown_owned();raw.close()

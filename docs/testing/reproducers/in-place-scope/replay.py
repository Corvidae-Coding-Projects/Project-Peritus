"""Replay actual Peritus directory tracking on archived copies, without unit tests.
Requires Cargo, Python 3, git, and GNU patch. Outputs stay in the supplied disposable directory.
"""
import argparse,contextlib,hashlib,json,os,pathlib,random,select,shutil,stat,subprocess,sys,time,tarfile
parser=argparse.ArgumentParser()
parser.add_argument('--output',type=pathlib.Path,required=True)
parser.add_argument('--consumer',type=pathlib.Path,required=True)
a=parser.parse_args()
BASE=a.output.resolve();BASE.mkdir(parents=True,exist_ok=True)
CONSUMER=a.consumer.resolve()
SOURCE=pathlib.Path(__file__).resolve().parents[4]
TEMPLATE=BASE/'template';TEMPLATE.mkdir(exist_ok=True)
proc=subprocess.Popen(['git','archive','HEAD','README.md','docs/provider-contracts.md','crates/app/peritus-product-runner'],cwd=SOURCE,stdout=subprocess.PIPE)
with tarfile.open(fileobj=proc.stdout,mode='r|') as archive: archive.extractall(TEMPLATE,filter='data')
if proc.wait()!=0:raise RuntimeError('actual source archive failed')
MANIFEST={str(p.relative_to(TEMPLATE)):hashlib.sha256(p.read_bytes()).hexdigest() for p in TEMPLATE.rglob('*') if p.is_file()}
(BASE/'template-manifest.json').write_text(json.dumps(MANIFEST,sort_keys=True,indent=2))
RUN=BASE/'runs'/time.strftime('%Y%m%d-%H%M%S');RUN.mkdir(parents=True,mode=0o700)
ROWS=[]
def note(case,event):
 with (case/'operations.jsonl').open('a') as f:f.write(json.dumps(event,ensure_ascii=False)+'\n')
def mutate(case,body,*args):
 command=['python3','-c','from pathlib import Path; import os,shutil,sys; '+body,*map(str,args)]
 result=subprocess.run(command,capture_output=True,text=True,timeout=5)
 note(case,{'filesystem_process':command,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
 if result.returncode:raise RuntimeError('fixture operation failed: '+result.stderr)
def invoke(case,*args):
 command=[str(CONSUMER),*map(str,args)];start=time.monotonic()
 try:
  result=subprocess.run(command,capture_output=True,text=True,timeout=5)
  try:observation=json.loads(result.stdout)
  except ValueError:observation={'ok':False,'unparsed_stdout':result.stdout,'stderr':result.stderr}
  observation['exit_code']=result.returncode
 except subprocess.TimeoutExpired:observation={'ok':False,'timeout':True}
 note(case,{'public_consumer':command,'seconds':round(time.monotonic()-start,3),'observation':observation})
 return observation
def setup(name,expected,protected=()):
 case=RUN/name;case.mkdir(mode=0o700);root=case/'copy';shutil.copytree(TEMPLATE,root)
 state=case/'state';state.mkdir(mode=0o700);descriptor=state/'descriptor.json';journal=state/'baseline.jsonl'
 (case/'expected.json').write_text(json.dumps(expected,indent=2,ensure_ascii=False))
 result=invoke(case,'new',root,journal,73,json.dumps([str(root/path) for path in protected]),descriptor)
 if not result.get('ok'):raise RuntimeError(result)
 return case,root,descriptor,journal
def enroll(case,descriptor,path):
 result=invoke(case,'enroll',descriptor,path)
 if not result.get('ok'):raise RuntimeError('enroll '+path+': '+json.dumps(result))
def inspect(case,descriptor):
 result=invoke(case,'inspect',descriptor)
 if not result.get('ok'):raise RuntimeError('inspect: '+json.dumps(result))
 return result
def paths(result,key):return set(result[key])
def require_changes(result,required,allowed_extra=()):
 actual=paths(result,'changed_paths');required=set(required);allowed=set(allowed_extra)
 if not required.issubset(actual) or not actual.issubset(required|allowed):
  raise RuntimeError('changed_paths expected '+repr(sorted(required))+' allowed directory roots '+repr(sorted(allowed))+' observed '+repr(sorted(actual)))
def untouched(root,allowed=()):
 allowed=set(allowed);differences=[]
 for name,digest in MANIFEST.items():
  p=root/name
  if name not in allowed and (not p.is_file() or hashlib.sha256(p.read_bytes()).hexdigest()!=digest):differences.append(name)
 if differences:raise RuntimeError('unrelated copied Peritus files changed: '+repr(differences))
def scenario(name,expected,action,protected=()):
 case,root,descriptor,journal=setup(name,expected,protected)
 try:
  action(case,root,descriptor,journal);untouched(root,expected.get('allowed_copy_mutations',()))
  row={'case':name,'status':'PASS','fixture':str(case)}
 except Exception as error:row={'case':name,'status':'FAIL','failure':str(error),'fixture':str(case)}
 ROWS.append(row);(case/'result.json').write_text(json.dumps(row,indent=2));print(json.dumps(row,ensure_ascii=False),flush=True)
# The expected path sets below derive from explicit file operations, not production results.
def brown(case,root,descriptor,journal):
 folder='crates/app/peritus-product-runner';enroll(case,descriptor,folder)
 require_changes(inspect(case,descriptor),[])
 readme=root/folder/'README.md';review=root/folder/'src/review.rs'
 mutate(case,'p=Path(sys.argv[1]);p.write_bytes(p.read_bytes()+b"\\nBROWNFIELD APPENDED MARKER\\n");Path(sys.argv[2]).unlink();p=Path(sys.argv[3]);p.parent.mkdir(parents=True);p.write_text("CREATED NESTED MARKER\\n")',readme,review,root/folder/'generated/deep/created.txt')
 result=inspect(case,descriptor);require_changes(result,[folder+'/README.md',folder+'/src/review.rs',folder+'/generated/deep/created.txt'],[folder+'/generated',folder+'/generated/deep'])
 for path in [folder+'/README.md',folder+'/src/review.rs',folder+'/generated/deep/created.txt']:
  if path not in result['diff']:raise RuntimeError('diff omitted changed path '+path)
scenario('brownfield-nonempty-directory',{'expected':'Existing nonempty directory enrollment succeeds; detect one modified existing file, one deleted existing file, one nested creation; all other copy files unchanged.','allowed_copy_mutations':['crates/app/peritus-product-runner/README.md','crates/app/peritus-product-runner/src/review.rs']},brown)
def growth(case,root,descriptor,journal,absent=False,legacy=False):
 folder='output'
 if not absent:mutate(case,'Path(sys.argv[1]).mkdir()',root/folder)
 enroll(case,descriptor,folder)
 if legacy:
  values=[json.loads(line) for line in journal.read_text().splitlines()]
  for entry in values:entry['version']=1;entry.pop('descendants',None)
  journal.write_text(''.join(json.dumps(entry)+'\n' for entry in values));note(case,{'legacy_v1_journal_input':values})
 mutate(case,'p=Path(sys.argv[1]);p.parent.mkdir(parents=True,exist_ok=True);p.write_text("CREATED-A\\n");p=Path(sys.argv[2]);p.parent.mkdir(parents=True,exist_ok=True);p.write_text("CREATED-B\\n")',root/folder/'a.txt',root/folder/'nested/deep/b.txt')
 # Re-enrolling the now populated path must preserve the initial absent/empty baseline.
 enroll(case,descriptor,folder)
 reopened=descriptor.with_name('reopened.json');invoke(case,'serialize',descriptor,reopened)
 required=[folder+'/a.txt',folder+'/nested/deep/b.txt'];extra=[folder,folder+'/nested',folder+'/nested/deep']
 result=inspect(case,reopened);require_changes(result,required,extra)
 # Restoring an originally empty directory returns its baseline state.
 mutate(case,'p=Path(sys.argv[1]);shutil.rmtree(p);'+('' if absent else 'p.mkdir()'),root/folder)
 require_changes(inspect(case,reopened),[])
for absent,legacy in [(False,False),(True,False),(False,True),(True,True)]:
 name=('absent' if absent else 'empty')+'-directory-growth'+('-legacy-v1' if legacy else '')
 scenario(name,{'expected':'Declare '+('absent' if absent else 'empty')+' directory; command populates nested files; repeated enrollment/reopen retains creations; restoring original state is unchanged.','legacy_version':1 if legacy else None},lambda c,r,d,j,a=absent,l=legacy:growth(c,r,d,j,a,l))
def overlap(case,root,descriptor,journal,file_first=False):
 mutate(case,'p=Path(sys.argv[1]);p.parent.mkdir(parents=True);p.write_text("ORIGINAL BASELINE\\n")',root/'task/nested/existing.txt')
 first='task/nested/existing.txt' if file_first else 'task';second='task' if file_first else 'task/nested/existing.txt'
 enroll(case,descriptor,first)
 mutate(case,'Path(sys.argv[1]).write_text("INTERMEDIATE VALUE MUST NOT BECOME BASELINE\\n")',root/'task/nested/existing.txt')
 enroll(case,descriptor,second)
 mutate(case,'Path(sys.argv[1]).write_text("ORIGINAL BASELINE\\n")',root/'task/nested/existing.txt')
 require_changes(inspect(case,descriptor),[])
 mutate(case,'Path(sys.argv[1]).write_text("FINAL MODIFIED VALUE\\n")',root/'task/nested/existing.txt')
 result=inspect(case,descriptor);require_changes(result,['task/nested/existing.txt'])
 if 'ORIGINAL BASELINE' not in result['diff']:raise RuntimeError('diff lost earliest original baseline preview')
for file_first in [False,True]:
 scenario('overlap-'+('file-then-directory' if file_first else 'directory-then-file'),{'expected':'Overlapping enrollment preserves earliest original file baseline despite intermediate edits; restoring original is unchanged.'},lambda c,r,d,j,f=file_first:overlap(c,r,d,j,f))
def late_child_directory(case,root,descriptor,journal,growth):
 mutate(case,'p=Path(sys.argv[1]);p.parent.mkdir(parents=True);p.write_text("EARLIEST CHILD BASELINE\\n")',root/'task/sub/existing.txt')
 enroll(case,descriptor,'task')
 mutate(case,'Path(sys.argv[1]).write_text("MODIFIED CHILD\\n")',root/'task/sub/existing.txt')
 if growth=='wide':
  mutate(case,'p=Path(sys.argv[1]);[(p/("new-"+str(i)+".txt")).write_text("CREATED CHILD\\n") for i in range(4105)]',root/'task/sub')
  created=['task/sub/new-'+str(i)+'.txt' for i in range(4105)];extra=[]
 elif growth=='deep':
  mutate(case,'p=Path(sys.argv[1]);[(p:=p/("level-"+str(i))).mkdir() for i in range(70)];(p/"new.txt").write_text("CREATED CHILD\\n")',root/'task/sub')
  levels=['level-'+str(i) for i in range(70)];created=['task/sub/'+('/'.join(levels))+'/new.txt'];extra=['task/sub/'+('/'.join(levels[:i])) for i in range(1,71)]
 else:
  mutate(case,'Path(sys.argv[1]).write_text("CREATED CHILD\\n")',root/'task/sub/new.txt')
  created=['task/sub/new.txt'];extra=[]
 # A later child-directory request must retain every creation and the earlier file baseline,
 # including growth beyond the bounds that applied before the original parent admission.
 enroll(case,descriptor,'task/sub')
 reopened=descriptor.with_name('child-directory-reopened.json');invoke(case,'serialize',descriptor,reopened)
 result=inspect(case,reopened);require_changes(result,['task/sub/existing.txt',*created],extra)
 if 'EARLIEST CHILD BASELINE' not in result['diff']:raise RuntimeError('child-directory request lost original file baseline')
 mutate(case,'p=Path(sys.argv[1]);shutil.rmtree(p);p.mkdir();(p/"existing.txt").write_text("EARLIEST CHILD BASELINE\\n")',root/'task/sub')
 require_changes(inspect(case,reopened),[])
for growth in ['one','wide','deep']:
 scenario('late-child-directory-'+growth,{'expected':'Parent enrollment, child modification/creation, later child-directory declaration and reopen retain the earliest baseline and every new child; already admitted growth is not rejected. Restoring original subtree returns unchanged.','growth':growth},lambda c,r,d,j,g=growth:late_child_directory(c,r,d,j,g))
def transition(case,root,descriptor,journal,kind):
 if kind=='file-to-directory':
  mutate(case,'Path(sys.argv[1]).write_text("ORIGINAL FILE\\n")',root/'task')
 else:mutate(case,'p=Path(sys.argv[1]);p.parent.mkdir(parents=True);p.write_text("PREEXISTING A\\n");p=Path(sys.argv[2]);p.parent.mkdir(parents=True);p.write_text("PREEXISTING B\\n")',root/'task/a.txt',root/'task/nested/b.txt')
 enroll(case,descriptor,'task')
 if kind=='directory-delete':mutate(case,'shutil.rmtree(sys.argv[1])',root/'task');required=['task/a.txt','task/nested/b.txt'];extra=['task','task/nested']
 elif kind=='directory-to-file':mutate(case,'p=Path(sys.argv[1]);shutil.rmtree(p);p.write_text("REPLACEMENT FILE\\n")',root/'task');required=['task','task/a.txt','task/nested/b.txt'];extra=['task/nested']
 else:mutate(case,'p=Path(sys.argv[1]);p.unlink();p.mkdir();(p/"created.txt").write_text("CREATED CHILD\\n")',root/'task');required=['task','task/created.txt'];extra=[]
 result=inspect(case,descriptor);require_changes(result,required,extra)
for kind in ['directory-delete','directory-to-file','file-to-directory']:
 scenario(kind,{'expected':'Retain deleted existing descendants and detect changed path kind; new directory children are observed without a nonempty-directory error.'},lambda c,r,d,j,k=kind:transition(c,r,d,j,k))
def ignored(case,root,descriptor,journal):
 mutate(case,'p=Path(sys.argv[1]);p.mkdir();(p/"normal.txt").write_text("ORIGINAL NORMAL\\n")',root/'task')
 canaries=[]
 for name in ['protected','target','node_modules','.git','__pycache__','.venv']:
  folder=root/'task'/name;folder.mkdir();secret=folder/'never-read.txt';secret.write_text('EXCLUDED MARKER '+name+'\n');os.utime(secret,(946684800,946684800));os.mkfifo(folder/'never-open-fifo');canaries.append((secret,secret.stat().st_atime_ns))
 external=case/'outside-workspace';external.mkdir();secret=external/'never-read.txt';secret.write_text('LINK TARGET MUST NOT BE READ\n');os.utime(secret,(946684800,946684800));os.mkfifo(external/'never-open-fifo');canaries.append((secret,secret.stat().st_atime_ns));(root/'task/external-link').symlink_to(external,target_is_directory=True);(root/'task/file-link').symlink_to(secret)
 enroll(case,descriptor,'task')
 result=inspect(case,descriptor);require_changes(result,[])
 forbidden=['task/'+name for name in ['protected','target','node_modules','.git','__pycache__','.venv','external-link','file-link']]
 for path in result['paths']:
  if any(path==p or path.startswith(p+'/') for p in forbidden):raise RuntimeError('paths exposed excluded entry '+path)
 mutate(case,'Path(sys.argv[1]).write_text("MODIFIED NORMAL\\n")',root/'task/normal.txt')
 require_changes(inspect(case,descriptor),['task/normal.txt'])
 for secret,original_atime in canaries:
  if secret.stat().st_atime_ns!=original_atime:raise RuntimeError('excluded file was read: '+str(secret))
 for path in ['task/protected/never-read.txt','task/external-link/never-read.txt','task/file-link']:
  before=journal.read_bytes();result=invoke(case,'enroll',descriptor,path)
  if result.get('ok'):raise RuntimeError('explicit excluded enrollment accepted '+path)
  if journal.read_bytes()!=before:raise RuntimeError('rejected enrollment changed durable journal')
 # Exact cache-file enrollment is a separate authorized bounded inspection.
 cache_path='task/target/never-read.txt';cache_file=root/cache_path
 cache_atime=cache_file.stat().st_atime_ns
 enroll(case,descriptor,cache_path)
 if cache_file.stat().st_atime_ns==cache_atime:raise RuntimeError('explicit cache-file enrollment did not capture the requested file')
 if 'EXCLUDED MARKER target' not in journal.read_text():raise RuntimeError('explicit cache-file baseline content missing')
 mutate(case,'Path(sys.argv[1]).write_text("EXPLICITLY REQUESTED CACHE FILE MODIFIED\\n")',cache_file)
 require_changes(inspect(case,descriptor),['task/normal.txt',cache_path])
 for secret,original_atime in canaries:
  if secret!=cache_file and secret.stat().st_atime_ns!=original_atime:raise RuntimeError('explicit cache-file request read another excluded file: '+str(secret))

scenario('protected-cache-link-no-read',{'expected':'Parent discovery omits unread protected/cache/link subtrees and FIFO canaries. Explicit protected/link declarations reject atomically; a separately requested exact cache file is captured and its later modification observed without reading other excluded files.'},ignored,protected=['task/protected'])
def generated(case,root,descriptor,journal):
 seed=260213;rng=random.Random(seed);folder='varied-output';mutate(case,'Path(sys.argv[1]).mkdir()',root/folder);enroll(case,descriptor,folder)
 names=['plain.txt','space name.txt','café-雪.txt','deep/level/leaf.json','very-long-'+('z'*100)+'.txt']
 ordered=names[:];rng.shuffle(ordered);live=set()
 for index,name in enumerate(ordered):
  relative=folder+'/'+name
  mutate(case,'p=Path(sys.argv[1]);p.parent.mkdir(parents=True,exist_ok=True);p.write_text(sys.argv[2])',root/relative,'VALUE-'+str(index)+'-'+str(rng.randrange(10**6)))
  live.add(relative);enroll(case,descriptor,folder)
  result=inspect(case,descriptor);required=live;extra=[folder,folder+'/deep',folder+'/deep/level']
  require_changes(result,required,extra)
 for name in reversed(ordered):
  relative=folder+'/'+name;mutate(case,'Path(sys.argv[1]).unlink()',root/relative);live.remove(relative)
  require_changes(inspect(case,descriptor),live,[folder,folder+'/deep',folder+'/deep/level'])
 mutate(case,'shutil.rmtree(sys.argv[1]);Path(sys.argv[1]).mkdir()',root/folder);require_changes(inspect(case,descriptor),[])
scenario('varied-state-sequence',{'expected':'Repeated create/read/re-enroll/delete/read on Unicode, space, nested and long file names preserves initial empty baseline across every fresh consumer process.','seed':260213,'concrete_names':['plain.txt','space name.txt','café-雪.txt','deep/level/leaf.json','very-long-'+('z'*100)+'.txt']},generated)
def after_admission_growth(case,root,descriptor,journal,wide):
 folder='output'
 if wide:mutate(case,'Path(sys.argv[1]).mkdir()',root/folder)
 enroll(case,descriptor,folder)
 if wide:
  count=4105
  mutate(case,'p=Path(sys.argv[1]);[(p/("child-"+str(i).zfill(5)+".txt")).write_text("WIDE CREATED "+str(i)+"\\n") for i in range(int(sys.argv[2]))]',root/folder,count)
  required=[folder+'/child-'+str(i).zfill(5)+'.txt' for i in range(count)]
  extra=[folder]
 else:
  depth=70
  mutate(case,'p=Path(sys.argv[1]);[(p:=p/("level-"+str(i).zfill(2))).mkdir(parents=True,exist_ok=True) for i in range(int(sys.argv[2]))];(p/"leaf.txt").write_text("DEEP CREATED LEAF\\n")',root/folder,depth)
  levels=['level-'+str(i).zfill(2) for i in range(depth)]
  required=[folder+'/'+('/'.join(levels))+'/leaf.txt']
  extra=[folder]+[folder+'/'+('/'.join(levels[:i])) for i in range(1,depth+1)]
 enroll(case,descriptor,folder)
 reopened=descriptor.with_name('growth-reopened.json');invoke(case,'serialize',descriptor,reopened)
 result=inspect(case,reopened);require_changes(result,required,extra)
 if not result['diff'] or folder+'/' not in result['diff']:raise RuntimeError('post-growth diff omitted admitted output evidence')
 # An exact file remains separately writable after the admitted directory population grows.
 late=root/'late-exact.txt'
 mutate(case,'Path(sys.argv[1]).write_text("BEFORE EXACT PATCH\\n")',late)
 enroll(case,reopened,'late-exact.txt')
 patch_text='--- late-exact.txt\n+++ late-exact.txt\n@@ -1 +1 @@\n-BEFORE EXACT PATCH\n+AFTER EXACT PATCH\n'
 command=['patch','--batch','--forward','--no-backup-if-mismatch','-p0']
 result=subprocess.run(command,cwd=root,input=patch_text,capture_output=True,text=True,timeout=5)
 note(case,{'actual_patch_process':command,'patch':patch_text,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
 if result.returncode or late.read_text()!='AFTER EXACT PATCH\n':raise RuntimeError('actual exact-file patch failed after output growth')
 result=inspect(case,reopened);require_changes(result,[*required,'late-exact.txt'],extra)
 if 'BEFORE EXACT PATCH' not in result['diff'] or 'AFTER EXACT PATCH' not in result['diff']:raise RuntimeError('final diff lost exact-file patch evidence after output growth')
for wide in [True,False]:
 scenario('admitted-growth-'+('4105-children' if wide else '70-levels'),{'expected':'An already admitted '+('empty' if wide else 'absent')+' output directory can grow beyond the before-effect inventory bound; repeated declaration, fresh-process reopen/diff and later exact-file patch remain functional.','initial_empty':wide,'child_count':4105 if wide else 1,'directory_depth':0 if wide else 70,'before_effect_admission_guard':'not re-applied after effects'},lambda c,r,d,j,w=wide:after_admission_growth(c,r,d,j,w))
# Additional independent node-kind cases, specified before executing the changed library.
@contextlib.contextmanager
def live_socket(case,root,relative):
 command=[sys.executable,'-u','-c','import socket,sys; server=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM); server.bind(sys.argv[1]); server.listen(1); print("BOUND",flush=True); sys.stdin.read(); server.close()',relative]
 process=subprocess.Popen(command,cwd=root,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 try:
  if not select.select([process.stdout],[],[],5)[0] or process.stdout.readline().strip()!='BOUND':raise RuntimeError('real Unix socket process failed to become ready')
  if not stat.S_ISSOCK((root/relative).lstat().st_mode):raise RuntimeError('actual bound socket node missing')
  note(case,{'actual_socket_server_process':command,'cwd':str(root),'pid':process.pid,'ready':True,'type':'AF_UNIX/SOCK_STREAM'})
  yield
 finally:
  if process.stdin is not None:process.stdin.close()
  try:process.wait(timeout=5)
  except subprocess.TimeoutExpired:
   process.terminate()
   try:process.wait(timeout=2)
   except subprocess.TimeoutExpired:process.kill();process.wait(timeout=2)
  note(case,{'actual_socket_server_exit':process.returncode,'stderr':process.stderr.read()})
def external_canary(case,relative,marker):
 path=case/'outside-workspace'/relative
 mutate(case,'p=Path(sys.argv[1]);p.parent.mkdir(parents=True,exist_ok=True);p.write_text(sys.argv[2]);os.utime(p,(946684800,946684800))',path,marker)
 return path,path.stat().st_atime_ns
def unread(canaries):
 for path,atime in canaries:
  if path.stat().st_atime_ns!=atime:raise RuntimeError('external target was read: '+str(path))
def socket_birth(case,root,descriptor,journal):
 relative='browser.sock';enroll(case,descriptor,relative)
 with live_socket(case,root,relative):
  result=inspect(case,descriptor);require_changes(result,[relative])
  reopened=descriptor.with_name('socket-reopened.json');invoke(case,'serialize',descriptor,reopened)
  require_changes(inspect(case,reopened),[relative])
  enroll(case,reopened,relative);require_changes(inspect(case,reopened),[relative])
 mutate(case,'Path(sys.argv[1]).unlink()',root/relative)
 require_changes(inspect(case,reopened),[])
scenario('absent-path-to-live-unix-socket',{'expected':'An absent declared browser.sock becomes an actual bound Unix stream socket; metadata-only inspection/reopen/re-enrollment succeeds without reading it; removal returns original absence.'},socket_birth)
def fifo_birth(case,root,descriptor,journal):
 relative='stream.fifo';enroll(case,descriptor,relative)
 mutate(case,'os.mkfifo(sys.argv[1])',root/relative)
 if not stat.S_ISFIFO((root/relative).lstat().st_mode):raise RuntimeError('actual FIFO was not created')
 require_changes(inspect(case,descriptor),[relative])
 reopened=descriptor.with_name('fifo-reopened.json');invoke(case,'serialize',descriptor,reopened)
 require_changes(inspect(case,reopened),[relative]);enroll(case,reopened,relative)
 mutate(case,'Path(sys.argv[1]).unlink()',root/relative)
 require_changes(inspect(case,reopened),[])
scenario('absent-path-to-unopened-fifo',{'expected':'An admitted absent path becomes a real FIFO with no writer; inspect/reopen/re-enrollment never opens it or hangs; removal restores absence.','operation_timeout_seconds':5},fifo_birth)
def file_to_link(case,root,descriptor,journal):
 original='ORIGINAL TRACKED FILE\n';relative='tracked.txt'
 mutate(case,'Path(sys.argv[1]).write_text(sys.argv[2])',root/relative,original);enroll(case,descriptor,relative)
 canary=external_canary(case,'private-file.txt','EXTERNAL FILE TARGET MUST NOT BE READ\n')
 mutate(case,'p=Path(sys.argv[1]);p.unlink();p.symlink_to(sys.argv[2])',root/relative,canary[0])
 result=inspect(case,descriptor);require_changes(result,[relative]);unread([canary])
 if 'EXTERNAL FILE TARGET MUST NOT BE READ' in result['diff']:raise RuntimeError('diff contains external symlink target bytes')
 reopened=descriptor.with_name('link-reopened.json');invoke(case,'serialize',descriptor,reopened);require_changes(inspect(case,reopened),[relative]);unread([canary])
 mutate(case,'p=Path(sys.argv[1]);p.unlink();p.write_text(sys.argv[2])',root/relative,original)
 require_changes(inspect(case,reopened),[]);unread([canary])
scenario('tracked-file-to-external-symlink',{'expected':'A tracked regular file becoming a symlink is observable by its own metadata, without following/reading the external target; serde reopen preserves original baseline; restoring original type/content returns unchanged.'},file_to_link)
def directory_to_link(case,root,descriptor,journal):
 folder='task'
 mutate(case,'p=Path(sys.argv[1]);p.parent.mkdir(parents=True);p.write_text("ORIGINAL A\\n");p=Path(sys.argv[2]);p.parent.mkdir(parents=True);p.write_text("ORIGINAL B\\n")',root/folder/'a.txt',root/folder/'nested/b.txt')
 enroll(case,descriptor,folder)
 canaries=[external_canary(case,'a.txt','EXTERNAL MATCHING A MUST NOT READ\n'),external_canary(case,'nested/b.txt','EXTERNAL MATCHING B MUST NOT READ\n')]
 outside=case/'outside-workspace';mutate(case,'os.mkfifo(sys.argv[1])',outside/'external-fifo')
 mutate(case,'p=Path(sys.argv[1]);shutil.rmtree(p);p.symlink_to(sys.argv[2],target_is_directory=True)',root/folder,outside)
 required=[folder,folder+'/a.txt',folder+'/nested/b.txt'];extra=[folder+'/nested']
 result=inspect(case,descriptor);require_changes(result,required,extra);unread(canaries)
 if 'EXTERNAL MATCHING' in result['diff']:raise RuntimeError('directory transition read external matching child contents')
 if any(path.startswith(folder+'/external-fifo') for path in result['paths']):raise RuntimeError('parent inventory followed external tree')
 reopened=descriptor.with_name('directory-link-reopened.json');invoke(case,'serialize',descriptor,reopened);require_changes(inspect(case,reopened),required,extra);unread(canaries)
 mutate(case,'p=Path(sys.argv[1]);p.unlink();p.mkdir();(p/"a.txt").write_text("ORIGINAL A\\n");(p/"nested").mkdir();(p/"nested/b.txt").write_text("ORIGINAL B\\n")',root/folder)
 require_changes(inspect(case,reopened),[]);unread(canaries)
scenario('tracked-directory-to-external-symlink',{'expected':'A nonempty tracked directory becoming an external-tree symlink reports its type change and original descendant deletions. External tree deliberately has matching old child names, old-atime canaries and an unopened FIFO; none may be followed/read. Restoring original directory/content restores baseline.'},directory_to_link)
def initial_link_rejected(case,root,descriptor,journal):
 canary=external_canary(case,'initial-private.txt','INITIAL LINK TARGET MUST NOT READ\n');relative='new-link'
 mutate(case,'Path(sys.argv[1]).symlink_to(sys.argv[2])',root/relative,canary[0])
 before=journal.read_bytes() if journal.exists() else None
 result=invoke(case,'enroll',descriptor,relative)
 if result.get('ok'):raise RuntimeError('initial explicit symlink declaration was accepted')
 after=journal.read_bytes() if journal.exists() else None
 if before!=after:raise RuntimeError('initial symlink rejection changed durable baseline')
 require_changes(inspect(case,descriptor),[]);unread([canary])
scenario('initial-explicit-symlink-rejected',{'expected':'An initially undeclared symlink remains rejected atomically; its external target is never read and journal state is unchanged.'},initial_link_rejected)
def opaque_parent_inventory(case,root,descriptor,journal):
 folder='task';mutate(case,'p=Path(sys.argv[1]);p.mkdir();(p/"ordinary.txt").write_text("ORDINARY BEFORE\\n");os.mkfifo(p/"unopened.fifo")',root/folder)
 canary=external_canary(case,'private-tree/secret.txt','INVENTORY EXTERNAL TARGET MUST NOT READ\n')
 outside=canary[0].parent;mutate(case,'os.mkfifo(sys.argv[1]);Path(sys.argv[2]).symlink_to(sys.argv[3],target_is_directory=True)',outside/'external-fifo',root/folder/'external-link',outside)
 with live_socket(case,root,folder+'/browser.sock'):
  enroll(case,descriptor,folder);result=inspect(case,descriptor);require_changes(result,[]);unread([canary])
  if any(path.startswith(folder+'/external-link/') for path in result['paths']):raise RuntimeError('directory inventory traversed external link')
  mutate(case,'Path(sys.argv[1]).write_text("ORDINARY AFTER\\n")',root/folder/'ordinary.txt')
  result=inspect(case,descriptor);require_changes(result,[folder+'/ordinary.txt']);unread([canary])
  if 'INVENTORY EXTERNAL TARGET MUST NOT READ' in result['diff']:raise RuntimeError('directory diff read opaque target contents')
 mutate(case,'Path(sys.argv[1]).unlink()',root/folder/'browser.sock')
scenario('parent-inventory-mixed-opaque-nodes',{'expected':'Parent declaration containing actual socket/FIFO/symlink remains usable, inspects only opaque metadata or skips opaque entries, never opens FIFOs or traverses external links; ordinary file modification remains observable.'},opaque_parent_inventory)

def runtime_lifecycle(case,root,descriptor,journal):
 mutate(case,'p=Path(sys.argv[1]);p.mkdir();Path(sys.argv[2]).write_text("SOURCE BEFORE\\n")',root/'runtime',root/'owned.txt')
 enroll(case,descriptor,'runtime');enroll(case,descriptor,'owned.txt')
 mutate(case,'Path(sys.argv[1]).write_text("SOURCE AFTER\\n")',root/'owned.txt')
 stop=case/'stop-writer'
 code='''from pathlib import Path
import random,sys,time
root=Path(sys.argv[1]);stop=Path(sys.argv[2]);rng=random.Random(260215)
while not stop.exists():
 folder=root/'live space Ω';folder.mkdir(exist_ok=True)
 path=folder/('wal-'+str(rng.randrange(3))+'.dat')
 path.write_bytes(bytes([rng.randrange(256)])*rng.choice([128,1024,16384]))
 time.sleep(.0005)
 path.unlink()
 folder.rmdir()
'''
 writer=subprocess.Popen(['python3','-c',code,str(root/'runtime'),str(stop)],stderr=(case/'writer.stderr').open('w'))
 note(case,{'fixture_writer_pid':writer.pid,'seed':260215,'fresh_consumer_observations':40,'payload_bytes':[128,1024,16384]})
 try:
  for index in range(40):
   result=inspect(case,descriptor)
   changed=paths(result,'changed_paths')
   if 'owned.txt' not in changed or any(path!='owned.txt' and not path.startswith('runtime/') for path in changed):
    raise RuntimeError('runtime churn hid source change or escaped enrolled paths: '+repr(sorted(changed)))
   if writer.poll() is not None:raise RuntimeError('fixture writer ended before observation '+str(index))
 finally:
  stop.write_text('stop\n');writer.wait(timeout=5)
 if writer.returncode:raise RuntimeError('fixture writer failed: '+(case/'writer.stderr').read_text())
 require_changes(inspect(case,descriptor),['owned.txt'])
 if (root/'owned.txt').read_text()!='SOURCE AFTER\n':raise RuntimeError('inspection altered source')
scenario('runtime-create-remove-during-observation',{'expected':'Forty fresh public-API inspections succeed while a bounded writer creates/removes runtime files and their parent; an independent source change remains visible throughout; after churn only that source change remains.','seed':260215,'fresh_observations':40,'allowed_copy_mutations':[]},runtime_lifecycle)

(RUN/'results.json').write_text(json.dumps(ROWS,indent=2));print('RESULTS',RUN/'results.json',flush=True)
raise SystemExit(1 if any(row['status']=='FAIL' for row in ROWS) else 0)

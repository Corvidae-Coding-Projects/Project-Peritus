#!/usr/bin/env python3
"""Operate real retained Markdown output and durable composer history through the packaged TUI.

Requires an owned disposable tui-visible-work.py instance. Creates a separate conversation and
37 pending inputs through actual TUI controls; starts no model work. Never use a user's instance.
"""
import argparse, codecs, fcntl, hashlib, json, os, pathlib, pty, random, select, signal, struct, subprocess, termios, time
import pyte

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--instance',type=pathlib.Path,required=True)
parser.add_argument('--binary',type=pathlib.Path,required=True)
parser.add_argument('--output',type=pathlib.Path,required=True)
parser.add_argument('--smoke',action='store_true',help='Read-only final-binary recall/search/reconnect smoke; does not create inputs')
args=parser.parse_args();root=args.instance.resolve();out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
campaign_title='Recall generated '+out.name
binary=args.binary.resolve();record_path=next((root/'state/peritus/daemon/workbench-v1/runs').glob('*.json'))
def record():return json.loads(record_path.read_text())
original=record();messages=[a[2] for a in original['interaction']['activities'] if a[1]==1]
assert len(messages)>=3
# These expectations come from the original submitted inputs, never from the history implementation.
env=dict(os.environ,TERM='xterm-256color',TOKIO_WORKER_THREADS='2')
env.pop('NO_COLOR',None)
for n in ('config','state','cache','data'):env['XDG_'+n.upper()+'_HOME']=str(root/n)
env.setdefault('PERITUS_AUTOMATION_PYTHON','/home/doll/.local/share/peritus/automation/runtimes/playwright-1.64.0-pillow-13.0.0/bin/python')
env.setdefault('PLAYWRIGHT_BROWSERS_PATH','/home/doll/.cache/ms-playwright')
master=process=screen=stream=decoder=None;raw=bytearray();checks=[];paused_daemon=None
def launch(conversation=None):
 global master,process,screen,stream,decoder
 master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',48,160,0,0))
 target=['--conversation',conversation] if conversation else ['--run',original['run_id']]
 process=subprocess.Popen([str(binary),'open',str(root/'project'),*target],cwd=root/'project',env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
 screen=pyte.Screen(160,48);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf-8')('replace')
 wait(lambda:'Chat ·' in display() and 'Ctrl-C exits' in display())
 if conversation:key(b'\x1b')
def display():return '\n'.join(''.join(screen.buffer[y][x].data for x in range(screen.columns)) for y in range(screen.lines))
def pump(seconds=.15):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  if select.select([master],[],[],max(0,min(.03,end-time.monotonic())))[0]:
   try:data=os.read(master,65536)
   except OSError:return
   raw.extend(data);stream.feed(decoder.decode(data))
   if b'\x1b[?u' in data:os.write(master,b'\x1b[?1;2c')
   if b'\x1b[6n' in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
def wait(predicate,seconds=35):
 end=time.monotonic()+seconds
 while time.monotonic()<end and process.poll() is None:
  pump()
  if predicate():return
 raise AssertionError(display())
def key(data,seconds=.15):os.write(master,data);pump(seconds)
def paste(text):key(b'\x1b[200~'+text.encode()+b'\x1b[201~')
def save(label):
 (out/f'{label}.txt').write_text(display())
 (out/f'{label}.json').write_text(json.dumps([{'x':x,'y':y,'text':c.data,'fg':c.fg,'bg':c.bg,'bold':c.bold,'underline':c.underscore,'reverse':c.reverse} for y,row in screen.buffer.items() for x,c in row.items()],ensure_ascii=False))
def draft():
 lines=display().splitlines()
 tops=[i for i,line in enumerate(lines) if line.startswith('┌') and 'Chat' in line]
 assert tops,display()
 start=tops[-1];end=next(i for i in range(start+1,len(lines)) if lines[i].startswith('└'))
 return '\n'.join(line[1:line.rfind('│')].rstrip() for line in lines[start+1:end])
def flat_draft():return draft().replace('\n','')
def assert_draft(text):assert flat_draft()==text.replace('\n',''),(draft(),text,display())
def clear():key(b'\x1b[1;5H');key(b'\x1b[1;6F');key(b'\x7f')
def resize(w,h):
 screen.resize(lines=h,columns=w);fcntl.ioctl(master,termios.TIOCSWINSZ,struct.pack('HHHH',h,w,0,0));os.kill(process.pid,signal.SIGWINCH);pump(.3)
def find(marker,label):
 key(b'\x1b[F',.4)
 for _ in range(140):
  if marker in display():save(label);return
  key(b'\x1b[5~',.18)
 raise AssertionError('Not scrollable: '+marker)
def quit_client():
 global master
 if process.poll() is None:
  key(b'\x11',.1)
  try:process.wait(timeout=12)
  except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=12)
 os.close(master);master=None

def command(text):
 clear();key(text.encode(),.08);key(b'\r',.3)

def endpoint():
 identity=dict(line.split('=',1) for line in (root/'state/peritus/daemon/daemon.instance').read_text().splitlines())
 return next(line.split()[4] for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines() if f"pid={identity['pid']}," in line)

try:
 launch();save('ready')
 if args.smoke:
  import sys
  paste('SMOKE-DRAFT café 雪');key(b'\x1b[A');wait(lambda:flat_draft()==messages[-1]);key(b'\x1b');assert_draft('SMOKE-DRAFT café 雪')
  key(b'\x1bOR');paste('known one-line');wait(lambda:'FINAL-CLI' in display());key(b'\x1b[<0;8;4M\x1b[<32;34;5M\x1b[<0;34;5m');assert 'Message history' in display();key(b'\r');assert_draft(messages[-1]);key(b'\x1b');assert_draft('SMOKE-DRAFT café 雪')
  key(b'\x1bOS');wait(lambda:'Pending inputs' in display());assert_draft('SMOKE-DRAFT café 雪');key(b'\x1b')
  log=root/'state/peritus/logs/peritus-tui.log';connections=log.read_text().count('daemon session established');key(b'\x12');wait(lambda:log.read_text().count('daemon session established')>connections and 'Steering:' in display());pump(.3);assert_draft('SMOKE-DRAFT café 雪')
  save('final-recall-search-queue-reconnect');assert record()['interaction']==original['interaction']
  result={'status':'PASS','read_only_final_binary_recall_search_queue_reconnect':True,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()};(out/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result),flush=True);sys.exit(0)
 # Real provider's original 14 KB skill output spans protocol-sized chunks.
 find('Real Testing','formatted-skill-heading')
 assert '## Real Testing' not in display() and '# Real Testing' not in display()
 heading=[row for row in screen.buffer.values() if 'Real Testing' in ''.join(row[x].data for x in range(screen.columns))]
 assert any(c.bold and c.fg!='default' for row in heading for c in row.values())
 find('Fix and repeat','formatted-skill-section')
 assert '## 5.' not in display() and '**PASS**' not in display()
 section=[row for row in screen.buffer.values() if 'Fix and repeat' in ''.join(row[x].data for x in range(screen.columns))]
 assert any(c.bold for row in section for c in row.values())
 assert '· continued' not in display()
 find('-# OLD-LINE-23','literal-source-diff')
 assert '-# OLD-LINE-23' in display()
 command('/details');find('## 5. Fix and repeat','raw-document-details');key(b'\x1b')
 checks.append('retained-real-provider-skill-formatted-continuations-joined-raw-details-and-diffs-preserved')
 # Delay a real history reply by pausing only this owned daemon (no fake protocol).
 quit_client();launch()
 identity=dict(line.split('=',1) for line in (root/'state/peritus/daemon/daemon.instance').read_text().splitlines())
 paused_daemon=int(identity['pid']);os.kill(paused_daemon,signal.SIGSTOP)
 paste('ASYNC-DRAFT');key(b'\x1b[A');paste('-NEW-TYPING')
 os.kill(paused_daemon,signal.SIGCONT);paused_daemon=None
 pump(1);key(b'\x1b');assert_draft('ASYNC-DRAFT-NEW-TYPING');save('late-reply-keeps-new-typing')
 quit_client();launch()
 identity=dict(line.split('=',1) for line in (root/'state/peritus/daemon/daemon.instance').read_text().splitlines())
 paused_daemon=int(identity['pid']);os.kill(paused_daemon,signal.SIGSTOP)
 log=root/'state/peritus/logs/peritus-tui.log';connections=log.read_text().count('daemon session established')
 paste('RECONNECT-DRAFT');key(b'\x1b[A');key(b'\x12',.3)
 os.kill(paused_daemon,signal.SIGCONT);paused_daemon=None
 wait(lambda:log.read_text().count('daemon session established')>connections and 'Steering:' in display() and 'disconnected' not in display() and 'connecting' not in display());pump(.5)
 key(b'\x1b[A');wait(lambda:flat_draft()==messages[-1]);key(b'\x1b');assert_draft('RECONNECT-DRAFT')
 clear();checks.append('delayed-real-daemon-reply-and-interrupted-query-preserve-drafts-and-recover-history')
 # Preserve original cursor through recall; Unicode cursor offset is independently known.
 original_draft='UNSENT café 雪 tail';paste(original_draft);key(b'\x1b[D'*5)
 key(b'\x1b[A');wait(lambda:flat_draft()==messages[-1])
 key(b'\x1b[B');assert_draft(original_draft)
 key('§'.encode());assert_draft(original_draft[:-5]+'§'+original_draft[-5:]);save('restored-cursor')
 clear();paste('RESTORE-DRAFT')
 key(b'\x1b[A');wait(lambda:flat_draft()==messages[-1]);paste(' EDITED')
 key(b'\x1b[1;5H\x1b[A');wait(lambda:flat_draft()==messages[-2])
 key(b'\x1b[1;5F\x1b[B');assert_draft(messages[-1]+' EDITED')
 key(b'\x1b[1;5F\x1b[B');assert_draft('RESTORE-DRAFT')
 checks.append('up-down-recall-preserves-edits-original-draft-and-unicode-cursor')
 # Multiline editing and selection must retain their existing meaning.
 clear();paste('first α\nsecond 雪\nthird café');key(b'\x1b[A');key(b'!');assert_draft('first α\nsecond 雪!\nthird café')
 key(b'\x1b[1;5H');key(b'\x1b[1;2B');paste('REPLACED\n');assert_draft('REPLACED\nsecond 雪!\nthird café')
 checks.append('multiline-arrows-and-shift-selection-remain-editor-actions')
 clear();paste('SEARCH-DRAFT café 雪');key(b'\x1bOR');wait(lambda:'Message history' in display())
 paste('known one-line');pump(.3);assert 'FINAL-CLI' in display();save('filtered-history')
 key(b'\r');assert_draft(messages[-1]);key(b'\x1b');assert_draft('SEARCH-DRAFT café 雪')
 key(b'\x1bOR');paste('NO-MATCH-UNIQUE');assert 'No matching saved messages' in display();key(b'\x1b');assert_draft('SEARCH-DRAFT café 雪')
 checks.append('search-filters-enter-loads-without-send-and-escape-restores')
 # Each deletion/yank round trip is asserted from the original text, not source implementation.
 for content,motion,kill in [('α café 雪 end',b'\x01',b'\x0b'),('α café 雪 end',b'',b'\x15'),('α café 雪 end',b'',b'\x17'),('α\n雪',b'\x01\x1b[A',b'\x0b')]:
  clear();paste(content);key(motion+kill);key(b'\x19');assert_draft(content)
 checks.append('ctrl-k-u-w-yank-unicode-and-line-join-round-trips')
 clear();paste('QUEUE-DRAFT');key(b'\x1bOS');wait(lambda:'Pending' in display() or 'pending inputs' in display() or 'Input queue' in display());assert_draft('QUEUE-DRAFT');save('instructions-panel');key(b'\x1b');assert_draft('QUEUE-DRAFT')
 connections=log.read_text().count('daemon session established');key(b'\x12');wait(lambda:log.read_text().count('daemon session established')>connections and 'Steering:' in display() and 'disconnected' not in display());pump(.3);assert_draft('QUEUE-DRAFT')
 checks.append('f4-inspection-and-ctrl-r-reconnect-preserve-draft')
 # New empty conversation: enqueue through the actual UI; never Start or submit a coding run.
 command('/sessions new '+campaign_title);wait(lambda:campaign_title in display() and '/sessions new' not in flat_draft());save('separate-conversation');key(b'\x1b')
 generated=[f'RECALL-{i:02d} café 雪 {"older-only-needle" if i==0 else "editor example"}' for i in range(37)]
 for i,text in enumerate(generated):
  command('/queue add '+text)
  wait(lambda:not flat_draft())
  key(b'\x1b',.04)
  if i%10==0:print(json.dumps({'enqueued':i+1}),flush=True)
 key(b'\x1b[A');wait(lambda:flat_draft()==generated[-1]);save('newest-beyond-first-page');key(b'\x1b')
 key(b'\x1bOR');paste('OLDER-ONLY-NEEDLE');key(b'\x1bOB');wait(lambda:'RECALL-00' in display());assert 'FINAL-CLI' not in display();save('search-older-page');key(b'\r');assert_draft(generated[0]);key(b'\x1b')
 # Reopen the exact selected conversation using the ID shown by the session inspector.
 command('/sessions '+campaign_title);save('generated-session-identity')
 import re
 text=display();match=re.search(r'Conversation:\s*([0-9a-f]{32})',text)
 if not match:match=re.search(r'\bID\s+([0-9a-f]{32})',text)
 assert match,'Conversation identity not visible: '+text
 conversation=match.group(1);quit_client();launch(conversation)
 key(b'\x1b[A');wait(lambda:flat_draft()==generated[-1]);key(b'\x1b')
 checks.append('actual-37-input-paging-case-insensitive-search-conversation-isolation-and-reopen')
 # Dimensions are independent of layout code. Search preview is visibly bounded, draft is retained.
 paste('RESIZE-UNSENT café 雪');key(b'\x1bOR');rng=random.Random(20261019)
 dimensions=[(36,14),(48,20),(80,24),(140,40),(160,48)]+[(rng.randint(42,180),rng.randint(18,52)) for _ in range(8)]
 for i,(w,h) in enumerate(dimensions):
  resize(w,h);assert process.poll() is None;assert 'Message history' in display();assert 'RESIZE-UNSENT' in display();save(f'size-{i}-{w}x{h}')
 resize(160,48);key(b'\x1b');assert_draft('RESIZE-UNSENT café 雪')
 assert record()['interaction']==original['interaction'],'Read-only replay altered original run interaction'
 assert record()['phase']==original['phase'],'Read-only replay resumed original run'
 checks.append({'generated_size_transitions':len(dimensions),'seed':20261019})
 result={'status':'PASS','checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'new_conversation':conversation}
 (out/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result),flush=True)
finally:
 if paused_daemon is not None:os.kill(paused_daemon,signal.SIGCONT)
 if process is not None and master is not None:
  try:save('last')
  finally:quit_client()
 (out/'terminal.ansi').write_bytes(raw)

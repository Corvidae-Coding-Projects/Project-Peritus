#!/usr/bin/env python3
"""Read actual Markdown and code through signed-in Peritus tools and inspect the real terminal."""
import argparse,codecs,fcntl,hashlib,json,os,pathlib,pty,select,signal,struct,subprocess,termios,time
import pyte
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=pathlib.Path,required=True);p.add_argument('--provider-state',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);p.add_argument('--replay',action='store_true',help='Reopen the retained provider run without new inference');p.add_argument('--handle',help='Exact native document observation handle for --recover');p.add_argument('--recover',action='store_true',help='Perform one new read-only saved-context request');a=p.parse_args()
r=a.output.resolve();r.mkdir(mode=0o700,parents=True,exist_ok=a.replay);project=r/'project';project.mkdir(exist_ok=a.replay)
document='# Display contract\n\n## Decisions\n\n**STRONG-MARKER** and *emphasis* with `inline`.\n\n- LIST-ONE\n- LIST-TWO\n\n[Official example](https://example.com/)\n\n| File | Status |\n| --- | --- |\n| DOC.md | observed |\n\nsha256: KEEP-REAL-DOCUMENT-TEXT\n\n```rust\n'+''.join(f'let code_{i:03d} = "CODE-MARKER-{i:03d} literal **stars**";\n' for i in range(190))+'```\n\n## After continuation\n\nFINAL-DOCUMENT-MARKER café 雪\n\nEntity control: &#27;[31mINERT-ENTITY\n'
(project/'DOC.md').write_text(document);(project/'source.py').write_text('# SOURCE-HEADING\ntext = "**SOURCE-LITERAL**"\nprint(text)\n')
(project/'AGENTS.md').write_text('# Disposable formatting fixture\nRead-only request. Use only the requested file and skill reads; no coding, unit tests, gates, or whole-repository discovery.\n')
env=dict(os.environ,TERM='xterm-256color',TOKIO_WORKER_THREADS='2');env.pop('NO_COLOR',None)
for name in ('state','config','cache','data'):env['XDG_'+name.upper()+'_HOME']=str(r/name)
env.setdefault('PERITUS_AUTOMATION_PYTHON','/home/doll/.local/share/peritus/automation/runtimes/playwright-1.64.0-pillow-13.0.0/bin/python');env.setdefault('PLAYWRIGHT_BROWSERS_PATH','/home/doll/.cache/ms-playwright')
sd=r/'state/peritus/product-state';sd.mkdir(parents=True,exist_ok=True)
if not a.replay:(sd/a.provider_state.name).write_bytes(a.provider_state.read_bytes())
m,s=pty.openpty();fcntl.ioctl(s,termios.TIOCSWINSZ,struct.pack('HHHH',48,160,0,0));binary=a.binary.resolve();target=['--run',json.loads(next((r/'state/peritus/daemon/workbench-v1/runs').glob('*.json')).read_text())['run_id']] if a.replay else []
proc=subprocess.Popen([str(binary),'open',str(project),*target],cwd=project,env=env,stdin=s,stdout=s,stderr=s,start_new_session=True);os.close(s)
screen=pyte.Screen(160,48);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf8')('replace');raw=bytearray()
def display():return '\n'.join(''.join(screen.buffer[y][x].data for x in range(screen.columns)) for y in range(screen.lines))
def pump(secs=.15):
 end=time.monotonic()+secs
 while time.monotonic()<end:
  if select.select([m],[],[],max(0,min(.04,end-time.monotonic())))[0]:
   try:d=os.read(m,65536)
   except OSError:return
   raw.extend(d);stream.feed(decoder.decode(d))
   if b'\x1b[?u' in d:os.write(m,b'\x1b[?1;2c')
   if b'\x1b[6n' in d:os.write(m,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
def wait(predicate,seconds=180):
 end=time.monotonic()+seconds
 while time.monotonic()<end and proc.poll() is None:
  pump()
  if predicate():return
 raise AssertionError(display())
def key(b,t=.25):os.write(m,b);pump(t)
def save(label):
 (r/(label+'.txt')).write_text(display());(r/(label+'.json')).write_text(json.dumps([{'y':y,'x':x,'text':c.data,'fg':c.fg,'bg':c.bg,'bold':c.bold,'underline':c.underscore} for y,row in screen.buffer.items() for x,c in row.items()],ensure_ascii=False))
def find(marker,label):
 key(b'\x1b[F',.4)
 for _ in range(160):
  if marker in display():save(label);return
  key(b'\x1b[5~',.2)
 raise AssertionError('Missing rendered marker '+marker)
def record():
 paths=list((r/'state/peritus/daemon/workbench-v1/runs').glob('*.json'));return json.loads(paths[0].read_text()) if paths else None
try:
 answered=set();end=time.monotonic()+120
 while time.monotonic()<end:
  pump()
  if 'Trust this workspace?' in display() and 'trust' not in answered:key(b'y\r');answered.add('trust')
  if ('Ctrl-C exits' if a.replay else 'Ready · Enter sends') in display():break
 else:raise AssertionError(display())
 if not a.replay:
  key(b'/effort low\r');key(b'/plan\r')
  request='Read-only presentation check. Directly use workspace_read on DOC.md (max_bytes 65536, all lines), workspace_read on source.py, and skill_read for real-testing SKILL.md. Do not modify files, run commands, invoke run_pipeline, or run tests. After the three reads, reply with only DOCUMENT-READ-COMPLETE.'
  key(b'\x1b[200~'+request.encode()+b'\x1b[201~\r')
  wait(lambda:record() and all(any(name in row[2] for row in record()['interaction']['activities'] if row[1]==3) for name in ('workspace_read path=DOC.md','workspace_read path=source.py','skill_read')))
  wait(lambda:record() and any('DOCUMENT-READ-COMPLETE'==row[2].strip() for row in record()['interaction']['activities'] if row[1]==2),240)
 if a.recover:
  assert a.handle, '--recover requires --handle for an exact native tool result, not binary search windows'
  request=f'Use context_read with observation_ids ["{a.handle}"], query null, cursor null, offset 0 and max_bytes 65536 to recover the exact Markdown file observation. Do not use search, reread workspace files, or change anything. Reply only EXACT-CONTEXT-READ-COMPLETE.'
  key(b'\x1b[200~'+request.encode()+b'\x1b[201~\r')
  wait(lambda:record() and any(row[1]==3 and row[2].startswith('Read document from saved context') for row in record()['interaction']['activities']),180)
  wait(lambda:record() and any(row[1]==2 and row[2].strip()=='EXACT-CONTEXT-READ-COMPLETE' for row in record()['interaction']['activities']),180)
 if any(row[1]==3 and row[2].startswith('Read document from saved context') for row in record()['interaction']['activities']):
  find('Read document from saved context','recovered-markdown')
  assert 'Display contract' in display() and '1: # Display contract' not in display()
  recovered_rows=[row for row in screen.buffer.values() if 'Display contract' in ''.join(row[x].data for x in range(screen.columns))]
  assert any(c.bold for row in recovered_rows for c in row.values() if c.data.strip())
 find('Display contract','heading-and-table');assert '# Display contract' not in display() and '**STRONG-MARKER**' not in display();assert any(c.bold for row in screen.buffer.values() for c in row.values() if c.data.strip())
 assert any(glyph in display() for glyph in ('┬','┼'));assert 'KEEP-REAL-DOCUMENT-TEXT' in display()
 find('CODE-MARKER-150','continued-fence');assert '· continued' not in display();assert '**stars**' in display()
 code_rows=[row for row in screen.buffer.values() if 'CODE-MARKER-150' in ''.join(row[x].data for x in range(screen.columns))];assert any(c.bg in ('black','000000') for row in code_rows for c in row.values() if c.data.strip())
 find('After continuation','after-fence');assert '## After continuation' not in display();assert 'FINAL-DOCUMENT-MARKER' in display();assert 'INERT-ENTITY' in display()
 entity_rows=[row for row in screen.buffer.values() if 'INERT-ENTITY' in ''.join(row[x].data for x in range(screen.columns))];assert not any(c.fg=='red' for row in entity_rows for c in row.values())
 find('**SOURCE-LITERAL**','literal-python');assert '# SOURCE-HEADING' in display()
 key(b'/details\r');find('1: # Display contract','raw-numbered-document');key(b'\x1b')
 assert (project/'DOC.md').read_text()==document
 result={'status':'PASS','actual_provider_tools':True,'saved_context_markdown':any(row[2].startswith('Read document from saved context') for row in record()['interaction']['activities']),'markdown_headings_emphasis_table_link':True,'genuine_sha256_line_retained':True,'continuation_fence_styled_through_line_150':True,'source_markdown_characters_literal':True,'raw_numbered_details_retained':True,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()};(r/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result),flush=True)
finally:
 try:save('last')
 finally:
  if proc.poll() is None:
   key(b'\x11',.1)
   try:proc.wait(timeout=12)
   except subprocess.TimeoutExpired:proc.terminate();proc.wait(timeout=12)
  os.close(m);(r/'terminal.ansi').write_bytes(raw)
  ip=r/'state/peritus/daemon/daemon.instance'
  if ip.exists():
   ident=dict(line.split('=',1) for line in ip.read_text().splitlines());sockets=[line.split()[4] for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines() if f"pid={ident['pid']}," in line]
   if sockets:subprocess.run([str(binary),'--endpoint',sockets[0],'shutdown','--wait'],capture_output=True,timeout=40)

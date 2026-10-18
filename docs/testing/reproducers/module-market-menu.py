#!/usr/bin/env python3
"""Fresh actual TUI client: invoke generic native action metadata on an already checked report."""
import argparse,json,os,time
from pathlib import Path
import pexpect,pyte
ap=argparse.ArgumentParser();ap.add_argument('binary',type=Path);ap.add_argument('campaign',type=Path);args=ap.parse_args()
b=args.campaign.resolve();root=b/'instance';env={**os.environ,**{f'XDG_{k}_HOME':str(root/v)for k,v in [('CONFIG','config'),('STATE','state'),('CACHE','cache'),('DATA','data')]}}
runs=list((root/'state/peritus/daemon/workbench-v1/runs').glob('*.json'));assert len(runs)==1
record=runs[0];run=record.stem;screen=pyte.Screen(190,52);stream=pyte.Stream(screen);steps=[]
p=pexpect.spawn(str(args.binary.resolve()),['open',str(b/'workspace'),'--run',run],env=env,encoding='utf-8',dimensions=(52,190),timeout=1);raw=(b/'tui-menu.ansi').open('w')
def pump():
 try:
  text=p.read_nonblocking(65536,timeout=.2);raw.write(text);raw.flush();stream.feed(text)
  if '\x1b[6n'in text:p.send('\x1b[1;1R')
  if '\x1b[c'in text:p.send('\x1b[?1;2c')
 except pexpect.TIMEOUT:pass
 try:view='\n'.join(screen.display)
 except IndexError:view='\n'.join(''.join(screen.buffer[y][x].data for x in range(screen.columns))for y in range(screen.lines))
 (b/'tui-menu-screen.txt').write_text(view);return view
def wait(predicate,seconds=35):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  view=pump()
  if predicate(view):return
 raise TimeoutError(view)
try:
 wait(lambda v:'Peritus ·'in v and ('Idle'in v or 'Ready'in v),90)
 for number,argument,expected in [('1','plan','Plan digest:'),('4','show','Source-checked report')]:
  p.send('/modules\r');wait(lambda v:'Module market'in v and 'Deep Research'in v and 'Working…'not in v)
  p.send('\x1b[H');time.sleep(.1);p.send('\x1b[B');wait(lambda v:'1 View plan'in v and 'Idle — ready'in v);before=json.loads(record.read_text())['interaction']['public_input_count'];p.send(number)
  wanted='/module peritus.deep-research '+argument
  def received(view):
   value=json.loads(record.read_text());i=value['interaction'];users=[a[2]for a in i['activities']if a[1]==1]
   return users and i['public_input_count']>before and i['incorporated']>=i['public_input_count'] and users[-1]==wanted and value['phase']==11 and expected in value['summary'] and 'Idle — ready'in view and 'Module market'not in view
  wait(received);steps.append({'key':number,'command':wanted,'expected':expected})
 # Immediately affected older data-only module still uses its unchanged execution path.
 fixture=b/'legacy-module';fixture.mkdir(exist_ok=True);(fixture/'notes.txt').write_text('LEGACY RESOURCE ORACLE: indigo maple 47 — durable.\n')
 (fixture/'module.toml').write_text('schema=1\nid="zz.compat"\nversion="1.0.0"\nname="Compatibility reader"\ndescription="Existing declarative resource reader"\nresources=["notes.txt"]\n[[roles]]\nid="reader"\nmodel="writer"\ninstructions="Read notes.txt using module_resource_read. Return its exact text without extra claims. Do not run commands or edit files."\ntools=[]\n[[stages]]\nid="read"\nrole="reader"\n')
 import subprocess
 subprocess.run([str(args.binary.resolve()),'modules','install',str(fixture)],env=env,check=True,capture_output=True)
 subprocess.run([str(args.binary.resolve()),'modules','enable','zz.compat'],env=env,check=True,capture_output=True)
 command='/module zz.compat Read the declared notes.txt resource and quote its exact text.';p.send(command+'\r')
 def compatible(view):
  value=json.loads(record.read_text());users=[a[2]for a in value['interaction']['activities']if a[1]==1]
  return users and users[-1]==command and value['phase']==11 and 'LEGACY RESOURCE ORACLE: indigo maple 47 — durable.'in value['summary'] and 'Module Compatibility reader completed.'in value['summary'] and 'Idle — ready'in view
 wait(compatible,180);steps.append({'command':command,'expected':'Exact existing declarative resource reader output'})
 (b/'tui-menu-evidence.json').write_text(json.dumps({'status':'PASS','steps':steps},indent=2))
finally:
 if p.isalive():p.sendcontrol('q');p.expect(pexpect.EOF,timeout=12)
 raw.close()

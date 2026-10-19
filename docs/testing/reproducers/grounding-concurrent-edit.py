#!/usr/bin/env python3
"""Run actual ProductRunner/provider work while independently changing an observed source file."""
import argparse,hashlib,json,re,subprocess,time
from pathlib import Path
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--consumer',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args();b=args.output.resolve();b.mkdir(parents=True,exist_ok=False)
consumer=args.consumer.resolve();root=b/'workspace';root.mkdir()
(root/'hello.py').write_text('import sys\n\nname = sys.argv[1] if len(sys.argv) > 1 else "world"\nprint(f"Hello, {name}!")\n')
(root/'keep.txt').write_text('KEEP-EXTERNAL-CASE\n')
spec={'root':str(root),'trace':str(b/'private/run.trace'),'model':'gpt-6.1-sol','run':81,'workspace':82,'seed_ready':True,'ready':True,
 'task':'Implement the saved collaborative design from this conversation. Use its exact requirements and acceptance examples. If there is no saved design, help me start one instead of implementing anything.',
 'design':'# Add shouting\n\nAdd --shout to hello.py; --shout Ada must print HELLO, ADA!; no args prints Hello, world!. Preserve keep.txt and any concurrent external source comments. Use only actual CLI runs to check output. No unit tests, lint, formatting or formal checks. Keep the edit and review focused on this tiny request.'}
(b/'spec.json').write_text(json.dumps(spec))
with (b/'run.stdout').open('w') as stdout,(b/'run.stderr').open('w') as stderr:
 process=subprocess.Popen([str(consumer),'live',str(b/'spec.json')],stdout=stdout,stderr=stderr)
 receipt=b/'private/run.observations.json';source=b/'workspace/hello.py';original=hashlib.sha256(source.read_bytes()).hexdigest();injected=False
 while process.poll() is None:
  if not injected and receipt.exists():
   try:data=json.loads(receipt.read_text())
   except (OSError,ValueError):data={}
   if data.get('files',{}).get('hello.py')==original:
    with source.open('a') as target:target.write('\n# EXTERNAL-CONCURRENT-CHANGE-89123\n')
    injected=True;(b/'injection.json').write_text(json.dumps({'injected':True,'after_observed_sha256':original,'new_sha256':hashlib.sha256(source.read_bytes()).hexdigest()}))
  time.sleep(0.01)
 assert process.returncode==0,process.returncode
 assert injected,'Must observe read receipt and inject before first edit'
 result=json.loads((b/'run.stdout').read_text());assert result['settlement']=='Accepted',result
 assert '# EXTERNAL-CONCURRENT-CHANGE-89123' in source.read_text()
 for args,expected in [([], 'Hello, world!\n'),(['--shout','Ada'],'HELLO, ADA!\n')]:
  command=subprocess.run(['python3',str(source),*args],capture_output=True,text=True)
  assert command.returncode==0 and command.stdout==expected and not command.stderr,(args,command)
 assert (b/'workspace/keep.txt').read_text()=='KEEP-EXTERNAL-CASE\n'
 trace=(b/'private/run.trace').read_bytes()
 rejected = b'the target changed since its verified observation' in trace
 injected_hash=json.loads((b/'injection.json').read_text())['new_sha256'].encode()
 refreshed=bool(re.search(rb'source_sha256.{0,12}'+injected_hash,trace))
 assert rejected or refreshed,'Require host stale-edit rejection or actual refresh of the injected source before mutation'
 (b/'result.json').write_text(json.dumps({'passed':True,'injected':True,'stale_edit_rejected':rejected,'injected_source_refreshed':refreshed,'external_comment_preserved':True,'settlement':result['settlement']},indent=2))
 print((b/'result.json').read_text())

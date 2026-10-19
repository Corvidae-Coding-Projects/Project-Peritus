#!/usr/bin/env python3
"""Public context API: retain unaffected batch results and preserve assistant/tool order."""
import argparse,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--consumer',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();b=a.output.resolve();b.mkdir(parents=True,exist_ok=False);results=[]
for i,(label,edit,prefix) in enumerate([('mixed-batch-one-stale-read',True,None),('assistant-header-keeps-call-order',False,'ARCHIVED OBSERVATION: I will inspect both current files.')]):
 root=b/label/'workspace';root.mkdir(parents=True);(root/'focus.txt').write_text('ORIGINAL-FOCUS-527319\n');(root/'other.txt').write_text('UNIQUE-CURRENT-OTHER-781213\n');spec={'root':str(root),'trace':str(root.parent/'private/run.trace'),'run':120+i,'workspace':122+i,'task':'Read focus.txt and inspect other.txt. Preserve exact results of both observations.','external_edit':edit,'assistant_prefix':prefix,'capacity':200000};path=root.parent/'spec.json';path.write_text(json.dumps(spec));run=subprocess.run([str(a.consumer.resolve()),'mixed-batch',str(path)],capture_output=True,text=True,timeout=60);(root.parent/'stdout').write_text(run.stdout);(root.parent/'stderr').write_text(run.stderr);failures=[]
 if run.returncode:failures.append('consumer failed: '+run.stderr)
 else:
  messages=json.loads(run.stdout)['messages'];flat=[block for m in messages for block in m['blocks']];search=[block for block in flat if block['type']=='result' and block['id']=='real_mixed_1_1'];
  if len(search)!=1 or 'UNIQUE-CURRENT-OTHER-781213' not in search[0]['output']:failures.append('unaffected completed search result disappeared')
  pending=set()
  for m in messages:
   if m['role']=='Assistant' and pending:failures.append('new assistant appeared before pending batch results: '+','.join(sorted(pending)))
   for block in m['blocks']:
    if block['type']=='call':pending.add(block['id'])
    elif block['type']=='result':
     if block['id'] not in pending:failures.append('tool result appeared before its call: '+block['id'])
     else:pending.remove(block['id'])
  if pending:failures.append('tool calls lost settled results: '+','.join(sorted(pending)))
 results.append({'case':label,'passed':not failures,'failures':failures,'exit':run.returncode})
(b/'result.json').write_text(json.dumps(results,indent=2));print(json.dumps(results,indent=2));raise SystemExit(0 if all(r['passed'] for r in results) else 1)

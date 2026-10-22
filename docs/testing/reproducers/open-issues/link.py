#!/usr/bin/env python3
"""Link ordinary consumers to the exact libraries emitted by an actual product build."""
import argparse, json, subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--build',type=Path,required=True);p.add_argument('--source',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
artifacts={}
for line in a.build.read_text().splitlines():
    record=json.loads(line)
    if record.get('reason')=='compiler-artifact':
        for name in record['filenames']:
            if name.endswith('.rlib'):artifacts[record['target']['name']]=name
deps=Path(artifacts['peritus_daemon']).parent
if deps.name!='deps':deps=deps/'deps'
command=['rustc','--edition=2024','-C','debuginfo=0',str(a.source),'-L','dependency='+str(deps),'-o',str(a.output)]
for name in ('peritus_app_client','peritus_app_protocol','peritus_types','peritus_product_state','peritus_launcher','peritus_tui','peritus_provider_core','peritus_provider_opencode','peritus_provider_openai','peritus_model_protocol','serde_json','tokio'):
    if name in artifacts:command+=['--extern',name+'='+artifacts[name]]
for native in deps.parent.glob('build/*/out'):
    if any(native.glob('*.a')):command+=['-L','native='+str(native)]
subprocess.run(command,check=True)

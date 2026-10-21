#!/usr/bin/env python3
"""Link an ordinary consumer against one actual Cargo artifact cohort; no test doubles."""
import argparse,json,os,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--target',type=Path,required=True);p.add_argument('--cargo-output',type=Path);a=p.parse_args()
a.output.mkdir(parents=True,exist_ok=True)
repo=Path(__file__).resolve().parents[4]
records=a.cargo_output or a.output/'build.jsonl'
if not a.cargo_output:
 env=os.environ.copy();env.update(CARGO_TARGET_DIR=str(a.target.resolve()),CARGO_BUILD_JOBS='1',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_DEV_OPT_LEVEL='0')
 with records.open('w') as log:
  subprocess.run(['cargo','build','--offline','--locked','-p','peritus-daemon','-p','peritus-launcher','--message-format=json'],cwd=repo,env=env,stdout=log,check=True)
artifacts={}
for line in records.open():
 d=json.loads(line)
 if d.get('reason')=='compiler-artifact':
  for f in d['filenames']:
   if f.endswith('.rlib'):artifacts[d['target']['name']]=f
libraries=['peritus_daemon','peritus_agent','peritus_product_runner','peritus_process','peritus_role','peritus_provider_core','peritus_provider_openai','peritus_model_protocol','peritus_types','serde_json','tokio']
deps=Path(artifacts['peritus_product_runner']).parent
if deps.name!='deps':deps=deps/'deps'
command=['rustc','--edition=2024','-C','debuginfo=0',str(Path(__file__).with_name('consumer.rs')),'-L','dependency='+str(deps),'-o',str(a.output/'consumer')]
for name in libraries:command.extend(['--extern',name+'='+artifacts[name]])
for native in deps.parent.glob('build/*/out'):
 if any(native.glob('*.a')):command.extend(['-L','native='+str(native)])
subprocess.run(command,cwd=repo,check=True)

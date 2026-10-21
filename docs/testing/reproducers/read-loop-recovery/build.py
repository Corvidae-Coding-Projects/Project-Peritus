#!/usr/bin/env python3
"""Build and link an ordinary consumer of the actual Cargo artifact cohort; no test doubles."""
import argparse,json,os,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--target',type=Path,required=True);p.add_argument('--cargo-output',type=Path);a=p.parse_args()
a.output.mkdir(parents=True,exist_ok=True)
repo=Path(__file__).resolve().parents[4]
if a.cargo_output:
 records=a.cargo_output
else:
 records=a.output/'build.jsonl'
 env=os.environ.copy();env.update(CARGO_TARGET_DIR=str(a.target.resolve()),CARGO_BUILD_JOBS='1',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_DEV_OPT_LEVEL='0')
 with records.open('w') as log:
  subprocess.run(['cargo','build','--offline','--locked','-p','peritus-product-runner','-p','peritus-provider-openai','--message-format=json'],cwd=repo,env=env,stdout=log,check=True)
artifacts={}
for line in records.read_text().splitlines():
 row=json.loads(line)
 if row.get('reason')=='compiler-artifact':
  for name in row['filenames']:
   if name.endswith('.rlib'):artifacts[row['target']['name']]=name
libraries=['peritus_agent','peritus_product_runner','peritus_process','peritus_role','peritus_provider_core','peritus_provider_openai','peritus_model_protocol','peritus_types','serde_json','tokio']
deps=Path(artifacts['peritus_product_runner']).parent
if deps.name != 'deps':deps=deps/'deps'
command=['rustc','--edition=2024','-C','debuginfo=0',str(Path(__file__).with_name('consumer.rs')),'-L','dependency='+str(deps),'-o',str(a.output/'consumer')]
for name in libraries:command.extend(['--extern',name+'='+artifacts[name]])
for native in deps.parent.glob('build/*/out'):
 if any(native.glob('*.a')):command.extend(['-L','native='+str(native)])
subprocess.run(command,cwd=repo,check=True)

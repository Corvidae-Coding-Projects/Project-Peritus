#!/usr/bin/env python3
"""Build an ordinary consumer of the current production crates, outside Cargo membership."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess

p=argparse.ArgumentParser();p.add_argument('output',type=Path);a=p.parse_args()
repo=Path(__file__).resolve().parents[3];out=a.output.resolve();(out/'src').mkdir(parents=True,exist_ok=True)
shutil.copyfile(Path(__file__).with_name('grounding-efficiency.rs'),out/'src/main.rs')
manifest='''[package]
name="peritus-real-grounding-efficiency"
version="0.1.0"
edition="2024"
[workspace]
[dependencies]
serde_json="=1.0.149"
tokio={version="=1.53.1",features=["rt-multi-thread","macros"]}
'''
for package,directory in {
 'peritus-agent':'crates/orchestration/peritus-agent',
 'peritus-product-runner':'crates/app/peritus-product-runner',
 'peritus-provider-core':'crates/model/peritus-provider-core',
 'peritus-provider-openai':'crates/model/peritus-provider-openai',
 'peritus-model-protocol':'crates/model/peritus-model-protocol',
 'peritus-process':'crates/state/peritus-process',
 'peritus-role':'crates/orchestration/peritus-role',
 'peritus-types':'crates/foundation/peritus-types',
}.items():
 if not (repo/directory).exists():
  matches=list((repo/'crates').glob(f'*/{package}'))
  assert len(matches)==1,(package,matches)
  directory=str(matches[0].relative_to(repo))
 manifest+=f'{package}={{path={json.dumps(str(repo/directory))}}}\n'
manifest+='''[profile.release]
codegen-units=1
incremental=false
lto="thin"
overflow-checks=true
panic="abort"
strip="symbols"
'''
(out/'Cargo.toml').write_text(manifest)
subprocess.run(['cargo','generate-lockfile','--offline','--manifest-path',str(out/'Cargo.toml')],check=True,cwd=repo)
subprocess.run(['cargo','build','--locked','--offline','--release','--jobs','2','--target-dir',str(repo/'target'),
 '--manifest-path',str(out/'Cargo.toml')],check=True,cwd=repo)
shutil.copy2(repo/'target/release/peritus-real-grounding-efficiency',out/'consumer.next')
(out/'consumer.next').replace(out/'consumer')
print(out/'consumer')

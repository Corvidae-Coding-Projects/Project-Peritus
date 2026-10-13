#!/usr/bin/env python3
"""Build a public IPC consumer which reads the actual evaluation input queue."""
import argparse,json,os,shutil,subprocess,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args()
repo=Path(__file__).resolve().parents[4];out=a.output.resolve()
if out.exists():raise SystemExit('Use a fresh output directory.')
(out/'src').mkdir(parents=True);shutil.copyfile(Path(__file__).with_name('observer.rs'),out/'src/main.rs');shutil.copyfile(repo/'Cargo.lock',out/'Cargo.lock')
shutil.copyfile(Path(__file__).with_name('plan.rs'),out/'src/plan.rs')
w=tomllib.loads((repo/'Cargo.toml').read_text());manifest='[package]\nname="peritus-rsi-observer"\nversion="0.1.0"\nedition="2024"\n[dependencies]\n'
for name,path in [('peritus-app-client','crates/app/peritus-app-client'),('peritus-app-protocol','crates/app/peritus-app-protocol'),('peritus-types','crates/foundation/peritus-types'),('peritus-gates','crates/orchestration/peritus-gates')]:manifest+=f'{name}={{path={json.dumps(str(repo/path))}}}\n'
manifest+=f'tokio={{version={json.dumps(w["workspace"]["dependencies"]["tokio"]["version"])},features=["macros","rt-multi-thread"]}}\n[[bin]]\nname="peritus-rsi-plan"\npath="src/plan.rs"\n[profile.release]\n'
for k,v in w['profile']['release'].items():manifest+=f'{k}={json.dumps(v)}\n'
(out/'Cargo.toml').write_text(manifest);subprocess.run(['cargo','build','--offline','--release','-j','2','--manifest-path',str(out/'Cargo.toml')],env=dict(os.environ,CARGO_TARGET_DIR=str(repo/'target')),check=True)
print(repo/'target/release/peritus-rsi-observer')

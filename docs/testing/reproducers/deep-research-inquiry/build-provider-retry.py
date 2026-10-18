#!/usr/bin/env python3
"""Build the actual-provider retry consumer outside Cargo workspace membership."""
import argparse,json,shutil,subprocess
from pathlib import Path

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('output',type=Path)
parser.add_argument('--repo',type=Path,default=Path(__file__).resolve().parents[4])
args=parser.parse_args();repo=args.repo.resolve();out=args.output.resolve()
out.mkdir(parents=True,exist_ok=False);(out/'src').mkdir()
shutil.copyfile(Path(__file__).with_name('provider-retry.rs'),out/'src/main.rs')
packages={'peritus-agent':'crates/orchestration/peritus-agent',
 'peritus-model-protocol':'crates/model/peritus-model-protocol',
 'peritus-provider-core':'crates/model/peritus-provider-core',
 'peritus-provider-openai':'crates/model/peritus-provider-openai',
 'peritus-types':'crates/foundation/peritus-types'}
manifest='[package]\nname="peritus-real-provider-retry"\nversion="0.1.0"\nedition="2024"\n[workspace]\n[dependencies]\nserde_json="=1.0.149"\ntokio={version="=1.53.1",features=["rt-multi-thread","macros"]}\n'
for package,relative in packages.items():manifest+=f'{package}={{path={json.dumps(str(repo/relative))}}}\n'
manifest+='[profile.release]\ncodegen-units=1\nincremental=false\nlto="thin"\noverflow-checks=true\npanic="abort"\nstrip="symbols"\n'
(out/'Cargo.toml').write_text(manifest)
subprocess.run(['cargo','generate-lockfile','--offline','--manifest-path',str(out/'Cargo.toml')],cwd=repo,check=True)
subprocess.run(['cargo','build','--offline','--locked','--release','--jobs','2','--target-dir',str(repo/'target'),'--manifest-path',str(out/'Cargo.toml')],cwd=repo,check=True)

#!/usr/bin/env python3
"""Build the actual public-API consumer with bounded Cargo workers, outside the checkout."""
import argparse,json,shutil,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--repo',type=Path,default=Path(__file__).resolve().parents[4]);a=p.parse_args()
root=a.output.resolve();root.mkdir(mode=0o700,parents=True,exist_ok=False);(root/'src').mkdir();repo=a.repo.resolve()
shutil.copy2(Path(__file__).with_name('consumer.rs'),root/'src/main.rs')
manifest='[package]\nname = "peritus-binary-evidence-consumer"\nversion = "0.1.0"\nedition = "2024"\n[workspace]\n[dependencies]\n'
for name,relative in [('peritus-product-runner','crates/app/peritus-product-runner'),('peritus-model-protocol','crates/model/peritus-model-protocol')]:
 manifest+=f'{name} = {{ path = {json.dumps(str(repo/relative))} }}\n'
manifest+='serde_json = "=1.0.149"\nsha2 = { version = "=0.11.0", default-features = false }\n'
(root/'Cargo.toml').write_text(manifest)
subprocess.run(['cargo','generate-lockfile','--offline','--manifest-path',str(root/'Cargo.toml')],cwd=repo,check=True)
subprocess.run(['cargo','build','--locked','--offline','--jobs','2','--target-dir',str(root/'target'),'--manifest-path',str(root/'Cargo.toml')],cwd=repo,check=True)
print(root/'target/debug/peritus-binary-evidence-consumer')

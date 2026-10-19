#!/usr/bin/env python3
"""Link ordinary real-API consumers against one already-built Cargo dependency cohort."""
import argparse, subprocess
from pathlib import Path
from cohort import select
p=argparse.ArgumentParser();p.add_argument('--deps',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True);here=Path(__file__).resolve().parent
common=['peritus_model_protocol','peritus_provider_core','peritus_provider_openai','peritus_types','serde_json','tokio']
consumers={'scope-consumer':['peritus_product_runner','serde_json'], 'context-consumer':common+['peritus_agent','peritus_product_runner','peritus_process','peritus_role'], 'checkpoint-probe':['peritus_app_client','peritus_app_protocol','peritus_types','serde_json','tokio'], 'provider-consumer':common, 'provider-recovery-consumer':common, 'process-consumer':['peritus_provider_core','serde_json','tokio']}
for name,dependencies in consumers.items():
 command=['rustc','--edition=2024','-O',str(here/(name+'.rs')),'-L','dependency='+str(a.deps),'-o',str(a.output/name)]
 for library,rlib in select(a.deps,dependencies).items():command.extend(['--extern',library+'='+str(rlib)])
 for native in sorted(a.deps.parent.glob('build/*/out')):
  if any(native.glob('*.a')):command.extend(['-L','native='+str(native)])
 subprocess.run(command,check=True)

#!/usr/bin/env python3
"""Link a real OAuth consumer against the freshly built production dependency cohort."""
import argparse, subprocess, sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent.parent/'checkpoint-cache'))
from cohort import select
p=argparse.ArgumentParser();p.add_argument('--deps',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
libs=['peritus_provider_openai','peritus_provider_core','peritus_model_protocol','peritus_types','tokio']
command=['rustc','--edition=2024','-C','debuginfo=0',str(Path(__file__).with_name('consumer.rs')),'-L','dependency='+str(a.deps),'-o',str(a.output)]
for name,rlib in select(a.deps,libs,anchor='peritus_daemon').items():command.extend(['--extern',name+'='+str(rlib)])
for native in a.deps.parent.glob('build/*/out'):
 if any(native.glob('*.a')):command.extend(['-L','native='+str(native)])
subprocess.run(command,check=True)

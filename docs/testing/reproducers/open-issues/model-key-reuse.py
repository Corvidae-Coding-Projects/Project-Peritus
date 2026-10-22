#!/usr/bin/env python3
"""Operate actual provider settings, changing models with absent OS store and no key entry."""
import argparse, json, os, subprocess, tempfile
from pathlib import Path
from terminal import Terminal
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--seed-consumer',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
results=[]
with tempfile.TemporaryDirectory(prefix='p151keys-') as directory:
    for index,kind in enumerate(['open-code-go','open-code-zen','open-router','compatible-endpoint','anthropic-api','google-gemini-api']):
        root=Path(directory)/str(index)
        env=os.environ|{f'XDG_{k}_HOME':str(root/v) for k,v in [('CONFIG','c'),('STATE','s'),('CACHE','k')]}
        env.update(DBUS_SESSION_BUS_ADDRESS='unix:path='+str(root/'absent-bus'),PERITUS_INSTALL_DEPS='0',TERM='xterm-256color',TOKIO_WORKER_THREADS='2')
        seeded=json.loads(subprocess.check_output([str(a.seed_consumer),'seed',json.dumps([kind])],env=env,text=True))
        old=seeded['state']['providers']['direct_profiles'][0]
        terminal=Terminal([a.binary,'providers'],env,a.output/kind)
        try:
            terminal.wait(lambda text:'Providers (comma-separated' in text,'selection')
            terminal.send(b'\r')
            terminal.wait(lambda text:'m to change model using saved key' in text,'model-action')
            terminal.command('m')
            terminal.wait(lambda text:'Choose a model number' in text,'model-choice')
            expected='manual-reused-key-'+str(index)
            terminal.command('manual '+expected)
            terminal.wait(lambda text:'Reviewer provider [' in text,'reviewer')
            terminal.send(b'\r')
            terminal.wait(lambda text:'Provider settings saved.' in text,'saved')
            terminal.process.wait(timeout=10);assert terminal.process.returncode==0
            assert b'Paste the API key' not in terminal.raw and b'Credential captured' not in terminal.raw
            actual=json.loads(max((root/'s/peritus/product-state').glob('state-*.json')).read_text())['providers']['direct_profiles'][0]
            prior=dict(old);prior['model']=expected
            assert actual==prior,(kind,actual,prior)
            results.append({'provider':kind,'model':expected,'saved_key_reference_unchanged':True,'no_key_prompt':True,'passed':True})
        finally:terminal.close(False)
(a.output/'result.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps({'passed':True,'providers':len(results),'no_credentials_requested':True}))

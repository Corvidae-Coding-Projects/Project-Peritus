from pathlib import Path
import tempfile,subprocess,json,os
import argparse
ap=argparse.ArgumentParser();ap.add_argument('--binary',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
base=args.output.resolve();base.mkdir(parents=True,exist_ok=False)
env=dict(os.environ,XDG_DATA_HOME=str(base/'data'))
bin=args.binary.resolve();cases=[]
def run(*args,ok=True):
 r=subprocess.run([str(bin),'modules',*args],env=env,text=True,capture_output=True);assert (r.returncode==0)==ok,(args,r.stdout,r.stderr);cases.append((args,r.returncode));return json.loads(r.stdout) if ok else r.stderr
assert run('list')['modules'][0]['bundled']
run('disable','peritus.source-brief');assert not run('list')['modules'][0]['enabled'];run('enable','peritus.source-brief')
run('remove','peritus.source-brief',ok=False);run('show','../../x',ok=False);run('bogus',ok=False)
package=base/'example';package.mkdir(exist_ok=True)
text='''schema=1
id="local.example"
version="1.0.0"
name="Example"
description="Fixture"
resources=["notes.txt"]
[[roles]]
id="reader"
model="writer"
instructions="Read notes.txt with module_resource_read and return its exact content."
tools=[]
[[stages]]
id="read"
role="reader"
'''
(package/'module.toml').write_text(text);(package/'notes.txt').write_text('Expected resource: café 🦀\n')
d=run('install',str(package));assert not d['enabled'];run('enable','local.example');assert run('show','local.example')['digest']==d['digest']
(package/'notes.txt').write_text('Changed resource\n');d2=run('install',str(package));assert d2['digest']!=d['digest'];assert not d2['enabled'];run('enable','local.example')
installed=Path(d2['directory']);(installed/'notes.txt').write_text('corrupt');run('show','local.example',ok=False);assert any(m.get('error') for m in run('list')['modules'])
repair=run('install',str(package));assert repair['digest']==d2['digest'] and not repair['enabled'];assert Path(repair['quarantine'],'notes.txt').read_text()=='corrupt';assert run('show','local.example')['digest']==d2['digest']
run('remove','local.example');assert len(run('list')['modules'])==1
text=text.replace('1.0.0','1.0.1');(package/'module.toml').write_text(text);guard=run('install',str(package));run('enable','local.example')
for label,mutation in [('traversal',text.replace('notes.txt','../notes.txt')),('unknown',text+'unknown="x"\n'),('cycle',text+'inputs=["read"]\n'),('wrong-role',text.replace('role="reader"','role="missing"')),('reserved-id',text.replace('local.example','resume')),('wrong-model',text.replace('model="writer"','model="x"')),('bundled-shadow',text.replace('local.example','peritus.source-brief'))]:
 (package/'module.toml').write_text(mutation);run('install',str(package),ok=False);assert run('show','local.example')['digest']==guard['digest'];assert any(m.get('id')=='local.example' and m['enabled'] for m in run('list')['modules'])
(package/'module.toml').write_text(text);(package/'notes.txt').unlink();(package/'notes.txt').symlink_to('/etc/hostname');run('install',str(package),ok=False)

# Concurrent fresh processes must retain every independent registry update.
from concurrent.futures import ThreadPoolExecutor
roots=[]
for i in range(8):
 directory=base/f'parallel-{i}';directory.mkdir();(directory/'module.toml').write_text(text.replace('local.example',f'local.parallel-{i}').replace('resources=["notes.txt"]','resources=[]'));roots.append(directory)
with ThreadPoolExecutor(max_workers=4) as pool:list(pool.map(lambda directory:run('install',str(directory)),roots))
with ThreadPoolExecutor(max_workers=4) as pool:list(pool.map(lambda i:run('enable',f'local.parallel-{i}'),range(8)))
entries=run('list')['modules'];assert all(any(m['id']==f'local.parallel-{i}' and m['enabled'] for m in entries) for i in range(8))
# Invalid XDG base paths are ignored rather than writing relative to the checkout.
for value in ['', 'relative-data']:
 isolated=dict(env,HOME=str(base/'fallback-home'),XDG_DATA_HOME=value)
 result=subprocess.run([str(bin),'modules','list'],env=isolated,text=True,capture_output=True,check=True);data=json.loads(result.stdout)
 assert Path(data['directory'])==base/'fallback-home/.local/share/peritus/modules',data
 cases.append((['relative-xdg-fallback',value],result.returncode))

(base/'evidence.json').write_text(json.dumps(cases,indent=2));print(len(cases),'real CLI registry operations passed')

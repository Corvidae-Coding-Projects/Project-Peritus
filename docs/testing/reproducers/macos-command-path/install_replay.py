#!/usr/bin/env python3
"""Actual shell installers and repair command in a mount-isolated user home.

Uses real Linux binaries on Linux; the macOS adapter's filesystem operations
are portable, but this does not claim execution of the published Mach-O binary.
The published Apple-silicon archive is separately inspected with its checksum.
HOME is unchanged: bubblewrap redirects that path only inside its namespace.
"""
import argparse, hashlib, json, os, shutil, subprocess, tarfile, tomllib
from pathlib import Path

p=argparse.ArgumentParser()
p.add_argument('--repo',type=Path,required=True)
p.add_argument('--root',type=Path,required=True)
a=p.parse_args();repo=a.repo.resolve();root=a.root.resolve()
root.mkdir(parents=True,exist_ok=True)
(root/'tmp').mkdir(exist_ok=True)
release=tomllib.loads((repo/'Cargo.toml').read_text())['workspace']['package']['version']
version='v'+release

def stage(platform):
 bundle=root/'bundles'/f'peritus-{platform}-x86_64'
 for part in ['bin','libexec','share/peritus']:(bundle/part).mkdir(parents=True,exist_ok=True)
 for name in ['peritus','peritusd','peritus-tui']:shutil.copy2(repo/'target/release'/name,bundle/'bin'/name)
 helper='peritus-linux-sandbox-helper' if platform=='linux' else 'peritus-macos-sandbox-helper'
 source=repo/'target/release'/helper if platform=='linux' else root/'peritus-macos-aarch64/libexec'/helper
 shutil.copy2(source,bundle/'libexec'/helper)
 for name in ['Install-Peritus.sh','Upgrade-Peritus.sh','Uninstall-Peritus.sh']:
  shutil.copy2(repo/'packaging'/platform/name,bundle/name)
 share='peritus.service' if platform=='linux' else 'com.corvidae.peritus.plist.in'
 shutil.copy2(repo/'packaging'/platform/share,bundle/'share/peritus'/share)
 # Truthful fixture manifest: these are Linux consumers used for file-layout
 # replay, not a native Mac release or a claim of notarized/public assets.
 (bundle/'manifest.toml').write_text(f'schema=1\nrelease="{release}"\nplatform="linux"\narchitecture="x86_64"\n')
 files=sorted(p for p in bundle.rglob('*') if p.is_file() and p.name!='SHA256SUMS')
 (bundle/'SHA256SUMS').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(bundle))+'\n' for p in files))
 for f in [*bundle.glob('*.sh'),*(bundle/'bin').iterdir(),*(bundle/'libexec').iterdir()]:f.chmod(0o755)
 return bundle

linux=stage('linux');mac=stage('macos')
mirror=root/'mirror'/version;mirror.mkdir(parents=True,exist_ok=True)
archive=mirror/'peritus-linux-x86_64.tar.gz'
with tarfile.open(archive,'w:gz',compresslevel=1) as tar:tar.add(linux,arcname=linux.name)
(archive.with_name(archive.name+'.sha256')).write_text(hashlib.sha256(archive.read_bytes()).hexdigest()+'\n')
# Running the published bootstrap, with its documented local mirror override,
# exercises curl, checksum, extraction and real native installation/upgrade.
bootstrap=root/'published-install.sh'
assert bootstrap.is_file()
repair='''set -eu
test -x "$HOME/Library/Application Support/Peritus/bin/peritus"
test -x "$HOME/Library/Application Support/Peritus/bin/peritusd"
mkdir -p "$HOME/.local/bin"
peritus_repair=$(mktemp "$HOME/.local/bin/.peritus-repair.XXXXXX")
trap 'rm -f "$peritus_repair"' EXIT
printf '%s\\n' '#!/bin/sh' 'exec "$HOME/Library/Application Support/Peritus/bin/peritus" "$@"' > "$peritus_repair"
chmod 755 "$peritus_repair"
mv -f "$peritus_repair" "$HOME/.local/bin/peritus"
'''
(root/'repair-command.sh').write_text(repair)
script='''import codecs,hashlib,json,os,pty,select,struct,fcntl,subprocess,termios,time
from pathlib import Path
import sys
root=Path(sys.argv[1]);platform=sys.argv[2]
user_home=Path(os.environ['HOME'])
if platform=='linux':
 for stage in ['fresh','upgrade']:
  r=subprocess.run(['sh',str(root/'published-install.sh')],capture_output=True,text=True)
  (root/(platform+'-'+stage+'.log')).write_text(r.stdout+r.stderr)
  assert r.returncode==0,(stage,r.stdout,r.stderr)
 command=user_home/'.local/bin/peritus'
else:
 bundle=root/'bundles/peritus-macos-x86_64'
 r=subprocess.run(['sh',str(bundle/'Install-Peritus.sh'),str(bundle)],capture_output=True,text=True)
 (root/'mac-adapter.log').write_text(r.stdout+r.stderr)
 assert r.returncode==0,r.stderr
 command=user_home/'.local/bin/peritus'
 assert command.is_symlink()
 before=hashlib.sha256((user_home/'Library/Application Support/Peritus/bin/peritus').read_bytes()).hexdigest()
 assert not (user_home/'.local/bin/peritusd').exists()
 r=subprocess.run(['sh',str(root/'repair-command.sh')],capture_output=True,text=True)
 assert r.returncode==0,r.stderr
 assert not command.is_symlink()
 assert hashlib.sha256((user_home/'Library/Application Support/Peritus/bin/peritus').read_bytes()).hexdigest()==before
for program in [command,command.parent/'peritusd' if platform=='linux' else user_home/'Library/Application Support/Peritus/bin/peritusd']:
 r=subprocess.run([str(program),'--version'],capture_output=True,text=True)
 assert r.returncode==0,r.stderr
 (root/(platform+'-'+program.name+'-version.txt')).write_text(r.stdout)
master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',32,120,0,0))
p=subprocess.Popen([str(command),'open',str(root/'workspace')],stdin=slave,stdout=slave,stderr=slave,start_new_session=True)
raw=bytearray();answered_trust=False;answered_provider=False;ready=False
until=time.monotonic()+30
while time.monotonic()<until:
 if select.select([master],[],[],.1)[0]:
  try:data=os.read(master,65536)
  except OSError:break
  raw.extend(data)
  if b'Trust this workspace?' in raw and not answered_trust:os.write(master,b'n\\n');answered_trust=True
  if b'Providers (comma-separated numbers' in raw and not answered_provider:os.write(master,b'0\\n');answered_provider=True
  if b'\\x1b[?u' in data:os.write(master,b'\\x1b[?0u\\x1b[?1;2c')
  if b'\\x1b[6n' in data:os.write(master,b'\\x1b[1;1R')
  if b'connected to daemon' in raw:ready=True;os.write(master,b'\\x1b[113;5u');break
(root/(platform+'-startup.ansi')).write_bytes(raw)
assert ready,bytes(raw[-6000:])
p.wait(timeout=10);assert p.returncode==0
os.close(master);os.close(slave)
instance=user_home/'.local/state/peritus/daemon/daemon.instance'
i=dict(line.split('=',1) for line in instance.read_text().splitlines())
ep=str(instance.parent/(i['endpoint']+'.sock'))
if len(ep.encode())>107:ep='/tmp/peritus-'+hashlib.sha256(b'peritus/unix-runtime-endpoint/v1\\0'+ep.encode()).hexdigest()[:32]+'/daemon.sock'
r=subprocess.run([str(command),'--endpoint',ep,'shutdown','--wait'],capture_output=True,text=True,timeout=30)
(root/(platform+'-shutdown.txt')).write_text(r.stdout+r.stderr)
assert r.returncode==0 and 'shutdown clean; remaining=0' in r.stdout,r.stdout+r.stderr
print(json.dumps({'platform_adapter':platform,'connected':ready,'client_exit':p.returncode,'shutdown_exit':r.returncode,'actual_home':str(user_home)}))
'''
(root/'inside_replay.py').write_text(script)
(root/'workspace').mkdir(exist_ok=True)
results=[]
for platform in ['linux','macos']:
 fixture_home=root/(platform+'-home');fixture_home.mkdir(exist_ok=True)
 command=['bwrap','--die-with-parent','--unshare-user','--unshare-pid','--ro-bind','/','/','--bind',str(fixture_home),os.environ['HOME'],'--bind',str(root/'tmp'),'/tmp','--bind',str(root),str(root),'--proc','/proc','--dev','/dev','--chdir',str(root),'--unsetenv','XDG_CONFIG_HOME','--unsetenv','XDG_STATE_HOME','--unsetenv','XDG_CACHE_HOME','--setenv','PERITUS_INSTALL_DEPS','0','--setenv','PERITUS_VERSION',version,'--setenv','PERITUS_RELEASE_BASE_URL','file://'+str(root/'mirror'),'python3',str(root/'inside_replay.py'),str(root),platform]
 r=subprocess.run(command,capture_output=True,text=True,timeout=90)
 results.append({'adapter':platform,'exit':r.returncode,'output':r.stdout,'error':r.stderr})
 print(json.dumps(results[-1]),flush=True)
(root/'installer-results.json').write_text(json.dumps(results,indent=2)+'\n')
if any(r['exit']!=0 for r in results):raise SystemExit(1)

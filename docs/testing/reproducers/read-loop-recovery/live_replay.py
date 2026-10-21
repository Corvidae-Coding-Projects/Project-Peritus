#!/usr/bin/env python3
"""Run the real official-account pipeline, inspect historical reads, then build and operate its CLI."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--consumer',required=True,type=Path);p.add_argument('--output',required=True,type=Path);p.add_argument('--model',default='gpt-6-luna');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
w=a.output/'workspace';(w/'src').mkdir(parents=True)
manifest=b'[package]\nname = "read-loop-real-fixture"\nversion = "0.1.0"\nedition = "2024"\n'
(w/'Cargo.toml').write_bytes(manifest);(w/'src/main.rs').write_text('fn main() {\n    println!("Before");\n}\n');(w/'untouched.txt').write_text('UNTOUCHED-ORACLE\n');reference=a.output/'approved-phase.txt';reference.write_text('After\n')
task=f'''Use the real-testing skill. Do not create or run unit tests, mock suites, qualification suites, or conventional test/lint/format/build/check commands. Do not run shell commands or use the network. Host source readability and independent review may run; the external fixture owner will build and run the final CLI as the independent real-interface check. This is a small authorized edit in a disposable Rust project. Read the exact external reference `{reference}` using workspace_read for the approved word. Read `./src/main.rs` once. Use workspace_patch with path src/main.rs, old exactly `"Before"`, new exactly `"After"`, replace_all false. Use workspace_patch again with old `"After"`, new `"After"` as a deliberate no-op. Create one temporary JSON file using workspace_write containing {{"temporary":true}} followed by a newline, read it back, and remove that same temporary file with workspace_remove. Preserve Cargo.toml and untouched.txt. Final source bytes must be exactly `fn main() {{\\n    println!("After");\\n}}\\n`, interpreting the displayed escapes as newlines. Do not repeat grounding, inventory, or discovery. These operation choices are intentional acceptance actions. Read back the changed source. Give `cargo run --offline --quiet` as run_instructions for the user, but do not execute it. Finish after the approved word is implemented and the temporary file is removed.'''
spec={'root':str(w),'trace':str(a.output/'private/run.bin'),'task':task,'live':True,'converse':True,'model':a.model,'deny_network':True,'deny_process':True};spec_path=a.output/'spec.json';spec_path.write_text(json.dumps(spec))
with (a.output/'result.json').open('w') as output,(a.output/'runtime.log').open('w') as error:
 child=subprocess.Popen([str(a.consumer),str(spec_path)],stdout=output,stderr=error,start_new_session=True)
 try:code=child.wait(timeout=900)
 except subprocess.TimeoutExpired:
  os.killpg(child.pid,signal.SIGTERM);child.wait(timeout=20);raise
 if code:raise RuntimeError(f'product consumer exited {code}; inspect runtime.log')
outcome=json.loads((a.output/'result.json').read_text());assert outcome['settlement']=='Accepted',outcome
assert outcome['candidate']['changed_paths']==['src/main.rs'],outcome
assert (w/'src/main.rs').read_bytes()==b'fn main() {\n    println!("After");\n}\n'
assert (w/'Cargo.toml').read_bytes()==manifest and (w/'untouched.txt').read_bytes()==b'UNTOUCHED-ORACLE\n'
assert sorted(str(path.relative_to(w)) for path in w.rglob('*') if path.is_file())==['Cargo.toml','src/main.rs','untouched.txt']

# Reopen a copy of the real completed writer's durable state, without starting another invocation.
shutil.copytree(a.output/'private',a.output/'inspection-private')
inspection=dict(spec,live=False,restore_existing=True,trace=str(a.output/'inspection-private/run.bin'));inspect_path=a.output/'inspect-spec.json';inspect_path.write_text(json.dumps(inspection))
view=subprocess.run([str(a.consumer),str(inspect_path)],input='{"action":"assemble"}\n',text=True,capture_output=True,timeout=60,check=True);(a.output/'inspection.jsonl').write_text(view.stdout);(a.output/'inspection.stderr').write_text(view.stderr);assert not view.stderr
response=json.loads(view.stdout);assert response['answer']['ok'],response
results=[json.loads(block['output']) for message in response['answer']['value']['messages'] for block in message['blocks'] if block['type']=='result']
writes={value['path']:value for value in results if value.get('changed') is True and value.get('sha256') and value.get('path','').endswith('.json')}
assert writes,'no actual temporary write receipt retained'
for path,write in writes.items():
 reads=[value for value in results if value.get('path')==path and value.get('observation_time')=='before_later_settled_workspace_effect']
 assert len(reads)==1,(path,reads)
 read=reads[0];content='\n'.join(line.split(': ',1)[1] for line in read['content'].split('\n'))+'\n'
 assert json.loads(content)=={'temporary':True}
 assert hashlib.sha256(content.encode()).hexdigest()==read['source_sha256']==write['sha256']
 assert not (w/path).exists(),'deleted temporary file was recreated'
assert any(value.get('path')=='src/main.rs' and value.get('changed') is False for value in results),'no-op patch did not report changed=false'

# Independent human-equivalent operation of the generated program; this is not a test suite.
env=os.environ.copy();env.update(CARGO_BUILD_JOBS='1',CARGO_TARGET_DIR=str(a.output/'cli-build'),CARGO_PROFILE_DEV_DEBUG='0')
cli=subprocess.run(['cargo','run','--offline','--quiet','--manifest-path',str(w/'Cargo.toml')],text=True,capture_output=True,env=env,timeout=120,check=True);(a.output/'cli.stdout').write_text(cli.stdout);(a.output/'cli.stderr').write_text(cli.stderr);assert cli.stdout=='After\n' and not cli.stderr,(cli.stdout,cli.stderr)
logs=(a.output/'runtime.log').read_text();counters=re.findall(r'requests=(\d+) tools=(\d+)',logs)
result={'status':'PASS','consumer_sha256':hashlib.sha256(a.consumer.read_bytes()).hexdigest(),'settlement':outcome['settlement'],'last_provider_requests':int(counters[-1][0]),'last_tool_calls':int(counters[-1][1]),'current_source_exact':True,'noop_changed_false':True,'historical_read_paths':list(writes),'deleted_files_stay_absent':True,'real_cli_stdout':cli.stdout,'workspace_process_and_network_permissions':False};(a.output/'acceptance.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result),flush=True)

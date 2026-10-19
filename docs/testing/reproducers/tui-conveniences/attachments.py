from pathlib import Path
runner=Path(__file__).with_name('replay.py')
# Reuse only the PTY/fixture definitions, then operate the real application's attachment controls.
exec(compile(runner.read_text().split('\ntry:\n')[0],str(runner),'exec'))
record_path=next((fixture/'state/peritus/daemon/workbench-v1/runs').glob('*.json'));before=json.loads(record_path.read_text());run=before['run_id']
try:
 start(run=run);bootstrap()
 if 'Sessions ·'in text():send(b'\x1b')
 clear();paste('ATTACHMENT-DRAFT-雪')
 end=time.monotonic()+5
 while not Path(f'/tmp/.X11-unix/X{d}').exists()and time.monotonic()<end:time.sleep(.05)
 clipboard=subprocess.Popen(['xclip','-selection','clipboard','-target','image/png','-in','-quiet'],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=(root/'clipboard.log').open('wb'),env=env,start_new_session=True);clipboard.stdin.write(png());clipboard.stdin.close()
 deadline=time.monotonic()+5
 while time.monotonic()<deadline:
  readback=subprocess.run(['xclip','-selection','clipboard','-target','image/png','-out'],env=env,capture_output=True,timeout=2)
  if readback.returncode==0 and readback.stdout==png():break
  pump(.1)
 else:raise RuntimeError('The isolated clipboard fixture never became ready')
 send(b'\x16',.8);wait('Attach image');wait('Preview only');send(b't');paste('Disposable image caption');send(b'\r');send(b'c',.7);wait('Durably accepted');check('image-confirm-keeps-draft',draft()=='ATTACHMENT-DRAFT-雪');send(b'l',.5);wait('clipboard.png');check('image-retained-readback','clipboard.png'in text());send(b'\x1b')
 send(b'\x0f');paste('notes.md');wait('notes.md');pump(.3);send(b'\r');wait('Preview only');send(b't');paste('Disposable file caption');send(b'\r');send(b'c',.7);wait('Durably accepted');check('file-confirm-keeps-draft',draft()=='ATTACHMENT-DRAFT-雪');send(b'l',.5);wait('notes.md');check('file-retained-readback','notes.md'in text());send(b'\x1b')
 after=json.loads(record_path.read_text());check('attachment-confirmation-no-inference',after['progress']['model_requests']==before['progress']['model_requests'])
 close_client();(root/'results.json').write_text(json.dumps({'passed':True,'scenarios':results},indent=2));print(json.dumps({'passed':True,'checks':len(results)}))
finally:
 if master is not None:
  capture('final');send(b'\x11');child.wait(timeout=12);os.close(master)
 instance=fixture/'state/peritus/daemon/daemon.instance'
 if instance.exists():
  identity=dict(line.split('=',1)for line in instance.read_text().splitlines()if'='in line);pid=int(identity['pid'])
  for line in subprocess.check_output(['ss','-xlpnH'],text=True).splitlines():
   if f'pid={pid},'in line:subprocess.run([str(binary),'--endpoint',line.split()[4],'shutdown','--wait'],env=env,capture_output=True,timeout=35)
 if clipboard and clipboard.poll()is None:os.killpg(clipboard.pid,signal.SIGTERM);clipboard.wait(timeout=5)
 xvfb.terminate();xvfb.wait(timeout=5)
 if not(root/'results.json').exists():(root/'results.json').write_text(json.dumps({'passed':False,'scenarios':results},indent=2))

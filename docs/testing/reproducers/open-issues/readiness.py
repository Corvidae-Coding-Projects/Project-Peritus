#!/usr/bin/env python3
"""Actual daemon/IPC failure classification and native TUI retry with an unavailable OS store."""
import argparse, codecs, fcntl, json, os, pty, select, struct, subprocess, tempfile, termios, time
from pathlib import Path
import pyte
p=argparse.ArgumentParser();p.add_argument('--binary-dir',type=Path,required=True);p.add_argument('--seed-consumer',type=Path,required=True);p.add_argument('--probe',type=Path,required=True);p.add_argument('--ui',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--fixed',action='store_true');a=p.parse_args()
a.output.mkdir(parents=True,exist_ok=True)
with tempfile.TemporaryDirectory(prefix='pr149-') as temporary:
    root=Path(temporary)
    env=os.environ|{f'XDG_{k}_HOME':str(root/v) for k,v in [('CONFIG','c'),('STATE','s'),('CACHE','k')]}
    env.update(DBUS_SESSION_BUS_ADDRESS='unix:path='+str(root/'absent-bus'),PERITUS_INSTALL_DEPS='0',TERM='xterm-256color',TOKIO_WORKER_THREADS='2')
    seeded=json.loads(subprocess.check_output([str(a.seed_consumer),'seed','["open-router"]'],env=env,text=True))
    log=(a.output/'daemon.log').open('w')
    daemon=subprocess.Popen([str(a.binary_dir/'peritusd'),'serve','--config',seeded['configuration']],env=env,stdout=log,stderr=log)
    try:
        deadline=time.monotonic()+45
        while not Path(seeded['endpoint']).exists():
            assert daemon.poll() is None,(a.output/'daemon.log').read_text()
            assert time.monotonic()<deadline,'daemon readiness'
            time.sleep(.05)
        state=seeded['state'];workspace=state['workspaces']['active_workspace_id']
        probe=[str(a.probe),seeded['endpoint'],workspace,'a9000000000000000000000000000009']
        if a.fixed:probe+=['--require-fixed']
        observed=subprocess.run(probe,env=env,capture_output=True,text=True,timeout=45)
        (a.output/'admission.json').write_text(observed.stdout)
        assert observed.returncode==0,observed.stderr
        if a.ui:
            master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',42,140,0,0))
            client=subprocess.Popen([str(a.ui)],env=env,stdin=slave,stdout=slave,stderr=slave);os.close(slave)
            screen=pyte.Screen(140,42);stream=pyte.Stream(screen);decoder=codecs.getincrementaldecoder('utf-8')('replace');raw=bytearray()
            def pump(seconds):
                until=time.monotonic()+seconds
                while time.monotonic()<until:
                    if select.select([master],[],[],.05)[0]:
                        try:data=os.read(master,65536)
                        except OSError:return
                        raw.extend(data);stream.feed(decoder.decode(data))
                        if b'\x1b[6n' in data:os.write(master,b'\x1b[1;1R')
                        if b'\x1b[?u' in data:os.write(master,b'\x1b[?1;2c')
            def wait_for(predicate,label):
                deadline=time.monotonic()+40
                while time.monotonic()<deadline:
                    pump(.1)
                    text='\n'.join(screen.display)
                    if predicate(text):
                        (a.output/(label+'.screen')).write_text(text)
                        return text
                    assert client.poll() is None,text
                raise AssertionError(label+'\n'+text)
            try:
                wait_for(lambda text:'Enter sends' in text,'ready')
                os.write(master,b'Return exactly READY.\r')
                failure=wait_for(lambda text:'credential' in text.lower() and ('Error' in text or 'Stopped' in text),'provider-error')
                assert 'Create durable deliverable handoff failed' not in failure
                os.write(master,b'/retry\r');pump(2)
                retry=wait_for(lambda text:'credential' in text.lower() and ('Error' in text or 'Stopped' in text),'retry-error')
                assert 'Create durable deliverable handoff failed' not in retry
                os.write(master,b'/model\r')
                wait_for(lambda text:'Model' in text and ('Refresh' in text or 'refresh' in text),'recovery-control')
                os.write(master,b'\x1b');pump(.2);os.write(master,b'\x11');client.wait(timeout=10)
                assert client.returncode==0
            finally:
                (a.output/'terminal.ansi').write_bytes(raw)
                if client.poll() is None:client.terminate();client.wait(timeout=5)
                os.close(master)
        print(json.dumps({'passed':True,'fixed':a.fixed,'native_tui':bool(a.ui),'admission_cases':4}))
    finally:
        if daemon.poll() is None:
            subprocess.run([str(a.binary_dir/'peritus'),'--endpoint',seeded['endpoint'],'shutdown','--wait'],env=env,capture_output=True,timeout=30)
        if daemon.poll() is None:daemon.terminate()
        daemon.wait(timeout=10);log.close()

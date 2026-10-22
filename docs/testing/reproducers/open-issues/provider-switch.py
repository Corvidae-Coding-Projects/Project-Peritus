#!/usr/bin/env python3
"""Real old-provider removal, retained native TUI recovery, restart and official-account retry."""
import argparse, json, os, subprocess, tempfile, time
from pathlib import Path
from terminal import Terminal
p=argparse.ArgumentParser();p.add_argument('--binary-dir',type=Path,required=True);p.add_argument('--seed-consumer',type=Path,required=True);p.add_argument('--ui',type=Path,required=True);p.add_argument('--old-ui',type=Path,required=True);p.add_argument('--probe',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
marker='RETAINED-RUN-PROVIDER-RECOVERY-153791'
with tempfile.TemporaryDirectory(prefix='p151-') as directory:
    root=Path(directory)
    env=os.environ|{f'XDG_{k}_HOME':str(root/v) for k,v in [('CONFIG','c'),('STATE','s'),('CACHE','k')]}
    env.update(DBUS_SESSION_BUS_ADDRESS='unix:path='+str(root/'absent-bus'),PERITUS_INSTALL_DEPS='0',PERITUS_REAL_ACCOUNT_MODEL='gpt-6.1-sol',TERM='xterm-256color',TOKIO_WORKER_THREADS='2')
    daemon=None;log=None;ui=None
    def seed(kinds):return json.loads(subprocess.check_output([str(a.seed_consumer),'seed',json.dumps(kinds)],env=env,text=True))
    def stop():
        global daemon,log
        if daemon and daemon.poll() is None:
            result=subprocess.run([str(a.binary_dir/'peritus'),'--endpoint',seeded['endpoint'],'shutdown','--wait'],env=env,capture_output=True,text=True,timeout=30)
            assert result.returncode==0,result.stderr
            daemon.wait(timeout=10)
        if log:log.close()
        daemon=None;log=None
    def start(label):
        global daemon,log
        log=(a.output/(label+'.log')).open('w')
        daemon=subprocess.Popen([str(a.binary_dir/'peritusd'),'serve','--config',seeded['configuration']],env=env,stdout=log,stderr=log)
        deadline=time.monotonic()+45
        while not Path(seeded['endpoint']).exists():
            assert daemon.poll() is None,(a.output/(label+'.log')).read_text()
            assert time.monotonic()<deadline,'readiness';time.sleep(.05)
        deadline=time.monotonic()+10
        while time.monotonic()<deadline:
            result=subprocess.run([str(a.binary_dir/'peritus'),'--endpoint',seeded['endpoint'],'--json','status'],env=env,capture_output=True,text=True,timeout=5)
            if result.returncode==0:return
            time.sleep(.05)
        raise AssertionError(result.stderr)
    def inspect(run,mode=None):
        command=[str(a.probe),seeded['endpoint'],run]+([mode] if mode else [])
        result=subprocess.run(command,env=env,capture_output=True,text=True,timeout=30)
        assert result.returncode==0,result.stderr
        return json.loads(result.stdout)
    try:
        seeded=seed(['open-code-go']);start('old-route')
        ui=Terminal([a.old_ui],env,a.output/'initial')
        ui.wait(lambda text:'Enter sends' in text,'ready')
        ui.command('This is a discussion-only request. Use no tools. Return exactly '+marker+'.')
        ui.wait(lambda text:'credential' in text.lower() and ('Error' in text or 'Stopped' in text),'old-provider-failed')
        runs=json.loads(subprocess.check_output([str(a.binary_dir/'peritus'),'--endpoint',seeded['endpoint'],'--json','runs','list'],env=env,text=True))
        # IDs come from the actual public CLI observation; the fixture has exactly one run.
        def find_run(value):
            if isinstance(value,dict):
                for key,item in value.items():
                    if key in ('run_id','runId','run') and isinstance(item,str) and len(item)==32:return item
                    result=find_run(item)
                    if result:return result
            if isinstance(value,list):
                for item in value:
                    result=find_run(item)
                    if result:return result
        run=find_run(runs);assert run,runs
        before=inspect(run);assert before['providers']==['a8000000000000000000000000000008']*3
        ui.close();ui=None;stop()
        seeded=seed(['codex-account','open-code-zen']);start('new-route')
        ui=Terminal([a.old_ui,'--resume'],env,a.output/'baseline')
        ui.wait(lambda text:'No available configured provider' in text,'stranded-run')
        ui.command('/provider')
        ui.wait(lambda text:'unknown' in text.lower() or 'Unknown' in text,'missing-control')
        ui.close();ui=None
        ui=Terminal([a.ui,'--resume'],env,a.output/'select')
        ui.wait(lambda text:'No available configured provider' in text,'retained-run')
        ui.command('/provider')
        ui.wait(lambda text:'Provider for all roles' in text and 'OpenAI with ChatGPT' in text,'picker')
        ui.send(b'\r')
        ui.wait(lambda text:'Provider and model selections saved' in text,'saved')
        selected=inspect(run);assert selected['providers']==['a1000000000000000000000000000001']*3
        assert selected['models']==['']*3 and selected['phase']==before['phase']
        assert selected['activities'][:len(before['activities'])]==before['activities']
        inspect(run,'reject');inspect(run,'efforts')
        for action, expected in [
            ('/provider reviewer 2',['a1000000000000000000000000000001','a7000000000000000000000000000007','a1000000000000000000000000000001']),
            ('/provider fixer 2',['a1000000000000000000000000000001','a7000000000000000000000000000007','a7000000000000000000000000000007']),
            ('/provider 1',['a1000000000000000000000000000001']*3),
        ]:
            ui.command(action)
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                ui.pump(.1)
                if inspect(run)['providers']==expected:break
            else:raise AssertionError(action+' did not save the expected roles')
            ui.pump(.2)
        inspect(run,'legacy-model')
        ui.close();ui=None;stop();start('restart')
        restarted=inspect(run);assert restarted['providers']==selected['providers']
        ui=Terminal([a.ui,'--resume'],env,a.output/'resume')
        ui.wait(lambda text:'OpenAI with ChatGPT' in text and 'Enter sends' in text,'reopened')
        ui.command('/effort low');ui.pump(1)
        ui.command('/retry')
        deadline=time.monotonic()+180
        final=None
        while time.monotonic()<deadline:
            ui.pump(.25)
            observed=inspect(run)
            if any(activity['kind']=='Assistant' and activity['text'].strip()==marker for activity in observed['activities']):
                final=observed;break
            assert 'Create durable deliverable handoff failed' not in ui.text,ui.text
        assert final,ui.text
        assert final['run']==run and final['providers']==selected['providers']
        assert not any('Create durable deliverable handoff failed' in activity['text'] for activity in final['activities'])
        ui.close();ui=None
        (a.output/'result.json').write_text(json.dumps({'passed':True,'same_run':run,'original_providers':before['providers'],'recovered_providers':final['providers'],'restart_persisted':True,'rejected_unknown_unchanged':True,'legacy_model_update':True,'actual_account_reply':marker,'final':final},indent=2)+'\n')
        print(json.dumps({'passed':True,'same_run':run,'actual_account_reply':marker,'restart_persisted':True}))
    finally:
        if ui:ui.close()
        stop()

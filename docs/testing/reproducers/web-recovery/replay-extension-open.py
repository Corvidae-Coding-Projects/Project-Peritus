#!/usr/bin/env python3
"""Replay real extension open/share diagnostics in an owned Chromium profile."""
import argparse, contextlib, http.server, json, os, pathlib, shutil, subprocess, sys, tempfile, threading, time
from playwright.sync_api import sync_playwright

p=argparse.ArgumentParser(description=__doc__);p.add_argument('--expect-old',action='store_true');p.add_argument('--cli',type=pathlib.Path,required=True);p.add_argument('--driver-dir',type=pathlib.Path,required=True);p.add_argument('--browser',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args()
root=a.output.resolve();root.mkdir(mode=0o700,parents=True,exist_ok=False)
env=dict(os.environ,HOME=str(root/'home'))
for name in ('config','state','cache','data'):env['XDG_'+name.upper()+'_HOME']=str(root/name)
(root/'home').mkdir()
r=subprocess.run([str(a.cli.resolve()),'automation','chrome-extension','install'],env=env,capture_output=True,text=True,check=True);installed=json.loads(r.stdout)
profile=root/'profile';(profile/'NativeMessagingHosts').mkdir(parents=True)
shutil.copy2(installed['registrations'][0],profile/'NativeMessagingHosts/com.corvidae.peritus.json')
env['PERITUS_CHROME_EXTENSION_SOCKET']=str(root/'data/peritus/automation/chrome-extension/native.sock')
env['PERITUS_CHROME_EXTENSION_TOKEN_FILE']=str(root/'data/peritus/automation/chrome-extension/bridge-token')
os.environ.update({k:v for k,v in env.items() if k.startswith('PERITUS_CHROME_EXTENSION_')})
sys.path.insert(0,str(a.driver_dir.resolve()))
from extension_browser import ExtensionBrowsers
bridge=ExtensionBrowsers(root,lambda *args:None)
display=next(n for n in range(180,230) if not pathlib.Path(f'/tmp/.X11-unix/X{n}').exists())
xvfb=subprocess.Popen(['Xvfb',f':{display}','-screen','0','1280x900x24','-nolisten','tcp'],stdout=subprocess.DEVNULL,stderr=(root/'xvfb.log').open('w'))
env.pop('WAYLAND_DISPLAY',None);env['DISPLAY']=f':{display}'
class Page(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body=b'<title>Actual bridge regression</title><p>Loopback reader 9274</p>';self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Page);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
try:
    with sync_playwright() as pw:
        context=pw.chromium.launch_persistent_context(str(profile),executable_path=str(a.browser.resolve()),headless=False,env=env,ignore_default_args=['--disable-extensions'],args=[f'--disable-extensions-except={installed["unpacked_directory"]}',f'--load-extension={installed["unpacked_directory"]}','--ozone-platform=x11','--no-first-run','--no-default-browser-check'],chromium_sandbox=True)
        try:
            worker=context.service_workers[0] if context.service_workers else context.wait_for_event('serviceworker')
            page=context.pages[0];end=time.monotonic()+20
            while time.monotonic()<end:
                if pathlib.Path(env['PERITUS_CHROME_EXTENSION_SOCKET']).exists():break
                page.wait_for_timeout(100)
            assert bridge.request({'action':'status'})['connected']
            try:result=bridge.control({'action':'extension_open','url':'https://doc.qt.io/','visible':False})
            except ValueError as error:
                result={'raised':str(error)}
            tabs=worker.evaluate('chrome.tabs.query({}).then(tabs=>tabs.map(t=>({url:t.url})))')
            assert any(t['url'].startswith('https://doc.qt.io') for t in tabs),tabs
            if not a.expect_old:
                assert result['success'] is False and result['failure']=='browser_tab_selection_required',result
                assert 'Share this HTTPS tab' in result['next_action'],result
                assert not bridge.sessions
            else:assert result=={'raised':'Chrome extension action failed'},result
            local=bridge.control({'action':'extension_open','url':f'http://127.0.0.1:{server.server_port}/','visible':False})
            assert local['success'] and 'Loopback reader 9274' in local['text'],{k:v for k,v in local.items() if k not in ('screenshot','elements')}
            bridge.request({'action':'close','session':local['session']})
            try:bridge.request({'action':'extension_open','url':'http://example.com/'})
            except ValueError as error:assert 'HTTPS' in str(error)
            else:raise AssertionError('External HTTP accepted')
            receipt={'fixed':not a.expect_old,'open_result':result,'loopback_read':True,'external_http_rejected':True,'actual_chrome_extension':True,'passed':True}
            (root/'result.json').write_text(json.dumps(receipt,indent=2));print(json.dumps({'root':str(root),**receipt}),flush=True)
        finally:context.close()
finally:
    server.shutdown();server.server_close();thread.join();xvfb.terminate();xvfb.wait(timeout=10)

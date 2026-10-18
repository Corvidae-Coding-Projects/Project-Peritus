#!/usr/bin/env python3
"""Replay two genuine failed calls through native IPC, on a disposable real-state copy."""
import argparse, ast, copy, hashlib, json, pathlib, shutil, tempfile, time
P = argparse.ArgumentParser()
P.add_argument('--binary', type=pathlib.Path, required=True)
P.add_argument('--fixture', type=pathlib.Path, required=True)
P.add_argument('--expect-fixed', action='store_true')
P.add_argument('--version', default='1.4.0')
P.add_argument('--output', type=pathlib.Path)
A = P.parse_args()
F = A.fixture.resolve()
M = json.loads((F / 'manifest.json').read_text())
B = (A.binary or pathlib.Path(M['old_native_binary'])).resolve()
R = A.output or pathlib.Path(tempfile.mkdtemp(prefix='fixed-' if A.expect_fixed else 'before-', dir=F))
R = R.resolve()
assert not R.is_relative_to(F / 'seed-state'), 'Output must not overwrite the fixture'
R.mkdir(exist_ok=True)
S = R / 'state'
assert not S.exists(), 'Use a fresh output directory'
assert not R.is_relative_to(pathlib.Path(M['source_live_state_read_only']))
shutil.copytree(F / 'seed-state', S)
original = json.loads((F / 'evidence/failed-sequence-76-arguments.json').read_text())
corrected = json.loads((F / 'evidence/failed-sequence-78-arguments.json').read_text())
assert corrected['decision'] == 'qualified' and corrected['provenance']['relationship'] == 'unclear' and (corrected['provenance']['account'] == 'attributed_statement')
helper = pathlib.Path(__file__).resolve().parent.parent / 'native-research-rpc.py'
T = ast.parse(helper.read_text())
N = [n for n in T.body if isinstance(n, (ast.Import, ast.ImportFrom)) or (isinstance(n, ast.ClassDef) and n.name == 'Native') or (isinstance(n, ast.FunctionDef) and n.name == 'check')]
G = {'__name__': 'native_transport', 'ROOT': R, 'EXE': B, 'events': [], 'checks': 0, 'failures': []}
exec(compile(ast.Module(body=N, type_ignores=[]), str(helper), 'exec'), G)
Native = G['Native']
rpc = []
checks = []
n = None

def ck(label, ok):
    checks.append({'label': label, 'pass': bool(ok)})
    if not ok:
        raise AssertionError(label)

def initialize(native):
    parts = [int(x) for x in A.version.split('.')]
    assert len(parts) == 3
    return native.request('initialize', {'protocol_version': 1, 'plugin_id': 'peritus.deep-research', 'plugin_version': dict(zip(['major', 'minor', 'patch'], parts)), 'quotas': {'concurrent_requests': 1, 'frame_bytes': 33554432, 'output_bytes': 33554432, 'invocation_millis': 15000, 'lifecycle_requests': 10000, 'protocol_violations': 3}})

def command(native):
    o = native.call(S, 'command', text='reassess', revision=2001)
    rpc.append({'method': 'command', 'arguments': {'text': 'reassess', 'revision': 2001}, 'result': o})
    ck('actual public reassess requests assessor', o.get('kind') == 'model' and 'assess' in o.get('request', {}).get('status', ''))
    return o['request']['key']

def invoke(native, key, args):
    o = native.call(S, 'tool', key=key, name='research_admit', arguments_json=json.dumps(args, ensure_ascii=False))
    rpc.append({'method': 'tool', 'name': 'research_admit', 'arguments': args, 'result': o})
    return o
try:
    n = Native()
    init = initialize(n)
    rpc.append({'method': 'initialize', 'result': init})
    ck('native initialize ready', init == {'kind': 'status', 'body': {'status': 'ready'}})
    key = command(n)
    bad = invoke(n, key, original)
    ck('actual byte39 title mismatch remains rejected', 'error' in bad and 'admission' not in bad)
    if A.expect_fixed:
        ck('bad offset diagnosis names already observed byte38 without new read', '38' in str(bad) and 'another web_read is unnecessary' in str(bad))
    unqualified = copy.deepcopy(corrected)
    unqualified['decision'] = 'admitted'
    u = invoke(n, key, unqualified)
    ck('unqualified attributed statement remains rejected', 'error' in u and 'admission' not in u)
    good = invoke(n, key, corrected)
    if A.expect_fixed:
        ck('correct exact unclear attribution accepted', 'admission' in good and 'error' not in good)
        aid = good['admission']
        episode = json.loads((S / 'selected.json').read_text())['episode']
        ap = S / 'jobs' / episode / 'research/admissions' / f'{aid}.json'
        ad = json.loads(ap.read_text())
        ck('unclear relationship preserved without false reclassification', ad['provenance']['relationship'] == 'unclear')
        ck('attribution-only scope and qualified decision preserved', ad['decision'] == 'qualified' and ad['provenance']['account'] == 'attributed_statement' and (ad['claim'] == corrected['claim']))
        n.close()
        n = Native()
        ck('fresh native process initializes', initialize(n) == {'kind': 'status', 'body': {'status': 'ready'}})
        key = command(n)
        item = n.call(S, 'tool', key=key, name='research_item', arguments_json=json.dumps({'id': aid}))
        rpc.append({'method': 'tool', 'name': 'research_item', 'arguments': {'id': aid}, 'result': item})
        ck('fresh native process reads exact preserved attribution', item.get('assessment', {}).get('provenance', {}).get('relationship') == 'unclear' and item.get('assessment', {}).get('claim') == corrected['claim'])
    else:
        ck('v1.3.3 reproduces original relationship restriction', 'error' in good and 'First-party claims remain' in json.dumps(good))
        ck('before replay uses exact recorded native', hashlib.sha256(B.read_bytes()).hexdigest() == M['old_native_sha256'])
except Exception as e:
    checks.append({'label': 'replay_exception', 'pass': False, 'error': repr(e)})
finally:
    if n:
        try:
            n.close()
        except Exception as e:
            checks.append({'label': 'native_cleanup', 'pass': False, 'error': repr(e)})
            n.p.kill()
            n.p.wait()
    receipt = {'observed_unix': time.time(), 'status': 'PASS' if all((x['pass'] for x in checks)) and (not G['failures']) else 'FAIL', 'expect_fixed': A.expect_fixed, 'initialized_plugin_version': A.version, 'binary': str(B), 'binary_sha256': hashlib.sha256(B.read_bytes()).hexdigest(), 'copied_state': str(S), 'source_state_never_invoked': M['source_live_state_read_only'], 'checks': checks, 'helper_checks': G['checks'], 'helper_failures': G['failures'], 'provider_inference_calls': 0, 'web_calls': 0, 'scope': 'Actual native executable framed IPC; genuine retained arguments/read/candidate records. No model/provider output or HTTP observations fabricated. Reassessment only in disposable copied state.'}
    (R / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    (R / 'rpc.json').write_text(json.dumps(rpc, ensure_ascii=False, indent=2) + '\n')
    (R / 'transport-events.json').write_text(json.dumps(G['events'], indent=2) + '\n')
    print(json.dumps(receipt, indent=2))
    raise SystemExit(0 if receipt['status'] == 'PASS' else 1)

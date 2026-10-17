#!/usr/bin/env python3
"""Real built-in search with a fresh profile, no executable helpers and independent source domains."""
import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import subprocess
import time
from urllib.parse import urlparse

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--consumer', type=Path, help='Optional built real public-library consumer for engine/count exploration')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
binary = str(args.binary.resolve())
env = dict(os.environ, PATH=str(args.output / 'empty-path'))
for key in ['PERITUS_WEB_SEARCH_URL', 'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY']:
    env.pop(key, None)
for key in ['CONFIG', 'STATE', 'CACHE', 'DATA']:
    env[f'XDG_{key}_HOME'] = str(args.output / key.lower())
seen_children = set()


def run(*parts, success=True):
    start = time.monotonic()
    process = subprocess.Popen([binary, '--json', 'web', *parts], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    while process.poll() is None:
        children = Path(f'/proc/{process.pid}/task/{process.pid}/children')
        try:
            seen_children.update(children.read_text().split())
        except FileNotFoundError:
            pass
        if time.monotonic() - start > 25:
            process.kill()
            raise TimeoutError(parts)
        time.sleep(.02)
    stdout, stderr = process.communicate()
    assert (process.returncode == 0) == success, (parts, process.returncode, stdout, stderr)
    return json.loads(stdout if success else stderr)


status = run('status')
assert status['search_ready'] and status['search_backend'] == 'built-in-rust'
cases = []
queries = [
    ('Rust ownership official documentation', 'doc.rust-lang.org'),
    ('Tokio spawn Rust documentation', 'docs.rs'),
    ('Python asyncio documentation', 'docs.python.org'),
]
for index, (query, domain) in enumerate(queries):
    result = run('search', query)
    assert result['search_backend'] == 'built-in-rust' and result['results'], result
    assert result['cached'] is False
    primary = next((item for item in result['results'] if urlparse(item['url']).hostname == domain), None)
    assert primary, (query, domain, result)
    opened = run('open', primary['url'])
    assert opened['text'] and opened['source'] and urlparse(opened['url']).hostname == domain, opened
    cached = run('search', query)
    assert cached['cached'] is True and cached['results'] == result['results']
    (args.output / f'query-{index}.json').write_text(json.dumps({'query':result, 'primary':opened}, indent=2))
    cases.append({'query':query, 'primary':primary['url'], 'engine_failures':result['engine_failures'], 'empty_engines':result['empty_engines']})

# Fresh processes are independent consumers of the same on-disk result cache.
with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
    cached = list(pool.map(lambda _: run('search', queries[0][0]), range(3)))
assert all(item['cached'] for item in cached)

settings = args.output / 'data/peritus/web/settings.json'
settings.parent.mkdir(parents=True, exist_ok=True)
legacy = {'endpoint':'http://127.0.0.1:21987/search', 'managed':{'engine':'podman','name':'legacy-fixture','id':'nonexistent-disposable-identity','endpoint':'http://127.0.0.1:21987/search'}}
settings.write_text(json.dumps(legacy))
assert run('status')['search_backend'] == 'built-in-rust'
assert json.loads(settings.read_text()) == legacy # read migration leaves cleanup ownership intact
run('setup', 'http://127.0.0.1:9/search')
assert run('status')['search_backend'] == 'external-searxng'
error = run('search', 'Rust ownership', success=False)
assert 'web request' in str(error)
run('setup')
assert run('status')['search_backend'] == 'built-in-rust'
assert run('search', queries[0][0])['results']
assert not seen_children, seen_children
explored = []
if args.consumer:
    import random
    rng = random.Random(20261017)
    for index, (engines, count) in enumerate([('brave',1),('startpage',7),('brave,startpage',30),('startpage,brave',7),('brave,brave',7),('yahoo,brave,startpage',30)]):
        query, _ = rng.choice(queries)
        request = {'query':query, 'engines':engines, 'max_results':count}
        completed = subprocess.run([str(args.consumer.resolve()),str(args.output / f'explore-{index}'),json.dumps(request)],env=env,capture_output=True,text=True,timeout=25)
        assert completed.returncode == 0, (request, completed.stderr)
        result = json.loads(completed.stdout)
        assert 1 <= len(result['results']) <= count
        assert len({item['url'] for item in result['results']}) == len(result['results'])
        assert all(urlparse(item['url']).scheme in ('http','https') and item['title'] for item in result['results'])
        explored.append({'request':request,'result':result})
    (args.output / 'exploration.json').write_text(json.dumps({'seed':20261017,'cases':explored},indent=2))
summary = {'status':'PASS', 'live_queries':cases, 'fresh_process_cache_consumers':3, 'legacy_migration':True, 'external_failure_then_native_recovery':True, 'observed_child_pids':sorted(seen_children), 'empty_path':env['PATH'], 'explored_engine_count_cases':len(explored)}
(args.output / 'summary.json').write_text(json.dumps(summary, indent=2))
print(json.dumps(summary, indent=2))

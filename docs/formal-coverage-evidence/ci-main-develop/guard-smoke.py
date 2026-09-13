"""Execute the frozen workflow's identity and custody guards in disposable local Git trees."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import yaml

workflow = Path(sys.argv[1])
steps = yaml.safe_load(workflow.read_text())['jobs']['trusted-base-validation']['steps']
scripts = [steps[3]['run'], steps[4]['run']]
results = []

def command(arguments, cwd, environment=None):
    result = subprocess.run(arguments, cwd=cwd, env=environment, text=True, capture_output=True)
    if result.returncode:
        raise AssertionError((arguments, result.returncode, result.stderr))
    return result.stdout.strip()

with tempfile.TemporaryDirectory(prefix='peritus-parent-authority-guards-') as temporary:
    root = Path(temporary)
    def fixture(name, base_dirty=False, candidate_dirty=False, selector=False):
        location = root / name
        location.mkdir()
        repo = location / 'source'
        repo.mkdir()
        environment = os.environ.copy()
        environment.update(GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_NOSYSTEM='1')
        command(['git', 'init', '--quiet'], repo, environment)
        def commit(label):
            command(['git', 'add', '.'], repo, environment)
            command(['git', '-c', 'user.name=Peritus local test', '-c', 'user.email=peritus@example.invalid', 'commit', '--quiet', '-m', label], repo, environment)
            return command(['git', 'rev-parse', 'HEAD'], repo, environment)
        (repo/'xtask/src').mkdir(parents=True)
        (repo/'xtask/src/lib.rs').write_text('// reviewed inert checker input\n')
        checker = commit('checker baseline')
        (repo/'base-note.txt').write_text('unrelated develop content\n')
        if base_dirty:
            (repo/'xtask/src/lib.rs').write_text('// changed base checker input\n')
        base = commit('develop base')
        (repo/'candidate-note.txt').write_text('unrelated candidate content\n')
        if candidate_dirty:
            (repo/'xtask/src/lib.rs').write_text('// changed candidate checker input\n')
        if selector:
            (repo/'.cargo').mkdir()
            (repo/'.cargo/config').write_text('# alternate selector\n')
        candidate = commit('candidate')
        for dirname, revision in [('authority', checker), ('base', base), ('candidate', candidate)]:
            command(['git', 'clone', '--quiet', '--no-hardlinks', str(repo), dirname], location, environment)
            command(['git', 'checkout', '--quiet', '--detach', revision], location/dirname, environment)
        (location/'runner-temp').mkdir()
        environment.update(CHECKER_SHA=checker, EVENT_SHA=checker, BASE_SHA=base,
            CANDIDATE_SHA=candidate, EVENT_NAME='pull_request_target', DEFAULT_BRANCH='main',
            EVENT_REF='refs/heads/main', WORKFLOW_REF='fixture/project/.github/workflows/formal-authority.yml@refs/heads/main',
            GITHUB_REPOSITORY='fixture/project', BASE_REPOSITORY='fixture/project',
            BASE_REPOSITORY_ID='123', REPOSITORY_ID='123', BASE_REF='develop',
            RUNNER_TEMP=str(location/'runner-temp'))
        return location, environment

    def probe(name, fixture_value, expected_stage=None, overrides=None, base_revision=None):
        location, original_environment = fixture_value
        environment = original_environment.copy()
        environment.update(overrides or {})
        if base_revision:
            command(['git', 'checkout', '--quiet', '--detach', base_revision], location/'base', environment)
        codes = []
        failed_stage = None
        for stage, script in enumerate(scripts):
            result = subprocess.run(['bash', '-euo', 'pipefail', '-c', script], cwd=location,
                env=environment, text=True, capture_output=True)
            codes.append(result.returncode)
            if result.returncode:
                failed_stage = stage
                break
        if base_revision:
            command(['git', 'checkout', '--quiet', '--detach', original_environment['BASE_SHA']], location/'base', environment)
        assert failed_stage == expected_stage, (name, failed_stage, codes, result.stderr)
        results.append({'case': name, 'exit_codes': codes, 'expected_failure_stage': expected_stage})

    clean = fixture('clean')
    probe('unchanged_develop_three_distinct_revisions', clean)
    checker = clean[1]['CHECKER_SHA']
    base = clean[1]['BASE_SHA']
    probe('unchanged_main_checker_equals_base', clean, overrides={'BASE_REF':'main', 'BASE_SHA':checker}, base_revision=checker)
    probe('wrong_event_revision', clean, 0, {'EVENT_SHA':base})
    probe('checker_role_swap', clean, 0, {'CHECKER_SHA':base, 'EVENT_SHA':base})
    probe('base_checkout_mismatch', clean, 0, {'BASE_SHA':checker})
    probe('candidate_checkout_mismatch', clean, 0, {'CANDIDATE_SHA':base})
    probe('unsupported_base_branch', clean, 0, {'BASE_REF':'staging'})
    probe('wrong_repository_identity', clean, 0, {'BASE_REPOSITORY_ID':'124'})
    probe('main_base_cannot_differ_from_checker', clean, 0, {'BASE_REF':'main'})
    probe('protected_input_drift_at_base', fixture('base-drift', base_dirty=True), 1)
    probe('protected_input_drift_at_candidate', fixture('candidate-drift', candidate_dirty=True), 1)
    probe('alternate_cargo_selector', fixture('selector', selector=True), 1)

print(json.dumps({'workflow_sha256':hashlib.sha256(workflow.read_bytes()).hexdigest(),
    'scope':'Only unmodified identity/custody guard scripts; no build, metadata, candidate execution, hosted Actions, bootstrap or App authorization.',
    'passed':len(results),'results':results}, indent=2))

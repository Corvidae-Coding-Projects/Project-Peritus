#!/usr/bin/env python3
"""Reopen actual retained native read cards; expanding must not change source or history."""
import argparse, hashlib, json, os, subprocess, time
from pathlib import Path
from terminal import Terminal

p = argparse.ArgumentParser()
p.add_argument('--fixture', type=Path, required=True)
p.add_argument('--binary-dir', type=Path, required=True)
p.add_argument('--ui', type=Path, required=True)
p.add_argument('--probe', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--fixed', action='store_true')
p.add_argument('--variants', action='store_true', help='Also request fresh real-provider Markdown, Unicode and short reads')
p.add_argument('--variants-only', action='store_true', help='Replay existing variant evidence without repeating completed inference')
a = p.parse_args()
a.output.mkdir(parents=True, exist_ok=True)
fixture = json.loads(a.fixture.read_text())
env = os.environ | fixture['overrides']
source = Path(fixture['workspace']) / 'large.txt'
assert hashlib.sha256(source.read_bytes()).hexdigest() == fixture['source_sha256']
def inspect():
    return json.loads(subprocess.check_output(
        [str(a.probe), fixture['endpoint'], fixture['run']], env=env, text=True, timeout=20))

log = (a.output / 'daemon.log').open('w')
daemon = subprocess.Popen([str(a.binary_dir / 'peritusd'), 'serve', '--config', fixture['configuration']],
                          env=env, stdout=log, stderr=log)
ui = None
try:
    deadline = time.monotonic() + 45
    while not Path(fixture['endpoint']).exists():
        assert daemon.poll() is None, (a.output / 'daemon.log').read_text()
        assert time.monotonic() < deadline
        time.sleep(.05)
    before = inspect()
    assert any('READROW-0299' in item['detail'] for item in before['activities'])
    cases = []
    for width in (() if a.variants_only else (80, 120, 160)):
        ui = Terminal([a.ui, '--resume'], env, a.output / f'native-{width}', width=width, height=50)
        ui.wait(lambda text: 'DISPLAY-CONTEXT-TEST-COMPLETE-64291' in text, 'opened')
        if a.fixed:
            ui.wait(lambda text: '/details expands' in text, 'compact')
            assert 'READROW-0299' not in ui.text, ui.text
            assert 'READROW-0000' in ui.text, ui.text
            # This existing command toggles the actual native presentation; no inference.
            ui.command('/details')
            ui.wait(lambda text: 'READROW-0299' in text, 'expanded')
            assert '/details expands' not in ui.text
            ui.command('/details')
            ui.wait(lambda text: '/details expands' in text and 'READROW-0299' not in text, 'collapsed')
            # Toggle while scrolled: stale expanded offsets must not blank a compact transcript.
            ui.command('/details'); ui.wait(lambda text: 'READROW-0299' in text, 'expanded-again')
            for _ in range(8):
                ui.send(b'\x1b[5~'); ui.pump(.03)
            ui.send(b'\x1b'); ui.pump(.3)
            ui.wait(lambda text: '/details expands' in text and 'DISPLAY-CONTEXT-TEST-COMPLETE-64291' in text,
                    'escape-collapsed')
        else:
            assert 'READROW-0299' in ui.text, ui.text
            assert 'more lines' not in ui.text, ui.text
        cases.append({'width': width, 'compact_default': a.fixed, 'passed': True})
        ui.close(); ui = None
    after = inspect()
    assert before['activities'] == after['activities'], 'Presentation actions changed durable activities'
    assert before['phase'] == after['phase'], 'Presentation actions started work'
    assert hashlib.sha256(source.read_bytes()).hexdigest() == fixture['source_sha256']
    variants = None
    if a.variants:
        assert a.fixed, 'Variants require the changed native UI'
        workspace = Path(fixture['workspace'])
        files = {
            'preview-guide.md': '# Preview guide\n\n```rust\n' +
                ''.join(f'let row_{i} = {i};\n' for i in range(70)) + '```\nDOC-TAIL-27519\n',
            'preview-unicode.txt': 'UNICODE-HEAD-63581 ' + '🦀café界 ' * 500 + 'UNICODE-TAIL-82647\n',
            'preview-short.txt': 'SHORT-HEAD-17438\nSHORT-MIDDLE-92861\nSHORT-TAIL-34671\n',
        }
        for name, content in files.items():
            path = workspace / name
            if path.exists(): assert path.read_text() == content, path
            else: path.write_text(content)
        answer = 'PREVIEW-VARIANTS-COMPLETE-74395'
        observed = inspect()
        completed = any(item['kind'] == 'Assistant' and item['text'].strip() == answer
                        for item in observed['activities'])
        ui = Terminal([a.ui, '--resume'], env, a.output / 'variants', width=120, height=50)
        marker = answer if completed else 'DISPLAY-CONTEXT-TEST-COMPLETE-64291'
        ui.wait(lambda text: marker in text, 'ready')
        if not completed: ui.command('Read-only presentation exercise. Use workspace_read exactly once for each '
                   'preview-guide.md, preview-unicode.txt and preview-short.txt, without line ranges. '
                   'Also call workspace_read once on intentionally-absent-preview.txt to observe its '
                   'expected missing-file error; do not retry that missing file. Do not edit files, '
                   'run commands/tests, or invoke the pipeline. Finally respond with exactly ' + answer)
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            ui.pump(.25)
            observed = inspect()
            if any(item['kind'] == 'Assistant' and item['text'].strip() == answer
                   for item in observed['activities']): break
        else: raise AssertionError(ui.text)
        for name in files:
            assert any('workspace_read' in item['text'] and name in item['text']
                       for item in observed['activities']), name
        assert any(item['text'].startswith('Failed workspace_read') and
                   'intentionally-absent-preview.txt' in item['text'] for item in observed['activities'])
        ui.pump(.3)
        observed = inspect()
        def capture_pages(label):
            ui.send(b'\x1b[F'); ui.pump(.2)
            screens = []
            for index in range(140):
                text = ui.text
                if screens and text == screens[-1]: break
                screens.append(text)
                (a.output / f'{label}-{index:03}.screen').write_text(text)
                ui.send(b'\x1b[5~')
                for _ in range(5):
                    ui.pump(.1)
                    if ui.text != text: break
            return '\n'.join(screens)
        compact = capture_pages('variants-compact')
        assert 'DOC-TAIL-27519' not in compact and 'UNICODE-TAIL-82647' not in compact
        assert 'intentionally-absent-preview.txt' in compact and 'Failed workspace_read' in compact
        assert all(token in compact for token in ('Preview guide', 'row_0', '/details expands',
                    'UNICODE-HEAD-63581', 'SHORT-HEAD-17438', 'SHORT-MIDDLE-92861', 'SHORT-TAIL-34671'))
        ui.command('/details'); ui.pump(.2)
        expanded = capture_pages('variants-expanded')
        assert 'DOC-TAIL-27519' in expanded and 'UNICODE-TAIL-82647' in expanded
        assert '🦀café界' in expanded
        assert observed['activities'] == inspect()['activities'], 'Display paging started work'
        for name, content in files.items(): assert (workspace / name).read_text() == content
        ui.close(); ui = None
        variants = {'real_provider_read_count': 4, 'markdown': True, 'unicode_long_line': True,
                    'short_read_complete': True, 'missing_read_error_visible': True}
    result = {'passed': True, 'fixed': a.fixed, 'cases': cases, 'same_source': True,
              'same_durable_activities_for_display_actions': True,
              'presentation_actions_started_inference': False, 'additional_live_variants': variants}
    (a.output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
finally:
    if ui: ui.close()
    if daemon.poll() is None:
        subprocess.run([str(a.binary_dir / 'peritus'), '--endpoint', fixture['endpoint'], 'shutdown', '--wait'],
                       env=env, capture_output=True, timeout=30)
    if daemon.poll() is None: daemon.terminate()
    daemon.wait(timeout=10)
    log.close()

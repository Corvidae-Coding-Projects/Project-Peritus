"""Inspect committed live-campaign artifacts and actual terminal readbacks."""
import hashlib
import json
import sys
from pathlib import Path

import argparse
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('base', type=Path)
parser.add_argument('--prefix', default='')
parser.add_argument('--allow-host-upgrade', action='store_true')
args = parser.parse_args()
base = args.base
prefix = args.prefix
receipt = json.loads((base / (prefix + 'receipt.json')).read_text())
job = Path(receipt['job'])
ledger = job.parent / 'research'
report_digest = json.loads((ledger / 'report.json').read_text())['digest']
review_digest = json.loads((ledger / 'review.json').read_text())['digest']
report_bytes = (ledger / f'reports/{report_digest}.json').read_bytes()
report = json.loads(report_bytes)
review_bytes = (ledger / f'reviews/{review_digest}.json').read_bytes()
review = json.loads(review_bytes)
assert hashlib.sha256(report_bytes).hexdigest() == report_digest
assert hashlib.sha256(review_bytes).hexdigest() == review_digest
assert receipt['report_digest'] == report_digest
phase = 6 if json.loads(job.read_text()).get('workflow_version', 1) >= 2 else 5
assert json.loads(job.read_text())['phase'] == phase
assert review['approved'] and review['report_digest'] == report_digest
expected = {f['admission'] for f in report['findings']}
assert set(review['reviewed']) == expected
checks = [json.loads((ledger / f'checks/{report_digest}/{a}.json').read_text()) for a in expected]
assert all(c['approved'] and c['report_digest'] == report_digest for c in checks)
markdown = (base / (prefix + 'checked-report.md')).read_text()
assert markdown == (ledger / f'reports/{report_digest}-checked.md').read_text()
page = (base / 'report-ui-page.txt').read_text()
if len(markdown.encode()) <= 32768:
    assert page == markdown
else:
    marker = page.rsplit('\n\nContinue with show ', 1)
    offset = int(marker[1].split()[0])
    assert marker[0] == markdown.encode()[:offset].decode()
first = report['findings'][0]
admission = json.loads((ledger / f"admissions/{first['admission']}.json").read_text())
candidate = json.loads((ledger / f"candidates/{admission['candidate']}.json").read_text())
evidence = (base / 'evidence-ui.txt').read_text()
assert '> ' + candidate['quote'].replace('\n', '\n> ') in evidence
assert first['admission'] in evidence and report_digest in evidence
status = receipt['retained_status']
assert f'Committed stages: {phase} of {phase}' in status and 'Source-checked: true' in status
steps = receipt['steps']
shows = [s for s in steps if s['command'] == '/module peritus.deep-research show']
assert len(shows) == 2 and all(s['summary'] == page for s in shows)
assert (base / 'report-ui-complete.txt').read_text() == markdown
assert (base / 'inquiry-ui.txt').read_text().strip()
assert review['inquiry_assessment'].strip()
assert report['conclusions']
assert 'Source role:' in markdown
assert steps[-1]['summary'].strip() == 'ORDINARY-CHAT-STILL-WORKS'
canary = base / 'workspace'
assert (canary / 'README.md').read_text() == 'Disposable ordinary folder. Research must not edit this file.\n'
origins = receipt['origins']
assert len(origins) >= 2 and origins[0]['daemon_pid'] != origins[-1]['daemon_pid']
if not args.allow_host_upgrade:
    assert len({o['client_sha256'] for o in origins}) == 1
    assert len({o['daemon_sha256'] for o in origins}) == 1
result = {
    'status': 'PASS', 'exact_host_origins': origins, 'host_upgrade_explicitly_allowed': args.allow_host_upgrade, 'scope': 'Committed workflow and actual terminal readbacks; semantic audit separate',
    'report_digest': report_digest, 'review_digest': review_digest,
    'citations': len(expected), 'approved_exact_digest_checks': len(checks),
    'synthesis_words': len(report['synthesis'].split()),
    'rendered_words': len(markdown.split()), 'rendered_bytes': len(markdown.encode()),
    'report_page_exact': True, 'all_report_pages_exact': True, 'evidence_quote_exact': True, 'inquiry_state_readback': True,
    'checked_report_survived_fresh_daemon': True,
    'ordinary_chat_exact_marker': True, 'workspace_canary_unchanged': True,
    'origins': origins,
}
(base / 'readback-audit.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))

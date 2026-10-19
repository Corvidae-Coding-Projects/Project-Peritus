#!/usr/bin/env python3
"""Read back the actual terminal byte stream; never construct an application page."""
import argparse
import codecs
import json
import re
from pathlib import Path
import pyte

parser = argparse.ArgumentParser()
parser.add_argument('fixture', type=Path)
parser.add_argument('--required-stages', nargs='+', choices=['accepted', 'reasoning', 'generating'],
                    default=['accepted', 'reasoning', 'generating'])
parser.add_argument('--require-heartbeat', action='store_true')
args = parser.parse_args()
screen = pyte.Screen(160, 45)
stream = pyte.Stream(screen)
decoder = codecs.getincrementaldecoder('utf-8')('replace')
raw = (args.fixture / 'tui.ansi').read_bytes()
markers = {'accepted': 'Provider runtime accepted the request',
           'reasoning': 'Provider is reasoning',
           'generating': 'Receiving structured response:'}
found = {}
for offset in range(0, len(raw), 96):
    stream.feed(decoder.decode(raw[offset:offset + 96]))
    visible = '\n'.join(screen.display)
    for stage, marker in markers.items():
        if marker in visible and stage not in found:
            found[stage] = offset
            (args.fixture / (stage + '-rendered.screen')).write_text(visible)
    heartbeat = re.search(r'Provider is reasoning \((\d+)s elapsed\)', visible)
    if heartbeat and int(heartbeat[1]) >= 20 and 'heartbeat' not in found:
        found['heartbeat'] = offset
        (args.fixture / 'heartbeat-rendered.screen').write_text(visible)
assert set(args.required_stages).issubset(found), found
if args.require_heartbeat:
    assert 'heartbeat' in found, found
print(json.dumps({'rendered_stages': found, 'raw_bytes': len(raw)}))

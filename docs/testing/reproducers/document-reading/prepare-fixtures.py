#!/usr/bin/env python3
"""Generate ordinary text fixtures and a separate oracle for real Peritus reads."""
import argparse
import json
from pathlib import Path
import random

parser = argparse.ArgumentParser()
parser.add_argument('destination', type=Path)
parser.add_argument('--seed', type=int, default=20261011)
args = parser.parse_args()
args.destination.mkdir(parents=True, exist_ok=True)
workspace = args.destination/'workspace'
workspace.mkdir(exist_ok=True)
rng = random.Random(args.seed)
oracle = {}
for name, count, paragraph in (
    ('short.md', 12, 'Renée ✓ — text with "quotes" and \\ paths.'),
    ('design.md', 388, 'Ordinary product requirements describe the interface, expected behavior and independent saved state.'),
    ('long.md', 2000, 'Requirements cover distinct input values, state transitions, recovery and persistence through actual interfaces.'),
    ('multimegabyte.md', 18000, 'Product requirements describe accessible controls, caller authority, document boundaries, independent saved state and observable recovery.'),
):
    values = {key:rng.randbytes(12).hex() for key in ('first','middle','last')}
    rows = [f'Section {i:05}: {paragraph}\n' for i in range(1,count+1)]
    for key,index in (('first',3),('middle',count//2),('last',count-2)):
        rows[index-1] = f'{key.upper()}_MARKER={values[key]}\n'
    path = workspace/name
    path.write_text(''.join(rows))
    oracle[name] = {'markers':values,'bytes':path.stat().st_size,'lines':count}
values = {key:rng.randbytes(12).hex() for key in ('first','middle','last')}
text = f'FIRST_MARKER={values["first"]} ' + ('"\\ café🙂 ' * 7000) + f' MIDDLE_MARKER={values["middle"]} ' + ('"\\ العربية✓ ' * 7000) + f' LAST_MARKER={values["last"]}\n'
p = workspace/'escaped.md'
p.write_text(text)
oracle[p.name] = {'markers':values,'bytes':p.stat().st_size,'lines':1}
values = {key:rng.randbytes(12).hex() for key in ('first','middle','last')}
rows = [f'Section {i:05}: Product requirements describe accessible controls, caller authority, document boundaries, independent saved state and observable recovery.\n' for i in range(1,5001)]
for key,index in (('first',3),('middle',2500),('last',4998)):
    rows[index-1] = f'{key.upper()}_MARKER={values[key]}\n'
p = workspace/'large-supported.md'
p.write_text(''.join(rows))
oracle[p.name] = {'markers':values,'bytes':p.stat().st_size,'lines':5000}
(workspace/'empty.md').write_text('')
(workspace/'invalid.txt').write_bytes(b'Invalid UTF-8: \xff\xfe\n')
p = workspace/'blank.md'
p.write_bytes(b'\n'*(2*1024*1024))
oracle[p.name] = {'bytes':p.stat().st_size,'lines':2*1024*1024}
(workspace/'peritus-workspace.toml').write_text('schema_version = 1\nkind = "artifact"\n')
(args.destination/'oracle.json').write_text(json.dumps({'seed':args.seed,'files':oracle},ensure_ascii=False,indent=2)+'\n')
(args.destination/'ordinary-task.txt').write_text('List the workspace. In one call per file, read short.md, design.md and long.md with default workspace_read arguments (no line limits or max_bytes overrides). For each return the exact FIRST_MARKER, MIDDLE_MARKER and LAST_MARKER values. Then read escaped.md with max_bytes=4096, and continue its returned exact line/byte cursor with max_bytes=1048576; report all three markers and both cursor values. Finally read only the first page of blank.md with max_bytes=4096, reporting end_line and next_line; do not fetch its remaining blank pages. Return one JSON object keyed by those five filenames, including the observation metadata and markers. Use only workspace_list and workspace_read. Do not search, run commands, guess values, edit files, or read oracle files outside this workspace.\n')
(args.destination/'multimegabyte-task.txt').write_text('List the workspace, then read multimegabyte.md exactly once with workspace_read using max_bytes=4194304 and no line limits. From that actual observation return one JSON object with FIRST_MARKER, MIDDLE_MARKER, LAST_MARKER, end_line and next_line. Use no other reads, searches, commands, edits, or guesses.\n')
print(json.dumps({name:{k:v for k,v in spec.items() if k!='markers'} for name,spec in oracle.items()},indent=2))

(args.destination/'large-supported-task.txt').write_text('List the workspace, then read large-supported.md exactly once with default workspace_read arguments, without line limits or a max_bytes override. Return FIRST_MARKER, MIDDLE_MARKER, LAST_MARKER, end_line and next_line from that actual observation. Use no searches, commands, edits, other reads, or guesses.\n')
(args.destination/'range-task.txt').write_text('List the workspace. Read long.md from line 1490 through 1510 inclusive, empty.md with default arguments, and invalid.txt with default arguments, each exactly once. Return the line-range metadata and first/last returned line, the empty document content and continuation metadata, and the exact invalid-file error. Use no searches, commands, other reads or edits.\n')

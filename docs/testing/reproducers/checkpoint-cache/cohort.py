"""Select Cargo's actual linked dependency cohort, rather than newest feature variant."""
import json
from pathlib import Path
def select(deps,required,anchor=None):
    anchor=anchor or ('peritus_app_client' if 'peritus_app_client' in required else 'peritus_daemon')
    fingerprint=deps.parent/'.fingerprint';index={};roots=[]
    for p in fingerprint.glob('*/lib-*.json'):
        h=p.with_suffix('')
        try:stamp=int.from_bytes(bytes.fromhex(h.read_text().strip()),'little');data=json.loads(p.read_text())
        except (OSError,ValueError):continue
        name=p.stem.removeprefix('lib-');suffix=p.parent.name.rsplit('-',1)[1];rlib=deps/f'lib{name}-{suffix}.rlib'
        if not rlib.exists():continue
        index[(name,stamp)]=(data,rlib)
        if name==anchor:roots.append((p.stat().st_mtime,data,rlib))
    assert roots,'anchor fingerprint required';_,root,root_rlib=max(roots);selected={anchor:root_rlib};pending=[root]
    while pending:
        item=pending.pop()
        for _,name,_,stamp in item['deps']:
            match=index.get((name,stamp))
            if match and name not in selected:
                data,rlib=match;selected[name]=rlib;pending.append(data)
    for name in required:
        if name not in selected:
            # External client consumer is not a daemon dependency. Require its ABI peers to match.
            candidates=[]
            for (n,stamp),(data,rlib) in index.items():
                if n!=name:continue
                mismatch=False
                for _,dep,_,expected in data['deps']:
                    if dep in selected:
                        lookup=index.get((dep,expected))
                        if lookup and lookup[1]!=selected[dep]:mismatch=True;break
                if not mismatch:candidates.append(rlib)
            assert candidates,(name,candidates)
            selected[name]=max(candidates,key=lambda path:path.stat().st_mtime)
    return {name:selected[name] for name in required}

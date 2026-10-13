#!/usr/bin/env python3
"""Read the daemon's real workbench queue; reject the old acceptance-policy mismatch."""
import argparse,subprocess
p=argparse.ArgumentParser();p.add_argument('--observer',required=True);p.add_argument('--endpoint',required=True);p.add_argument('--run',required=True);a=p.parse_args()
r=subprocess.run([a.observer,a.endpoint,a.run],capture_output=True,text=True,check=True)
marker='\nPERITUS HARNESS EVALUATION\n'
if marker not in r.stdout:raise SystemExit('FAIL: native evaluation directive missing')
body=r.stdout.split(marker,1)[1]
if 'Use the real-testing skill' not in body or 'Do not generate or run unit tests' not in body or 'create the smallest complete harness patch and regression tests' in body:raise SystemExit('FAIL: evaluation directive does not govern real-interface acceptance')
print('PASS: actual native evaluation queue requires real testing and preserves the unit-test exclusion')

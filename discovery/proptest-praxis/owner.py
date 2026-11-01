#!/usr/bin/env python3
"""Build, attest and run discovery-only modules inside the actual product-context owner."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import campaign

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
TARGETS=("reads","decision","plans","inputs","daemon","openai_http")

def digest(data):
    return hashlib.sha256(data).hexdigest()

def git(*args):
    return subprocess.check_output(["git","-C",str(ROOT),*args])

def files_digest(root, paths):
    h=hashlib.sha256()
    for relative in paths:
        name=relative.encode(); data=(root/relative).read_bytes()
        h.update(len(name).to_bytes(8,"big"));h.update(name)
        h.update(len(data).to_bytes(8,"big"));h.update(data)
    return h.hexdigest()

def snapshot():
    diff=git("diff","--binary","HEAD","--","crates","Cargo.toml","Cargo.lock")
    paths=sorted(set(p.decode() for p in git("ls-files","--cached","--others","--exclude-standard","-z","--","crates","Cargo.toml","Cargo.lock","rust-toolchain.toml",".cargo/config.toml").split(b"\0") if p))
    harness=["Cargo.toml","Cargo.lock","build.rs"]
    harness+=sorted(str(p.relative_to(HERE)) for folder in ("src","owner","support") for p in (HERE/folder).rglob("*") if p.is_file() and "__pycache__" not in p.relative_to(HERE).parts)
    return {"source_head":git("rev-parse","HEAD").decode().strip(),"production_diff_sha256":digest(diff),"production_dirty":bool(diff),"production_source_sha256":files_digest(ROOT,paths),"harness_sha256":files_digest(HERE,sorted(harness))}

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("command",choices=("build","lint","search","examples","calibration","replay"))
    p.add_argument("--output",type=Path,required=True)
    p.add_argument("--target",choices=TARGETS+("all",),default="all")
    p.add_argument("--package", choices=("peritus-product-runner","peritus-daemon","peritus-provider-openai"))
    p.add_argument("--receipt",type=Path)
    p.add_argument("--target-dir",type=Path)
    p.add_argument("--timeout",type=int,default=900)
    p.add_argument("--cases",type=int,default=256)
    p.add_argument("--max-steps",type=int,default=64)
    p.add_argument("--seed",type=int,default=7319)
    p.add_argument("--case",type=Path)
    a=p.parse_args()
    a.output=a.output.resolve();a.output.mkdir(mode=0o700)
    target=(a.target_dir or ROOT/"target/praxis").resolve()
    package=a.package or ("peritus-provider-openai" if a.target=="openai_http" else "peritus-daemon" if a.target=="daemon" else "peritus-product-runner")
    if a.command in ("build","lint"):
        before=snapshot()
        cmd=["cargo","test" if a.command=="build" else "clippy","--locked","--offline","-p",package,"--lib","--target-dir",str(target),"-j","1"]
        cmd += ["--no-run","--message-format=json"] if a.command=="build" else ["--tests","--","-D","warnings"]
        result=campaign.command(cmd,a.output,a.timeout)
        if result["status"]!="pass": return 2
        if before!=snapshot(): raise ValueError("inputs changed during owner build")
        if a.command=="lint": return 0
        executables=[]
        for line in (a.output/"stdout.log").read_text().splitlines():
            try: v=json.loads(line)
            except ValueError: continue
            if v.get("reason")=="compiler-artifact" and v.get("target",{}).get("name")==package.replace("-","_") and v.get("profile",{}).get("test") and v.get("executable"):
                executables.append(v["executable"])
        if len(executables)!=1: raise ValueError(f"expected one actual owner test executable, got {executables}")
        exe=Path(executables[0])
        receipt={"package":package,"schema_version":1,"inputs":before,"executable":str(exe),"executable_sha256":digest(exe.read_bytes()),"rustc":subprocess.check_output(["rustc","--version"],cwd=ROOT).decode().strip(),"build_process":result}
        campaign.save(a.output/"receipt.json",receipt)
        print(json.dumps({"status":"pass","receipt":str(a.output/"receipt.json")}));return 0
    if a.receipt is None: p.error("a successful --receipt from this source state is required")
    receipt=json.loads(a.receipt.read_text())
    if receipt["inputs"]!=snapshot(): raise ValueError("stale owner build receipt; rebuild")
    exe=Path(receipt["executable"])
    if digest(exe.read_bytes())!=receipt["executable_sha256"]: raise ValueError("owner executable digest changed")
    available=("openai_http",) if receipt.get("package")=="peritus-provider-openai" else ("daemon",) if receipt.get("package")=="peritus-daemon" else ("reads","decision","plans","inputs")
    targets=available if a.target=="all" else (a.target,)
    if any(t not in available for t in targets): raise ValueError("target does not belong to the attested owner package")
    results=[]
    for t in targets:
        folder=a.output/t;folder.mkdir(mode=0o700)
        env={"PRAXIS_OUTPUT":str(folder/"case-output"),"PRAXIS_COMMAND":a.command,"PRAXIS_RECEIPT":str(a.receipt.resolve()),"PRAXIS_CASES":str(a.cases),"PRAXIS_STEPS":str(a.max_steps),"PRAXIS_SEED":str(a.seed)}
        if a.case: env["PRAXIS_CASE"]=str(a.case.resolve())
        stage=5 if t=="openai_http" else 3 if t in ("inputs","daemon") else 2
        result=campaign.command([str(exe),f"praxis_stage{stage}_{t}","--nocapture","--test-threads=1"],folder,a.timeout,env)
        report=folder/"case-output/report.json"
        if report.exists():
            result["report"]=json.loads(report.read_text())
            reported=result["report"].get("status")
            # The Rust test runner exits 101 for an ordinary failed assertion. A
            # report written before a timeout, signal or abnormal exit cannot
            # qualify that process as a successful behavioral check.
            native_ok=not result["timed_out"] and result["exit_code"]==0
            ordinary_failure=not result["timed_out"] and result["exit_code"]==101
            if reported=="pass" and native_ok:result["status"]="pass"
            elif reported=="fail" and ordinary_failure:result["status"]="fail"
            else:
                result["status"]="incomplete"
                result["reason"]="behavioral report and native process outcome do not agree"
        else:result["status"]="incomplete"
        result["target"]=t;results.append(result)
    status="incomplete" if any(r["status"]=="incomplete" for r in results) else "fail" if any(r["status"]=="fail" for r in results) else "pass"
    campaign.save(a.output/"summary.json",{"status":status,"results":results})
    print(json.dumps({"status":status,"output":str(a.output),"targets":[{"target":r["target"],"status":r["status"],"seconds":r["elapsed_seconds"]}for r in results]}))
    return {"pass":0,"fail":1,"incomplete":2}[status]

if __name__=="__main__":
    try:sys.exit(main())
    except (OSError,ValueError,subprocess.SubprocessError) as e:
        print(json.dumps({"status":"incomplete","error":str(e)}),file=sys.stderr);sys.exit(2)

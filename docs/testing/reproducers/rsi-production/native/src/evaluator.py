import json, pathlib, sys, time
sealed, stdout, stderr, terminal = map(pathlib.Path, sys.argv[1:])
expected = json.loads(sealed.read_text())
time.sleep(expected.get("delay_seconds", 0))
actual = json.loads(terminal.read_text())
out = stdout.read_bytes().decode("utf-8", errors="replace")
err = stderr.read_bytes().decode("utf-8", errors="replace")
passed = (actual["disposition"] == "Exited" and actual["exit_code"] == expected["code"]
          and actual["output_complete"] and actual["tree_cleanup_complete"]
          and actual["support_tasks_joined"] and actual["publication_complete"])
if "stdout" in expected: passed = passed and out == expected["stdout"]
for marker in expected.get("stdout_contains", []): passed = passed and marker in out
for marker in expected.get("stderr_contains", []): passed = passed and marker in err
if expected.get("stderr_empty"): passed = passed and not err
print(json.dumps({"passed": bool(passed)}, separators=(",", ":")))

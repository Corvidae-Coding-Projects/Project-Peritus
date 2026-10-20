#!/usr/bin/env python3
"""Drive the real `peritus providers` flow in a pty and record its exact transcript."""

import os
import pty
import select
import time


def drive(argv, env, sends, timeout=180, settle=0.6):
    """Run `argv` on a pty, answer scripted prompts, and return (output, exit code).

    `sends` is an ordered list of `(trigger, payload)` pairs: the payload is written once the
    accumulated output contains its trigger. `settle` waits after a trigger before writing,
    because hidden credential entry enables raw mode and discards input queued before it.
    """
    pid, fd = pty.fork()
    if pid == 0:
        try:
            os.execvpe(argv[0], argv, env)
        finally:
            os._exit(127)
    out = bytearray()
    pending = list(sends)
    deadline = time.time() + timeout
    while True:
        if time.time() > deadline:
            os.kill(pid, 9)
            out += b"\n[driver timeout]\n"
            break
        readable, _, _ = select.select([fd], [], [], 0.2)
        if readable:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                chunk = b""
            if not chunk:
                break
            out += chunk
        if pending and pending[0][0] in out:
            _, payload = pending.pop(0)
            time.sleep(settle)
            os.write(fd, payload)
    _, status = os.waitpid(pid, 0)
    return bytes(out), os.waitstatus_to_exitcode(status)

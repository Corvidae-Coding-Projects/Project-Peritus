#!/usr/bin/env python3
"""Exercise the real updater CLI through a local HTTPS metadata endpoint."""

import argparse
import errno
import hashlib
import json
import os
from pathlib import Path
import pty
import random
import select
import socketserver
import ssl
import subprocess
import tempfile
import threading
import time


LIMIT = 4 * 1024 * 1024
SEED = 424242


def metadata(size, unicode_text=False):
    prefix = json.dumps(
        {"tag_name": "v0.0.7", "name": "release λ" if unicode_text else "release"},
        ensure_ascii=False,
    )[:-1].encode() + b',"body":"'
    suffix = b'","assets":[]}'
    return prefix + b"x" * (size - len(prefix) - len(suffix)) + suffix


def run_cli(binary, root, additions=None):
    root.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment.update(
        XDG_CONFIG_HOME=str(root / "config"),
        XDG_STATE_HOME=str(root / "state"),
        XDG_CACHE_HOME=str(root / "cache"),
    )
    if additions:
        environment.update(additions)
    master, slave = pty.openpty()
    process = subprocess.Popen(
        [str(binary), "update"], stdin=slave, stdout=slave, stderr=slave,
        env=environment, start_new_session=True,
    )
    os.close(slave)
    output = bytearray()
    deadline = time.monotonic() + 30
    try:
        while True:
            if time.monotonic() >= deadline:
                raise TimeoutError("real updater did not finish within 30 seconds")
            if select.select([master], [], [], 0.1)[0]:
                try:
                    chunk = os.read(master, 65536)
                except OSError as error:
                    if error.errno == errno.EIO:
                        break
                    raise
                if not chunk:
                    break
                output.extend(chunk)
        return process.wait(timeout=5), output.decode(errors="replace").replace("\r", "")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)


def certificates(root):
    def openssl(*arguments):
        subprocess.run(
            ["openssl", *arguments], cwd=root, check=True,
            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=15,
        )

    openssl("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
            "-subj", "/CN=Peritus disposable replay CA", "-keyout", "ca.key",
            "-out", "ca.pem", "-addext", "basicConstraints=critical,CA:TRUE")
    openssl("req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=api.github.com",
            "-keyout", "server.key", "-out", "server.csr")
    (root / "extensions").write_text(
        "subjectAltName=DNS:api.github.com\nbasicConstraints=critical,CA:FALSE\n"
        "keyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n"
    )
    openssl("x509", "-req", "-in", "server.csr", "-CA", "ca.pem", "-CAkey",
            "ca.key", "-CAcreateserial", "-days", "1", "-out", "server.pem",
            "-extfile", "extensions")
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(root / "server.pem", root / "server.key")
    return context


def read_headers(stream):
    data = bytearray()
    while not data.endswith(b"\r\n\r\n"):
        byte = stream.recv(1)
        if not byte:
            raise EOFError("client closed while sending request headers")
        data.extend(byte)
        if len(data) > 16384:
            raise ValueError("fixture request header exceeded 16 KiB")
    return bytes(data)


class Endpoint(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


class Handler(socketserver.BaseRequestHandler):
    def handle(self):
        try:
            self.request.settimeout(10)
            connect = read_headers(self.request)
            if not connect.startswith(b"CONNECT api.github.com:443 HTTP/"):
                raise ValueError("fixture received an unexpected CONNECT destination")
            self.request.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
            with self.server.context.wrap_socket(self.request, server_side=True) as stream:
                request = read_headers(stream)
                expected = b"GET /repos/Corvidae-Coding-Projects/Project-Peritus/releases/latest HTTP/1.1\r\n"
                if not request.startswith(expected):
                    raise ValueError("fixture received an unexpected updater URL")
                self.server.requests.append(request.decode())
                case = self.server.case
                body = case["body"]
                status = case.get("status", 200)
                headers = f"HTTP/1.1 {status} fixture\r\nContent-Type: application/json\r\nConnection: close\r\n"
                chunked = case.get("chunked", False)
                if chunked:
                    headers += "Transfer-Encoding: chunked\r\n"
                else:
                    headers += f"Content-Length: {case.get('declared', len(body))}\r\n"
                stream.sendall((headers + "\r\n").encode())
                chunk_size = case.get("chunk_size", 4096)
                for offset in range(0, len(body), chunk_size):
                    chunk = body[offset:offset + chunk_size]
                    if chunked:
                        stream.sendall(f"{len(chunk):x}\r\n".encode() + chunk + b"\r\n")
                    else:
                        stream.sendall(chunk)
                if chunked:
                    stream.sendall(b"0\r\n\r\n")
        except (BrokenPipeError, ConnectionResetError, ssl.SSLEOFError):
            # Oversized metadata is expected to make the updater close before consuming it.
            pass
        except Exception as error:
            self.server.errors.append(repr(error))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--output", type=Path)
    options = parser.parse_args()
    binary = options.binary.resolve()
    results = []
    randomizer = random.Random(SEED)
    with tempfile.TemporaryDirectory(prefix="peritus-update-replay-") as temporary:
        root = Path(temporary)
        (root / "empty-certs").mkdir()
        with Endpoint(("127.0.0.1", 0), Handler) as endpoint:
            endpoint.context = certificates(root)
            endpoint.errors = []
            endpoint.requests = []
            threading.Thread(target=endpoint.serve_forever, daemon=True).start()
            proxy = f"http://127.0.0.1:{endpoint.server_address[1]}"
            environment = {name: proxy for name in ("HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy")}
            environment.update(NO_PROXY="", no_proxy="", SSL_CERT_FILE=str(root / "ca.pem"),
                               SSL_CERT_DIR=str(root / "empty-certs"))

            def execute(name, case, executable=binary, expected_error=None):
                endpoint.case = case
                before = len(endpoint.requests)
                status, output = run_cli(executable, root / name, environment)
                if expected_error is None:
                    passed = status == 0 and "Peritus 0.0.7 is already current." in output
                else:
                    passed = status != 0 and expected_error in output
                passed = passed and len(endpoint.requests) == before + 1 and not endpoint.errors
                row = {"case": name, "status": "PASS" if passed else "FAIL", "exit": status,
                       "body_bytes": len(case["body"]), "chunked": case.get("chunked", False),
                       "chunk_size": case.get("chunk_size"), "output": output.strip()}
                results.append(row)
                print(json.dumps(row), flush=True)
                if not passed:
                    raise RuntimeError(f"{name} failed; server errors={endpoint.errors}")

            if options.baseline:
                execute("published-regression", {"body": metadata(65537)},
                        options.baseline.resolve(), "current release response exceeds 64 KiB")
            for size in [65535, 65536, 65537, 65538, 110283, 262144, 1048576, LIMIT - 1, LIMIT]:
                for chunked in (False, True):
                    execute(f"size-{size}-chunked-{chunked}", {
                        "body": metadata(size, unicode_text=True), "chunked": chunked,
                        "chunk_size": randomizer.choice([257, 4096, 65536]),
                    })
            execute("declared-oversize", {"body": b"", "declared": LIMIT + 1},
                    expected_error="current release response exceeds 4 MiB")
            execute("chunked-oversize", {"body": metadata(LIMIT + 1), "chunked": True},
                    expected_error="current release response exceeds 4 MiB")
            for name, body, message in [
                ("invalid-json", b"{", "decode current release"),
                ("missing-tag", b'{"body":"release"}', "omitted tag_name"),
                ("nonstring-tag", b'{"tag_name":7}', "omitted tag_name"),
                ("invalid-version", b'{"tag_name":"v0.0.7-preview"}', "not vMAJOR.MINOR.PATCH"),
            ]:
                execute(name, {"body": body}, expected_error=message)
            execute("no-release", {"body": b"{}", "status": 404}, expected_error="no current release")
            execute("server-error", {"body": b"{}", "status": 500}, expected_error="query current release")
            execute("truncated-body", {"body": metadata(65537), "declared": 65547},
                    expected_error="read current release")
            endpoint.shutdown()
        if options.live:
            status, output = run_cli(binary, root / "live")
            passed = status == 0 and "Peritus 0.0.7 is already current." in output
            results.append({"case": "public-github", "status": "PASS" if passed else "FAIL",
                            "exit": status, "output": output.strip()})
            print(json.dumps(results[-1]), flush=True)
            if not passed:
                raise RuntimeError("public GitHub update failed")
    report = {"binary": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "seed": SEED, "cases": len(results), "results": results}
    if options.output:
        options.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"PASS: {len(results)} real CLI scenarios; seed={SEED}")


if __name__ == "__main__":
    main()

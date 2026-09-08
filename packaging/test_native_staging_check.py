"""Explicit fixtures for manual diagnostic isolation and exact byte comparison."""

from contextlib import ExitStack
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import native_staging_check as check


class StagingCheckTests(unittest.TestCase):
    def test_each_previous_path_is_compiled_and_only_equal_bytes_pass(self):
        for windows in (False, True):
            for equal in (False, True):
                with self.subTest(windows=windows, equal=equal), ExitStack() as stack:
                    root = Path(stack.enter_context(tempfile.TemporaryDirectory()))
                    binary = "peritusd" if windows else "peritus"
                    filename = binary + (".exe" if windows else "")
                    bound = {"candidate": {"fixture": True}, "binary": binary,
                             "package": "peritus-daemon" if windows else "peritus-cli",
                             "environment": {"system": "Windows" if windows else "Darwin",
                                             "machine": "AMD64" if windows else "x86_64"}}
                    stack.enter_context(patch.object(check, "ROOT", root))
                    stack.enter_context(patch.object(check.inputs, "binding", return_value=bound))
                    stack.enter_context(patch.object(check.inputs, "normalize_verified_sources"))
                    restore = stack.enter_context(patch.object(check.transport, "restore"))
                    candidate = root / "target/staging-candidate" / filename
                    candidate.parent.mkdir(parents=True)
                    candidate.write_bytes(b"staged non-release fixture")

                    def compile_fixture(*_args):
                        target = root / ("target/release" if windows else "target/native-daemon/release")
                        target.mkdir(parents=True)
                        (target / filename).write_bytes(candidate.read_bytes() if equal else b"different fixture")
                        return {"fixture_observation": True}

                    module = check.windows_release if windows else check.native_build
                    method = "build" if windows else "compile_phase"
                    compile_call = stack.enter_context(patch.object(module, method, side_effect=compile_fixture))
                    if equal:
                        check.check(binary)
                    else:
                        with self.assertRaisesRegex(ValueError, "differs"):
                            check.check(binary)
                    compile_call.assert_called_once()
                    if windows:
                        restore.assert_not_called()
                    else:
                        restore.assert_called_once_with(root / "target/native-daemon-libraries",
                                                        root / "target/native-daemon",
                                                        check.inputs.daemon_binding(bound))
                    report = json.loads((root / "target/native-staging-check.json").read_bytes())
                    self.assertEqual(report["kind"], "native-staging-diagnostic")
                    self.assertEqual(report["byte_identical"], equal)
                    self.assertFalse((root / "target/native-peritusd-build.json").exists())
                    self.assertFalse((root / "target/native-peritus-build.json").exists())
                    with self.assertRaisesRegex(ValueError, "fresh"):
                        check.check(binary)
                    compile_call.assert_called_once()

    def test_unreviewed_platform_or_binary_cannot_compile(self):
        for system, machine, binary in (("Linux", "x86_64", "peritusd"),
                                        ("Darwin", "aarch64", "peritus"),
                                        ("Windows", "AMD64", "peritus")):
            bound = {"environment": {"system": system, "machine": machine}}
            with patch.object(check.inputs, "binding", return_value=bound), \
                    patch.object(check.windows_release, "build") as windows, \
                    patch.object(check.native_build, "compile_phase") as mac, self.assertRaises(ValueError):
                check.check(binary)
            windows.assert_not_called()
            mac.assert_not_called()


if __name__ == "__main__":
    unittest.main()

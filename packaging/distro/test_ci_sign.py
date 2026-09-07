"""Exercise CI signing with a disposable protected key and the real restricted agent."""

import os
from contextlib import contextmanager
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import ci_sign


class RestrictedCacheTests(unittest.TestCase):
    def test_only_selected_keygrips_are_seeded_over_stdin(self):
        grip = "A" * 40
        metadata = f"grp:::::::::{grip}:\ngrp:::::::::{grip}:\n"
        with patch.object(ci_sign, "run", side_effect=[metadata, "/usr/libexec", None]) as run:
            ci_sign.seed_restricted_cache("B" * 40, "test-only password")
        self.assertEqual(run.call_args_list[0].args[-1], "B" * 40)
        self.assertEqual(run.call_args_list[-1].args,
                         (Path("/usr/libexec/gpg-preset-passphrase"), "--preset", "--restricted", grip))
        self.assertEqual(run.call_args_list[-1].kwargs, {"input": "test-only password\n"})

    def test_missing_or_invalid_keygrips_cannot_seed_the_agent(self):
        for metadata in ("", "grp:::::::::invalid:", "grp:::::::::" + "A" * 39 + ":"):
            with self.subTest(metadata=metadata), \
                    patch.object(ci_sign, "run", return_value=metadata) as run:
                with self.assertRaisesRegex(ValueError, "valid agent keygrips"):
                    ci_sign.seed_restricted_cache("B" * 40, "test-only password")
                self.assertEqual(run.call_count, 1)


@unittest.skipUnless(shutil.which("gpg") and shutil.which("gpgconf") and os.name == "posix",
                     "requires native GnuPG and Unix agent sockets")
class CiSigningAgentTests(unittest.TestCase):
    def command(self, *arguments, env, input=None, check=True):
        return subprocess.run(arguments, env=env, input=input, text=True, check=check,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)

    def test_protected_key_signs_through_public_only_restricted_client(self):
        # This key is generated for this test only. Never inspect or export the user's keyring.
        with tempfile.TemporaryDirectory(prefix="peritus-test-key-") as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir(mode=0o700)
            source_env = {**os.environ, "GNUPGHOME": str(source)}
            password = "disposable CI regression key"
            try:
                self.command("gpg", "--batch", "--pinentry-mode", "loopback",
                             "--passphrase-fd", "0", "--quick-generate-key",
                             "Peritus Test <test@example.invalid>", "ed25519", "sign", "0",
                             env=source_env, input=password + "\n")
                listing = self.command("gpg", "--with-colons", "--list-secret-keys",
                                       env=source_env).stdout
                key = next(line.split(":")[9] for line in listing.splitlines()
                           if line.startswith("fpr:"))
                public = self.command("gpg", "--armor", "--export", key, env=source_env).stdout
                private = self.command("gpg", "--batch", "--pinentry-mode", "loopback",
                                       "--passphrase-fd", "0", "--armor", "--export-secret-keys",
                                       key, env=source_env, input=password + "\n").stdout
            finally:
                self.command("gpgconf", "--kill", "gpg-agent", env=source_env)

            observed = []
            temporary_directory = tempfile.TemporaryDirectory

            @contextmanager
            def headless_ci_home(**options):
                with temporary_directory(**options) as directory:
                    (Path(directory) / "gpg-agent.conf").write_text("pinentry-program /bin/false\n")
                    yield directory

            def restricted_sign():
                host_env = dict(os.environ)
                observed.append(Path(host_env["GNUPGHOME"]))
                self.assertNotIn("PERITUS_SIGNING_KEY", host_env)
                self.assertNotIn("PERITUS_SIGNING_PASSPHRASE", host_env)
                client = root / "client"
                client.mkdir(mode=0o700)
                (client / "gpg.conf").write_text("no-autostart\n")
                socket = self.command("gpgconf", "--list-dirs", "agent-extra-socket",
                                      env=host_env).stdout.strip()
                client_env = {**host_env, "GNUPGHOME": str(client)}
                self.command("gpgconf", "--create-socketdir", env=client_env)
                client_socket = Path(self.command("gpgconf", "--list-dirs", "agent-socket",
                                                  env=client_env).stdout.strip())
                client_socket.symlink_to(socket)
                try:
                    self.command("gpg", "--batch", "--import", env=client_env, input=public)
                    payload = root / "payload"
                    payload.write_text("Disposable release-signing regression payload.\n")
                    signature = root / "payload.sig"
                    # Never prompt during a regression, even if the agent cache is empty.
                    result = self.command("gpg", "--batch",
                                          "--local-user", key, "--output", str(signature),
                                          "--detach-sign", str(payload), env=client_env, check=False)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.command("gpg", "--batch", "--verify", str(signature), str(payload),
                                 env=client_env)
                    self.assertFalse((client / "private-keys-v1.d").exists())
                finally:
                    client_socket.unlink()
                    self.command("gpgconf", "--remove-socketdir", env=client_env)

            with patch.dict(os.environ, GITHUB_ACTIONS="true", GITHUB_REF_NAME="v1.2.3",
                            PERITUS_SIGNING_KEY=private, PERITUS_SIGNING_PASSPHRASE=password), \
                    patch.object(ci_sign, "version", return_value="1.2.3"), \
                    patch.object(ci_sign, "fingerprint", return_value=key), \
                    patch.object(ci_sign.tempfile, "TemporaryDirectory", headless_ci_home), \
                    patch.object(ci_sign, "sign", side_effect=restricted_sign):
                previous = os.environ.get("GNUPGHOME")
                ci_sign.sign_ci()
                self.assertEqual(os.environ.get("GNUPGHOME"), previous)
            self.assertEqual(len(observed), 1)
            self.assertFalse(observed[0].exists(), "ephemeral CI keyring must be removed")


if __name__ == "__main__":
    unittest.main()

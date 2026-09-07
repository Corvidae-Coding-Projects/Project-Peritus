"""Audited entry point for first-party distribution release operations."""

import argparse
import unittest
from pathlib import Path

import build
import compiled
import ci_sign
import sign
import verify
import upload


def main():
    actions = {"image": build.build_image, "build": build.build, "sign": sign.sign,
               "compile": compiled.compile_packages, "compile-checks": compiled.compile_checks,
               "package-compiled": compiled.package_compiled,
               "verify": verify.verify, "sign-ci": ci_sign.sign_ci, "upload": upload.upload,
               "image-save": build.save_image, "image-restore": build.restore_image,
               "test": test}
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=actions)
    args = parser.parse_args()
    actions[args.operation]()


def test():
    suite = unittest.defaultTestLoader.discover(str(Path(__file__).parent), pattern="test_*.py")
    if not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful():
        raise RuntimeError("distribution package regression tests failed")


if __name__ == "__main__":
    main()

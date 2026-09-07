"""Exact-version mapping for upstream notices omitted from published crate roots."""

import re

SUPPLEMENTS = {
    "anes-0.1.6": ["packaging/licenses/anes-MIT.txt", "vendor/anes-0.1.6/README.md"],
    "convert_case-0.4.0": ["packaging/licenses/convert-case-MIT.txt"],
    "jni-0.22.4": ["packaging/licenses/jni-MIT.txt"],
    "jni-macros-0.22.4": ["packaging/licenses/jni-MIT.txt"],
    "jni-sys-macros-0.4.1": ["vendor/jni-sys-0.4.1/LICENSE-MIT"],
    "r-efi-6.0.0": ["vendor/r-efi-6.0.0/AUTHORS"],
    "rustls-platform-verifier-android-0.1.1": ["vendor/rustls-platform-verifier-0.7.0/LICENSE-MIT"],
    "verus_builtin-0.0.0-2026-08-09-0044": ["packaging/licenses/verus-MIT.txt"],
    "verus_builtin_macros-0.0.0-2026-08-09-0044": ["packaging/licenses/verus-MIT.txt"],
    "verus_state_machines_macros-0.0.0-2026-08-02-0125": ["packaging/licenses/verus-MIT.txt"],
    "vstd-0.0.0-2026-08-09-0044": ["packaging/licenses/verus-MIT.txt"],
    "winapi-i686-pc-windows-gnu-0.4.0": ["vendor/winapi-0.3.9/LICENSE-MIT"],
    "winapi-x86_64-pc-windows-gnu-0.4.0": ["vendor/winapi-0.3.9/LICENSE-MIT"],
}


def debian_copyright(notices):
    """Use Debian's common Apache license while preserving upstream copyright notices."""
    apache = re.compile(
        r"^[\t ]*Apache License[\t ]*\n(?:[\t ]*\n)*[\t ]*Version 2\.0, January 2004.*?"
        r"^[\t ]*END OF TERMS AND CONDITIONS[\t ]*$", re.MULTILINE | re.DOTALL,
    )
    text = apache.sub(
        "On Debian systems the complete Apache License, Version 2.0 is in\n"
        "/usr/share/common-licenses/Apache-2.0.\n", notices,
    )
    return text + (
        "\nDebian common-license references:\n"
        "The GNU Lesser General Public License, version 2.1 is available in\n"
        "/usr/share/common-licenses/LGPL-2.1; version 3 is available in\n"
        "/usr/share/common-licenses/LGPL-3.\n"
        "The GNU General Public License versions 2 and 3 are available in\n"
        "/usr/share/common-licenses/GPL-2 and /usr/share/common-licenses/GPL-3.\n"
        "Unabridged upstream notices, including common license texts, are also\n"
        "installed in /usr/share/doc/peritus/THIRD-PARTY-NOTICES.txt.gz.\n"
    )

#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then echo "usage: Upgrade-Peritus.sh <absolute-package-directory>" >&2; exit 2; fi
bundle=$1
"$bundle/Install-Peritus.sh" "$bundle"

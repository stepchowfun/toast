#!/usr/bin/env bash
set -euo pipefail

"$TOAST" --read-local-cache false --write-local-cache false
grep Hello output.txt
rm output.txt

#!/usr/bin/env bash
set -euo pipefail

"$TOAST"
grep 'Hello' output.txt
rm output.txt

#!/usr/bin/env bash
set -euo pipefail

"$TOAST"
cat output.txt | grep 'Hello'
rm output.txt

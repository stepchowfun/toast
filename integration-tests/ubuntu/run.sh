#!/usr/bin/env bash
set -euo pipefail

"$BAKE"
cat output.txt | grep 'Hello'
rm output.txt

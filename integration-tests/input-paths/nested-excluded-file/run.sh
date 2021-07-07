#!/usr/bin/env bash
set -euo pipefail

"$TOAST" --read-local-cache false --write-local-cache false > output.txt
grep 'drwxrwxrwx .* root root .* foo' output.txt
(grep 'bar\.txt' output.txt && exit 1) || true
rm output.txt

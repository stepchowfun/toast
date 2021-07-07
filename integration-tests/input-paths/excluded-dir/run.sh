#!/usr/bin/env bash
set -euo pipefail

mkdir foo
mkdir bar
"$TOAST" --read-local-cache false --write-local-cache false > output.txt
grep 'drwxrwxrwx .* root root .* foo' output.txt
(grep bar output.txt && exit 1) || true
rm output.txt
rm -r foo
rm -r bar

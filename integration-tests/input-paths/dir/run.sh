#!/usr/bin/env bash
set -euxo pipefail

mkdir foo
"$TOAST" --read-local-cache false --write-local-cache false > output.txt
grep 'drwxrwxrwx .* root root .* foo' output.txt
rm output.txt
rm -r foo

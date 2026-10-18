#!/usr/bin/env bash
set -euxo pipefail

mkdir -p foo/bar
"$TOAST" --read-local-cache false --write-local-cache false > output.txt
grep 'drwxrwxrwx .* root root .* foo' output.txt
grep 'drwxrwxrwx .* root root .* bar' output.txt
rm output.txt
rm -rf foo

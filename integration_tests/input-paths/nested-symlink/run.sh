#!/usr/bin/env bash
set -euxo pipefail

mkdir foo
ln -s non-existent foo/symlink
"$TOAST" --read-local-cache false --write-local-cache false > output.txt
grep 'drwxrwxrwx .* root root .* foo' output.txt
grep 'lrwxrwxrwx .* root root .* symlink \-> non\-existent' output.txt
rm output.txt
rm -rf foo

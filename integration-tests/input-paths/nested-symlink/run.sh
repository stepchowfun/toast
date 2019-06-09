#!/usr/bin/env bash
set -euo pipefail

"$TOAST" --read-local-cache false --write-local-cache false > output.txt
grep 'drwxrwxrwx .* root root .* foo' output.txt
grep 'lrwxrwxrwx .* root root .* symlink \-> non\-existent' output.txt
rm output.txt

#!/usr/bin/env bash
set -euo pipefail

ln -s non-existent symlink
"$TOAST" --read-local-cache false --write-local-cache false > output.txt
grep 'lrwxrwxrwx .* root root .* symlink \-> non\-existent' output.txt
rm output.txt symlink

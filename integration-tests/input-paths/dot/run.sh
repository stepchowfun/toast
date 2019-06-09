#!/usr/bin/env bash
set -euo pipefail

ln -s non-existent symlink
"$TOAST" --read-local-cache false --write-local-cache false > output.txt
grep 'drwxrwxrwx .* root root .* foo' output.txt
grep '\-rw\-rw\-rw\- .* root root .* bar\.txt' output.txt
grep '\-rw\-rw\-rw\- .* root root .* baz\.txt' output.txt
grep 'lrwxrwxrwx .* root root .* symlink \-> non\-existent' output.txt
rm output.txt symlink

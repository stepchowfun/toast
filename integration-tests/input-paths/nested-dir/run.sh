#!/usr/bin/env bash
set -euo pipefail

"$TOAST" --read-local-cache false --write-local-cache false > output.txt
grep 'drwxrwxrwx .* root root .* foo' output.txt
grep 'drwxrwxrwx .* root root .* bar' output.txt
grep '\-rw\-rw\-rw\- .* root root .* baz\.txt' output.txt
rm output.txt

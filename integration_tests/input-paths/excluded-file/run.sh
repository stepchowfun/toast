#!/usr/bin/env bash
set -euxo pipefail

"$TOAST" --read-local-cache false --write-local-cache false > output.txt
grep '\-rw\-rw\-rw\- .* root root .* foo\.txt' output.txt
(grep 'bar\.txt' output.txt && exit 1) || true
rm output.txt

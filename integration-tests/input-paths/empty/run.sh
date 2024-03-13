#!/usr/bin/env bash
set -euxo pipefail

"$TOAST" --read-local-cache false --write-local-cache false > output.txt
grep 'drwxrwxrwx .* root root .* \.' output.txt
rm output.txt

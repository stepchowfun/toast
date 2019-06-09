#!/usr/bin/env bash
set -euo pipefail

"$TOAST" --read-local-cache false --write-local-cache false

# shellcheck disable=SC2010
ls -al | grep '^\-rw\-r\-\-r\-\-.* foo\.txt'

rm foo.txt

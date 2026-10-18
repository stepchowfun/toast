#!/usr/bin/env bash
set -euxo pipefail

"$TOAST" --read-local-cache false --write-local-cache false

# shellcheck disable=SC2010
ls -al | grep '^\-rw\-r\-\-r\-\-.* foo\.txt'

rm foo.txt

#!/usr/bin/env bash
set -euxo pipefail

"$TOAST" --read-local-cache false --write-local-cache false

# shellcheck disable=SC2010
ls -al | grep '^drwxr.xr.x.* foo'

# shellcheck disable=SC2010
ls -al foo | grep '^lrwxr.xr.x.* symlink \-> non\-existent'

rm -rf foo

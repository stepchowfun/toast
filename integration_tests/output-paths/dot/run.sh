#!/usr/bin/env bash
set -euxo pipefail

"$TOAST" --read-local-cache false --write-local-cache false

# shellcheck disable=SC2010
ls -al | grep '^drwxr.xr.x.* foo'

# shellcheck disable=SC2010
ls -al foo | grep '^\-rw\-r\-\-r\-\-.* bar\.txt'

# shellcheck disable=SC2010
ls -al | grep '^\-rw\-r\-\-r\-\-.* baz\.txt'

# shellcheck disable=SC2010
ls -al | grep '^lrwxr.xr.x.* symlink \-> non\-existent'

rm -rf foo
rm baz.txt symlink

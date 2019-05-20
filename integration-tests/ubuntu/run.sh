#!/usr/bin/env bash
set -euo pipefail

bake
cat output.txt | grep 'Hello'
rm output.txt

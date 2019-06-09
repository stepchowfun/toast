#!/usr/bin/env bash
set -euo pipefail

# This script runs the integration tests. The `TOAST` environment variable should be set to the
# absolute path of the Toast binary.

# Log the path to the binary.
echo "Toast location: $TOAST"

# Log the version of Docker.
docker --version

# Run the integration tests.
while IFS= read -d '' -r TEST; do
  # Log which integration test we're about to run.
  echo "Running integration test: $TEST"

  # Go into the test directory and run the test.
  (cd "$(dirname "$TEST")" > /dev/null && ./run.sh)
done < <(find integration-tests -name run.sh -print0)

# Inform the user of the good news.
echo 'All integration tests passed.'

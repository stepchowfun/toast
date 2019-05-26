#!/usr/bin/env bash
set -euo pipefail

# This script runs the integration tests. The `TOAST` environment variable should be set to the
# absolute path of the Toast binary.

# Log the path to the binary.
echo "Toast location: $TOAST"

# Log the version of Docker.
docker --version

# Run the integration tests.
# shellcheck disable=SC2045
for TEST in $(ls integration-tests); do
  # Log which integration test we're about to run.
  echo "Running integration test: $TEST"

  # Stop all running Docker containers.
  CONTAINERS="$(docker ps --no-trunc --quiet)"
  if [ -n "$CONTAINERS" ]; then
    # shellcheck disable=SC2086
    docker container stop $CONTAINERS > /dev/null
  fi

  # Delete all Docker objects.
  docker system prune --volumes --all --force > /dev/null

  # Go into the test directory and run the test.
  (cd "./integration-tests/$TEST/" && ./run.sh)
done

# Inform the user of the good news.
echo 'All integration tests passed.'

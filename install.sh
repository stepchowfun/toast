#!/usr/bin/env sh

# Usage examples:
#   ./install.sh
#   VERSION=x.y.z ./install.sh
#   PREFIX=/usr/local/bin ./install.sh

# We wrap everything in parentheses for two reasons:
# 1. To prevent the shell from executing only a prefix of the script if the
#    download is interrupted
# 2. To ensure that any working directory changes with `cd` are local to this
#    script and don't affect the calling user's shell
(
  # Where the binary will be installed
  DESTINATION="${PREFIX:-/usr/local/bin}/toast"

  # Which version to download
  RELEASE="v${VERSION:-0.17.0}"

  # Determine which binary to download.
  FILENAME=''
  CHECKSUM=''
  if uname -a | grep -qi 'x86_64.*GNU/Linux'; then
    echo 'x86_64 GNU/Linux detected.'
    FILENAME=toast-x86_64-unknown-linux-gnu
    CHECKSUM='e60b17ae29adb623d29da5511bc60c32eabb64a25324fe2c29bba8f0db36c7ff'
  fi
  if uname -a | grep -qi 'Darwin.*x86_64'; then
    echo 'macOS detected.'
    FILENAME=toast-x86_64-apple-darwin
    CHECKSUM='2ed56f6c48d7a28b5fbcf58fc6765efb9f502baae42e4ac4863d09d3367eab1a'
  fi

  # Find a temporary location for the binary.
  TEMPDIR=$(mktemp -d /tmp/toast.XXXXXXXX)

  # This is a helper function to clean up and fail.
  fail() {
    echo "$1" >&2
    cd "$TEMPDIR/.." || exit 1
    rm -rf "$TEMPDIR"
    exit 1
  }

  # Enter the temporary directory.
  cd "$TEMPDIR" || fail "Unable to access the temporary directory $TEMPDIR."

  # Fail if there is no pre-built binary for this platform.
  if [ -z "$FILENAME" ]; then
    fail 'Unfortunately, there is no pre-built binary for this platform.'
  fi

  # Download the binary.
  if ! curl "https://github.com/stepchowfun/toast/releases/download/$RELEASE/$FILENAME" -o "$FILENAME" -LSf; then
    fail 'There was an error downloading the binary.'
  fi

  # Download the checksum.
  if ! curl "https://github.com/stepchowfun/toast/releases/download/$RELEASE/$FILENAME.sha256" -o "$FILENAME.sha256" -LSf; then
    fail 'There was an error downloading the checksum.'
  fi

  # Verify the checksum.
  if ! echo "$CHECKSUM *$FILENAME" | sha256sum --check --strict --quiet --status; then
    fail 'The downloaded binary was corrupted. Feel free to try again.'
  fi

  # Make it executable.
  if ! chmod a+rx "$FILENAME"; then
    fail 'There was an error setting the permissions for the binary.'
  fi

  # Install it at the requested destination.
  # shellcheck disable=SC2024
  mv "$FILENAME" "$DESTINATION" 2> /dev/null || sudo mv "$FILENAME" "$DESTINATION" < /dev/tty || fail "Unable to install the binary at $DESTINATION."

  # Remove the temporary directory.
  cd ..
  rm -rf "$TEMPDIR"

  # Let the user know it worked.
  echo "$("$DESTINATION" --version) is now installed."
)

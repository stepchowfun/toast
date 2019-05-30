#!/usr/bin/env sh

# Usage examples:
#   ./install.sh
#   VERSION=x.y.z ./install.sh
#   PREFIX=/usr/local/bin ./install.sh

# We wrap everything in parentheses for two reasons:
# 1. To prevent the shell from executing only a prefix of the script if the download is interrupted
# 2. To ensure that any working directory changes with `cd` are local to this script and don't
#    affect the calling user's shell
(
  # Where the binary will be installed
  DESTINATION="${PREFIX:-/usr/local/bin}/toast"

  # Which version to download
  RELEASE="v${VERSION:-0.21.0}"

  # Determine which binary to download.
  FILENAME=''
  if uname -a | grep -qi 'x86_64.*GNU/Linux'; then
    echo 'x86_64 GNU/Linux detected.'
    FILENAME=toast-x86_64-unknown-linux-gnu
  fi
  if uname -a | grep -qi 'Darwin.*x86_64'; then
    echo 'macOS detected.'
    FILENAME=toast-x86_64-apple-darwin
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
  if ! curl "https://github.com/stepchowfun/toast/releases/download/$RELEASE/$FILENAME" \
      -o "$FILENAME" -LSf; then
    fail 'There was an error downloading the binary.'
  fi

  # Make it executable.
  if ! chmod a+rx "$FILENAME"; then
    fail 'There was an error setting the permissions for the binary.'
  fi

  # Install it at the requested destination.
  # shellcheck disable=SC2024
  mv "$FILENAME" "$DESTINATION" 2> /dev/null ||
    sudo mv "$FILENAME" "$DESTINATION" < /dev/tty ||
    fail "Unable to install the binary at $DESTINATION."

  # Remove the temporary directory.
  cd ..
  rm -rf "$TEMPDIR"

  # Let the user know it worked.
  echo "$("$DESTINATION" --version) is now installed."
)

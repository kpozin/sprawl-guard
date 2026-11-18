#!/bin/sh
# Installs Sprawl Guard with pip and checks that the installed command runs. The release workflow
# runs this inside a musl Python container, where the GitHub Actions steps can't run directly.
#
# Usage: check-wheel-install.sh PIP_INSTALL_ARGUMENT...
#
# INSTALL_ATTEMPTS and INSTALL_RETRY_SECONDS set how often and how far apart pip retries, which
# covers the minutes PyPI can take to serve a new release. SPRAWL_GUARD_VERSION is the version
# that `sprawl-guard --version` must print.
set -eu

attempt=1
until pip install --root-user-action=ignore --disable-pip-version-check --no-cache-dir "$@"; do
  if [ "${attempt}" -ge "${INSTALL_ATTEMPTS}" ]; then
    exit 1
  fi
  attempt=$((attempt + 1))
  sleep "${INSTALL_RETRY_SECONDS}"
done

test "$(sprawl-guard --version 2>&1)" = "sprawl-guard ${SPRAWL_GUARD_VERSION}"
sprawl-guard --help

#!/bin/sh
# The one way CI downloads: retries a flaky network, never a failing test. Usage: download.sh <url> [more curl options]
# --compressed: Antigravity's server sometimes sends its script gzip-compressed (seen 2026-10-04).
exec curl -fsSL --compressed --retry 5 --retry-all-errors "$@"

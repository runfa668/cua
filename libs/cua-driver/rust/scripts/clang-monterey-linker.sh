#!/usr/bin/env bash
set -euo pipefail
CLANG="$(xcrun --find clang)"
ARGS=()
NEED_SYSTEM_FRAMEWORK=0
for arg in "$@"; do
    if [[ "$arg" == "-ldispatch" ]]; then NEED_SYSTEM_FRAMEWORK=1; else ARGS+=("$arg"); fi
done
if [[ "$NEED_SYSTEM_FRAMEWORK" == "1" ]]; then ARGS+=("-framework" "System"); fi
exec "$CLANG" "${ARGS[@]}"

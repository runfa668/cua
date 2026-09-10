#!/usr/bin/env bash
# Monterey linker compatibility wrapper.
# Rewrites legacy `-ldispatch` arguments emitted by transitive Rust crates to
# the supported Monterey umbrella framework form: `-framework System`.
set -euo pipefail

CLANG="$(xcrun --find clang)"
ARGS=()
NEED_SYSTEM_FRAMEWORK=0

for arg in "$@"; do
    if [[ "$arg" == "-ldispatch" ]]; then
        NEED_SYSTEM_FRAMEWORK=1
    else
        ARGS+=("$arg")
    fi
done

if [[ "$NEED_SYSTEM_FRAMEWORK" == "1" ]]; then
    ARGS+=("-framework" "System")
fi

exec "$CLANG" "${ARGS[@]}"

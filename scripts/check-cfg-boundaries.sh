#!/usr/bin/env bash
# Platform-specific code belongs in `platform/` modules only (PLAN.md, section 6).
# Fails if an OS-dependent `cfg` appears anywhere else under `crates/`.
set -euo pipefail

pattern='cfg(_attr)?!?[[:space:]]*\(.*\b(windows|unix|target_os|target_family)\b'

if violations=$(grep -rnE --include='*.rs' "$pattern" crates | grep -v '/platform/'); then
    echo "OS-specific cfg outside platform/ modules:" >&2
    echo "$violations" >&2
    exit 1
fi

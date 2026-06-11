#!/bin/bash
# SessionStart hook — install the Zig 0.15.1 toolchain so Claude Code on the web
# can build this repo and run the (platform-agnostic) kernel tests.
#
# Birnam needs Zig 0.15.1, which isn't preinstalled in the web container and
# ziglang.org is outside the network allowlist. Zig is published on PyPI as the
# `ziglang` package, which IS reachable — install that and expose a `zig` shim.
#
# Web-only (no-op locally). Synchronous so the toolchain is ready before the
# session starts. Idempotent.
set -euo pipefail

# Only run in the remote web environment.
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

ZIG_VERSION="0.15.1"
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"

# Install the Zig compiler from PyPI (idempotent: pip skips if satisfied).
if ! python3 -m ziglang version 2>/dev/null | grep -qx "$ZIG_VERSION"; then
  pip3 install --user --quiet "ziglang==${ZIG_VERSION}"
fi

# Expose a `zig` entrypoint that forwards to the PyPI-installed compiler.
cat > "$BIN_DIR/zig" <<'SHIM'
#!/bin/bash
exec python3 -m ziglang "$@"
SHIM
chmod +x "$BIN_DIR/zig"

# Put the shim on PATH for this session and all subsequent commands.
export PATH="$BIN_DIR:$PATH"
if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  echo "export PATH=\"$BIN_DIR:\$PATH\"" >> "$CLAUDE_ENV_FILE"
fi

echo "[session-start] zig $(zig version) ready (PyPI ziglang)"

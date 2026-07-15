#!/usr/bin/env bash
set -euo pipefail

bin="${BIRNAM_BIN:?BIRNAM_BIN must point to the installed birnam binary}"
tmp="$(mktemp -d /tmp/birnam-lsp-project.XXXXXX)"
tmp="$(cd "$tmp" && pwd -P)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/app/src"
cat >"$tmp/app/birnam.toml" <<'EOF'
name = "app"
source_roots = ["src"]
load = ["missing"]
EOF

export ROOT_URI="file://$tmp/app"
export DOC_URI="$ROOT_URI/src/main.birnam"

# The body contains a two-byte UTF-8 character. `use bytes` is essential:
# Content-Length counts encoded bytes, exactly as LSP requires.
perl -e '
use bytes;
sub sendm { my ($body) = @_; print "Content-Length: ", length($body), "\r\n\r\n", $body; }
my $root = $ENV{ROOT_URI};
my $doc = $ENV{DOC_URI};
my $bad = "[\\\"" . "\xC3\xA9" . "\\\" + ]";
sendm("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"rootUri\":\"$root\"}}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":2,\"text\":\"$bad\"}}}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":1},\"contentChanges\":[{\"text\":\"[1 + 2]\"}]}}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":3},\"contentChanges\":[{\"text\":\"[1 + 2]\"}]}}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didClose\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"}}}");
sendm("{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"shutdown\",\"params\":null}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"exit\",\"params\":null}");
' | "$bin" --lsp >"$tmp/out" 2>"$tmp/err"

grep -q '"id":1' "$tmp/out"
grep -q '"code":"moduleNotFound"' "$tmp/out"
grep -q '"character":7' "$tmp/out"
test "$(grep -o 'textDocument/publishDiagnostics' "$tmp/out" | wc -l | tr -d ' ')" = 3
grep -q '"diagnostics":\[\]' "$tmp/out"
test ! -s "$tmp/err"

echo "lsp-project smoke: workspace, shared diagnostics, UTF-16, versions, and close passed"

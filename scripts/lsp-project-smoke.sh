#!/usr/bin/env bash
set -euo pipefail

bin="${BIRNAM_BIN:?BIRNAM_BIN must point to the installed birnam binary}"
tmp="$(mktemp -d /tmp/birnam-lsp-project.XXXXXX)"
tmp="$(cd "$tmp" && pwd -P)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/app/src" "$tmp/other/src"
cat >"$tmp/app/birnam.toml" <<'EOF'
name = "app"
source_roots = ["src"]
load = ["appmissing"]
EOF
cat >"$tmp/other/birnam.toml" <<'EOF'
name = "other"
source_roots = ["src"]
load = ["othermissing"]
EOF

export ROOT_URI="file://$tmp/app"
export OTHER_ROOT_URI="file://$tmp/other"
export DOC_URI="$OTHER_ROOT_URI/src/main.birnam"
export OTHER_MANIFEST="$tmp/other/birnam.toml"

# The body contains a two-byte UTF-8 character. `use bytes` is essential:
# Content-Length counts encoded bytes, exactly as LSP requires. The bounded
# startup wait lets the server consume the initial open before the fixture is
# changed, so didSave observes a real on-disk transition.
# The body contains a two-byte UTF-8 character. `use bytes` is essential:
# Content-Length counts encoded bytes, exactly as LSP requires. The bounded
# wait lets the server consume the initial open before the fixture is changed.
perl -e '
use bytes;
$| = 1;
sub sendm { my ($body) = @_; print "Content-Length: ", length($body), "\r\n\r\n", $body; }
my $root = $ENV{ROOT_URI};
my $other_root = $ENV{OTHER_ROOT_URI};
my $doc = $ENV{DOC_URI};
my $manifest = $ENV{OTHER_MANIFEST};
my $bad = "[\\\"" . "\xC3\xA9" . "\\\" + ]";
sendm("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"workspaceFolders\":[{\"uri\":\"$root\"},{\"uri\":\"$other_root\"}]}}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":2,\"text\":\"$bad\"}}}");
sleep 5;
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":1},\"contentChanges\":[{\"text\":\"[1 + 2]\"}]}}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":3},\"contentChanges\":[{\"text\":\"[1 + 2]\"}]}}");
open my $fh, ">", $manifest or die "open $manifest: $!";
print {$fh} "name = \"other\"\nsource_roots = [\"src\"]\nload = []\n";
close $fh or die "close $manifest: $!";
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didSave\",\"params\":{\"textDocument\":{\"uri\":\"file://$manifest\"}}}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didClose\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"}}}");
sendm("{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"shutdown\",\"params\":null}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"exit\",\"params\":null}");
' | "$bin" --lsp >"$tmp/out" 2>"$tmp/err"

grep -q '"id":1' "$tmp/out"
grep -q '"code":"moduleNotFound"' "$tmp/out"
grep -q '#othermissing' "$tmp/out"
! grep -q '#appmissing' "$tmp/out"
grep -q '"character":7' "$tmp/out"
test "$(grep -o 'textDocument/publishDiagnostics' "$tmp/out" | wc -l | tr -d ' ')" = 3
grep -q '"diagnostics":\[\]' "$tmp/out"
test ! -s "$tmp/err"

echo "lsp-project smoke: workspace, shared diagnostics, UTF-16, versions, and close passed"

#!/usr/bin/env bash
set -euo pipefail

bin="${BIRNAM_BIN:?BIRNAM_BIN must point to the installed birnam binary}"
tmp="$(mktemp -d /tmp/birnam-lsp-project.XXXXXX)"
tmp="$(cd "$tmp" && pwd -P)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/app/src" "$tmp/other/src" "$tmp/dep/src"
cat >"$tmp/app/birnam.toml" <<'EOF'
name = "app"
source_roots = ["src"]
load = ["appmissing"]
EOF
cat >"$tmp/other/birnam.toml" <<'EOF'
name = "other"
source_roots = ["src"]
load = ["main"]

[dependencies]
dep = { path = "../dep" }
EOF
cat >"$tmp/dep/birnam.toml" <<'EOF'
name = "dep"
source_roots = ["src"]
load = ["dep"]
EOF
cat >"$tmp/dep/src/dep.birnam" <<'EOF'
[Module named: #dep requires: #[]];
[Reflect defineMethod: #dependencyAnswer on: Integer as: [:self | 7]]
EOF
cat >"$tmp/other/src/main.birnam" <<'EOF'
[Module named: #main requires: #[#dep]];
let answer := 1;
[1 dependencyAnswer]
EOF

export ROOT_URI="file://$tmp/app"
export OTHER_ROOT_URI="file://$tmp/other"
export APP_DOC_URI="$ROOT_URI/src/app.birnam"
export DOC_URI="$OTHER_ROOT_URI/src/main.birnam"
export OTHER_MANIFEST="$tmp/other/birnam.toml"
export DEP_URI="file://$tmp/dep/src/dep.birnam"

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
my $app_doc = $ENV{APP_DOC_URI};
my $doc = $ENV{DOC_URI};
my $manifest = $ENV{OTHER_MANIFEST};
my $dep_uri = $ENV{DEP_URI};
my $bad = "[\\\"" . "\xC3\xA9" . "\\\" + ]";
my $arity = "[Reflect defineMethod: #needs: on: Integer as: [:self | 1]]";
my $nav = "let answer := 1; [answer + 1]; [1 dependencyAnswer]; " . $arity;
my $dep_call = index($nav, "dependencyAnswer");
my $answer_use = index($nav, "answer +");
sendm("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"workspaceFolders\":[{\"uri\":\"$root\"},{\"uri\":\"$other_root\"}]}}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{\"textDocument\":{\"uri\":\"$app_doc\",\"version\":1,\"text\":\"[1 + 2]\"}}}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":2,\"text\":\"$bad\"}}}");
sleep 5;
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":1},\"contentChanges\":[{\"text\":\"[1 + 2]\"}]}}");
sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":3},\"contentChanges\":[{\"text\":\"$nav\"}]}}");
sendm("{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"textDocument/documentSymbol\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"}}}");
sendm("{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"textDocument/definition\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$dep_call}}}");
sendm("{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"textDocument/references\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$dep_call},\"context\":{\"includeDeclaration\":true}}}");
sendm("{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"workspace/symbol\",\"params\":{\"query\":\"dependencyAnswer\"}}");
sendm("{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"textDocument/hover\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$dep_call}}}");
sendm("{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"textDocument/completion\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$dep_call}}}");
sendm("{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"textDocument/signatureHelp\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$dep_call}}}");
sendm("{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"textDocument/definition\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$answer_use}}}");
for my $version (4 .. 20) {
  sendm("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":$version},\"contentChanges\":[{\"text\":\"$arity\"}]}}");
}
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
grep -q '#appmissing' "$tmp/out"
grep -q '"character":7' "$tmp/out"
grep -q '"code":"selectorArity"' "$tmp/out"
grep -q '"id":3' "$tmp/out"
grep -q '"name":"answer"' "$tmp/out"
grep -q '"name":"needs:"' "$tmp/out"
grep -q '"id":4' "$tmp/out"
grep -q '"id":5' "$tmp/out"
grep -q '"id":6' "$tmp/out"
grep -q '"id":7' "$tmp/out"
grep -q 'Birnam method' "$tmp/out"
grep -q '"id":8' "$tmp/out"
grep -q 'dependencyAnswer' "$tmp/out"
grep -q '"id":9' "$tmp/out"
grep -q '"id":10' "$tmp/out"
grep -q "$DEP_URI" "$tmp/out"
test "$(grep -o 'textDocument/publishDiagnostics' "$tmp/out" | wc -l | tr -d ' ')" -ge 20
grep -q '"diagnostics":\[\]' "$tmp/out"
test ! -s "$tmp/err"

echo "lsp-project smoke: workspace, shared diagnostics, UTF-16, versions, and close passed"

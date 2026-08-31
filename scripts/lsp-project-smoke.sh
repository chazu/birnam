#!/usr/bin/env bash
set -euo pipefail

bin="${BIRNAM_BIN:?BIRNAM_BIN must point to the installed birnam binary}"
bin="$(cd "$(dirname "$bin")" && pwd -P)/$(basename "$bin")"
tmp="$(mktemp -d /tmp/birnam-lsp-project.XXXXXX)"
tmp="$(cd "$tmp" && pwd -P)"
trap 'rm -rf "$tmp"' EXIT

# LSP must resolve its installed resources from the executable, not from the
# caller's checkout cwd or inherited test overrides.
unset BIRNAM_LIB_DIR BIRNAM_ID_DYLIB BIRNAM_RUST_WORKSPACE_DIR

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
# protocol barriers below wait for the server to publish diagnostics and answer
# navigation before the fixture changes, so didSave observes a real transition.
export BIRNAM_LSP_BIN="$bin"
export BIRNAM_LSP_OUT="$tmp/out"
export BIRNAM_LSP_ERR="$tmp/err"
(
cd /tmp
perl -e '
use bytes;
use IPC::Open3;
use Symbol qw(gensym);

my $child_err = gensym;
my $pid = open3(my $writer, my $reader, $child_err, $ENV{BIRNAM_LSP_BIN}, "--lsp");
select((select($writer), $| = 1)[0]);
open my $out, ">", $ENV{BIRNAM_LSP_OUT} or die "open output: $!";
binmode $out;

my $sendm = sub {
  my ($body) = @_;
  print {$writer} "Content-Length: ", length($body), "\r\n\r\n", $body;
};
my $diagnostic_count = 0;
my $receive = sub {
  my $header = <$reader>;
  return undef unless defined $header;
  $header =~ /^Content-Length: ([0-9]+)\r?\n$/ or die "bad LSP header: $header";
  my $length = $1;
  my $blank = <$reader>;
  defined $blank && $blank =~ /^\r?\n$/ or die "missing LSP header terminator";
  my $body = "";
  while (length($body) < $length) {
    my $read = read($reader, $body, $length - length($body), length($body));
    defined $read && $read > 0 or die "short LSP body";
  }
  print {$out} "Content-Length: $length\r\n\r\n$body";
  $diagnostic_count++ if $body =~ /"method":"textDocument\/publishDiagnostics"/;
  return $body;
};
my $receive_until = sub {
  my ($predicate, $label) = @_;
  for (1 .. 256) {
    my $body = $receive->();
    defined $body or die "LSP exited before $label";
    return if $predicate->($body);
  }
  die "LSP did not reach $label";
};
my $root = $ENV{ROOT_URI};
my $other_root = $ENV{OTHER_ROOT_URI};
my $app_doc = $ENV{APP_DOC_URI};
my $doc = $ENV{DOC_URI};
my $manifest = $ENV{OTHER_MANIFEST};
my $dep_uri = $ENV{DEP_URI};
my $bad = "[\\\"" . "\xC3\xA9" . "\\\" + ]";
my $arity = "[Reflect defineMethod: #needs: on: Integer as: [:self | 1]]";
my $nav = "let answer := 1; [answer + 1]; [1 dependencyAnswer]; [System pri]; " . $arity;
my $dep_call = index($nav, "dependencyAnswer");
my $answer_use = index($nav, "answer +");
my $runtime_call = index($nav, "pri") + 3;
$sendm->("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"workspaceFolders\":[{\"uri\":\"$root\"},{\"uri\":\"$other_root\"}]}}");
$receive_until->(sub { $_[0] =~ /"id":1(?:,|\})/ }, "initialize response");
$sendm->("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{\"textDocument\":{\"uri\":\"$app_doc\",\"version\":1,\"text\":\"[1 + 2]\"}}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":2,\"text\":\"$bad\"}}}");
my ($saw_missing, $saw_utf16) = (0, 0);
$receive_until->(sub {
  my ($body) = @_;
  $saw_missing = 1 if $body =~ /"code":"moduleNotFound"/;
  $saw_utf16 = 1 if $body =~ /"character":7/;
  return $saw_missing && $saw_utf16;
}, "both didOpen diagnostics");
$sendm->("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":1},\"contentChanges\":[{\"text\":\"[1 + 2]\"}]}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":3},\"contentChanges\":[{\"text\":\"$nav\"}]}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"textDocument/documentSymbol\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"}}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"textDocument/definition\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$dep_call}}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"textDocument/references\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$dep_call},\"context\":{\"includeDeclaration\":true}}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"workspace/symbol\",\"params\":{\"query\":\"dependencyAnswer\"}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"textDocument/hover\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$dep_call}}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"textDocument/completion\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$dep_call}}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"textDocument/signatureHelp\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$dep_call}}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"textDocument/definition\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$answer_use}}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"id\":11,\"method\":\"textDocument/completion\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"},\"position\":{\"line\":0,\"character\":$runtime_call}}}");
my %navigation_ids;
$receive_until->(sub {
  my ($body) = @_;
  $navigation_ids{$1} = 1 if $body =~ /"id":(3|4|5|6|7|8|9|10|11)(?:,|\})/;
  return scalar(keys %navigation_ids) == 9;
}, "navigation responses");
for my $version (4 .. 20) {
  $sendm->("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"textDocument\":{\"uri\":\"$doc\",\"version\":$version},\"contentChanges\":[{\"text\":\"$arity\"}]}}");
}
$receive_until->(sub { $diagnostic_count >= 20 }, "versioned diagnostics");
open my $fh, ">", $manifest or die "open $manifest: $!";
print {$fh} "name = \"other\"\nsource_roots = [\"src\"]\nload = []\n";
close $fh or die "close $manifest: $!";
$sendm->("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didSave\",\"params\":{\"textDocument\":{\"uri\":\"file://$manifest\"}}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didClose\",\"params\":{\"textDocument\":{\"uri\":\"$doc\"}}}");
$sendm->("{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"shutdown\",\"params\":null}");
$receive_until->(sub { $_[0] =~ /"id":2(?:,|\})/ }, "shutdown response");
$sendm->("{\"jsonrpc\":\"2.0\",\"method\":\"exit\",\"params\":null}");
close $writer;
while (defined($receive->())) {}
waitpid($pid, 0);
close $out;
my $stderr = do { local $/; <$child_err> };
open my $err_out, ">", $ENV{BIRNAM_LSP_ERR} or die "open stderr output: $!";
print {$err_out} $stderr if defined $stderr;
close $err_out;
exit(($? >> 8) == 0 ? 0 : 1);
'
)

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
if ! grep -q '"id":11.*"label":"println:"' "$tmp/out"; then
  echo "lsp-project smoke: System pri completion omitted println:" >&2
  grep '"id":11' "$tmp/out" >&2 || true
  sed -n '1,80p' "$tmp/err" >&2
  exit 1
fi
grep -q "$DEP_URI" "$tmp/out"
test "$(grep -o 'textDocument/publishDiagnostics' "$tmp/out" | wc -l | tr -d ' ')" -ge 20
grep -q '"diagnostics":\[\]' "$tmp/out"
test ! -s "$tmp/err"

echo "lsp-project smoke: workspace, shared diagnostics, UTF-16, versions, and close passed"

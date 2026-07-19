#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C
export LANG=C

bin="${BIRNAM_BIN:?BIRNAM_BIN must point to the installed birnam binary}"
project="${BIRNAM_DIGLA_PROJECT:?BIRNAM_DIGLA_PROJECT must point to examples/digla}"
tmp="$(mktemp -d /tmp/birnam-digla-session.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

"$bin" build "$project" >/dev/null

# The evaluated writes must become JSON events. Any direct System output would
# leave bytes between frames and make the strict decoder below fail.
perl -MJSON::PP -e '
use bytes;
for my $request (
  {jsonrpc => "2.0", id => 1, method => "birnam/session/open", params => {}},
  {jsonrpc => "2.0", id => 2, method => "birnam/evaluate",
    params => {source => q{[System println: "hello"]; [System print: "there"]; 42}}},
  {jsonrpc => "2.0", id => 3, method => "birnam/evaluate", params => {source =>
    q{let c := ["x" asCell]; [1 to: 19 do: [:i | [c set: [[c value] , [c value]]]]]; [System print: [c value]]; 7}}},
  {jsonrpc => "2.0", id => 4, method => "birnam/close", params => {}}
) {
  my $body = encode_json($request);
  print "Content-Length: ", length($body), "\r\n\r\n", $body;
}
' | "$bin" run "$project" -- --session-stdio >"$tmp/replies"

perl -MJSON::PP -0777 -e '
use bytes;
my $stream = <>;
my @messages;
while ($stream =~ s/^Content-Length: (\d+)\r\n\r\n//) {
  my $length = $1;
  die "truncated Digla frame\n" if length($stream) < $length;
  my $body = substr($stream, 0, $length, "");
  push @messages, decode_json($body);
}
die "unframed Digla stdout bytes\n" if length($stream);
die "expected six Digla messages\n" unless @messages == 6;

my ($open, $output, $evaluated, $large_output, $large_evaluated, $closed) = @messages;
my $capabilities = $open->{result}{capabilities};
die "output events not advertised\n" unless $capabilities->{outputEvents};
die "output notifications not advertised\n" unless $capabilities->{outputNotifications};
die "synchronous output incorrectly advertised as streamed\n" if $capabilities->{streamedOutput};

die "output notification missing\n" unless $output->{method} eq "birnam/output";
die "wrong output request correlation\n" unless $output->{params}{requestId} == 2;
die "captured output text changed\n" unless $output->{params}{event}{text} eq "hello\nthere";
die "output sequence changed\n" unless $output->{params}{event}{sequence} == 1;
die "short output marked truncated\n" if $output->{params}{event}{truncated};
die "evaluation value changed\n" unless $evaluated->{result}{value}{value} == 42;
die "buffered event fallback missing\n" unless @{$evaluated->{result}{events}} == 1;
die "large output notification missing\n" unless $large_output->{method} eq "birnam/output";
my $large_event = $large_output->{params}{event};
die "large output not bounded\n" unless length($large_event->{text}) == 262144;
die "large output not marked truncated\n" unless $large_event->{truncated};
die "large output marker missing\n" unless $large_event->{text} =~ /\[Digla output truncated at 262144 bytes\]\n\z/;
die "large evaluation value changed\n" unless $large_evaluated->{result}{value}{value} == 7;
die "session did not close\n" unless $closed->{result}{closed};
' "$tmp/replies"

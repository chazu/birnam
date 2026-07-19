#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C
export LANG=C

bin="${BIRNAM_BIN:?BIRNAM_BIN must point to the installed birnam binary}"
project="${BIRNAM_DIGLA_PROJECT:?BIRNAM_DIGLA_PROJECT must point to examples/digla}"
tmp="$(mktemp -d /tmp/birnam-digla-session.XXXXXX)"
remote_pid=""
cleanup() {
  if [[ -n "$remote_pid" ]] && kill -0 "$remote_pid" 2>/dev/null; then
    kill "$remote_pid" 2>/dev/null || true
    wait "$remote_pid" 2>/dev/null || true
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT

"$bin" build "$project" >/dev/null

# The evaluated writes must become JSON events. Any direct System output would
# leave bytes between frames and make the strict decoder below fail.
perl -MJSON::PP -e '
use bytes;
for my $request (
  {jsonrpc => "2.0", id => 1, method => "birnam/session/open", params => {}},
  {jsonrpc => "2.0", id => 2, method => "birnam/evaluate",
    params => {source => q{[System println: "hello"]; [System print: "there"]; 42}, timeoutMs => 0}},
  {jsonrpc => "2.0", id => 3, method => "birnam/evaluate", params => {source =>
    q{let c := ["x" asCell]; [1 to: 19 do: [:i | [c set: [[c value] , [c value]]]]]; [System print: [c value]]; 7}, timeoutMs => 0}},
  {jsonrpc => "2.0", id => 4, method => "birnam/evaluate",
    params => {source => q{[[| true] whileTrue: [| nil]]}}},
  {jsonrpc => "2.0", id => 5, method => "birnam/evaluate",
    params => {source => q{[1 to: 9223372036854775807 do: [:i | nil]]}}},
  {jsonrpc => "2.0", id => 6, method => "birnam/close", params => {}}
) {
  my $body = encode_json($request);
  print "Content-Length: ", length($body), "\r\n\r\n", $body;
}
' | "$bin" run "$project" -- --session-stdio --evaluation-timeout-ms=10 >"$tmp/replies" &
session_pid=$!
for _ in $(seq 1 100); do
  if ! kill -0 "$session_pid" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if kill -0 "$session_pid" 2>/dev/null; then
  kill "$session_pid" 2>/dev/null || true
  wait "$session_pid" 2>/dev/null || true
  echo "Digla cooperative deadline did not stop the evaluation" >&2
  exit 1
fi
wait "$session_pid"

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
die "expected eight Digla messages\n" unless @messages == 8;

my ($open, $output, $evaluated, $large_output, $large_evaluated, $timed_out,
  $range_timed_out, $closed) = @messages;
my $capabilities = $open->{result}{capabilities};
die "output events not advertised\n" unless $capabilities->{outputEvents};
die "output notifications not advertised\n" unless $capabilities->{outputNotifications};
die "synchronous output incorrectly advertised as streamed\n" if $capabilities->{streamedOutput};
die "cooperative deadlines not advertised\n" unless $capabilities->{cooperativeDeadline};
die "protocol interrupt incorrectly advertised\n" if $capabilities->{interrupt};

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
die "infinite loop did not time out\n" unless $timed_out->{result}{phase} eq "timeout";
die "timeout flag missing\n" unless $timed_out->{result}{timedOut};
die "timeout duration changed\n" unless $timed_out->{result}{timeoutMs} == 10;
die "integer loop did not time out\n" unless $range_timed_out->{result}{phase} eq "timeout";
die "integer timeout flag missing\n" unless $range_timed_out->{result}{timedOut};
die "session did not close\n" unless $closed->{result}{closed};
' "$tmp/replies"

# Exercise the same lazy TCP adapter used by the terminal editor. The probe
# submits through the terminal-independent editor model so transport output,
# value insertion, state threading, and close semantics all cross the socket.
port="$(perl -MIO::Socket::INET -e '
  my $socket = IO::Socket::INET->new(LocalAddr => "127.0.0.1", LocalPort => 0,
    Listen => 1, Proto => "tcp") or die $!;
  print $socket->sockport;
')"
"$bin" run "$project" -- --session-listen-once --port="$port" >"$tmp/remote-server" 2>&1 &
remote_pid=$!
ready=0
for _ in $(seq 1 100); do
  if grep -q "listening on 127.0.0.1:$port" "$tmp/remote-server" 2>/dev/null; then
    ready=1
    break
  fi
  if ! kill -0 "$remote_pid" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if [[ "$ready" -ne 1 ]]; then
  echo "Digla TCP session server did not become ready" >&2
  sed -n '1,120p' "$tmp/remote-server" >&2
  exit 1
fi

probe="let c := [Digla configAtPath: nil environment: [Map new] args: #[\"--connect=remote\" \"--port=$port\"]]; let s := [Digla editorStateFor: c]; let r := [Digla submitIn: [s at: \"buffer\" put: \"[System println: 7]; [6 * 7]\"]]; let lines := [r at: \"transcript\"]; [System println: [\"REMOTE-LINES:\" , [[lines size] printString]]]; [System println: [\"REMOTE-INPUT:\" , [lines at: 0]]]; [System println: [\"REMOTE-OUTPUT:\" , [lines at: 1]]]; [System println: [\"REMOTE-VALUE:\" , [lines at: 2]]]; let closed := [Digla callSession: \"birnam/close\" params: [Map new] in: [r at: \"session\"]]; [System println: [\"REMOTE-CLOSED:\" , [[[Digla sessionResultOf: closed] at: \"closed\"] printString]]]"
printf '%s\n:quit\n' "$probe" | "$bin" run "$project" --repl=tty -- \
  --connect=remote --port="$port" >"$tmp/remote-client" 2>&1 &
client_pid=$!
for _ in $(seq 1 100); do
  if ! kill -0 "$client_pid" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if kill -0 "$client_pid" 2>/dev/null; then
  kill "$client_pid" 2>/dev/null || true
  wait "$client_pid" 2>/dev/null || true
  echo "Digla TCP editor client did not finish" >&2
  exit 1
fi
wait "$client_pid"

for _ in $(seq 1 100); do
  if ! kill -0 "$remote_pid" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if kill -0 "$remote_pid" 2>/dev/null; then
  echo "Digla TCP session server did not close with the client" >&2
  exit 1
fi
wait "$remote_pid"
remote_pid=""

grep -q '^birnam> REMOTE-LINES:3$' "$tmp/remote-client"
grep -q '^REMOTE-INPUT:> \[System println: 7\]; \[6 \* 7\]$' "$tmp/remote-client"
grep -q '^REMOTE-OUTPUT:7$' "$tmp/remote-client"
grep -q '^REMOTE-VALUE:42$' "$tmp/remote-client"
grep -q '^REMOTE-CLOSED:true$' "$tmp/remote-client"

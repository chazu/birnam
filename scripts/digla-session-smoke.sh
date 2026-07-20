#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C
export LANG=C

bin="${BIRNAM_BIN:?BIRNAM_BIN must point to the installed birnam binary}"
project="${BIRNAM_DIGLA_PROJECT:?BIRNAM_DIGLA_PROJECT must point to examples/digla}"
client_project="${BIRNAM_DIGLA_CLIENT_PROJECT:-$(dirname "$project")/digla-client}"
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

free_port() {
  perl -MIO::Socket::INET -e '
    my $socket = IO::Socket::INET->new(LocalAddr => "127.0.0.1", LocalPort => 0,
      Listen => 1, Proto => "tcp") or die $!;
    print $socket->sockport;
  '
}

"$bin" build "$project" >/dev/null
"$bin" build "$client_project" >/dev/null

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
  {jsonrpc => "2.0", id => 6, method => "birnam/evaluate",
    params => {source => q{[40 +}, timeoutMs => 0}},
  {jsonrpc => "2.0", id => 7, method => "birnam/evaluate", params => {source => q{
    let a := [DiglaAccelerator new]; let b := [DiglaAccelerator new];
    [a beginEvaluationOutputCapture]; [System print: "a1"];
    [b beginEvaluationOutputCapture]; [System print: "b"];
    let be := [b endEvaluationOutputCapture]; [System print: "a2"];
    let ae := [a endEvaluationOutputCapture];
    [[[[ae at: 0] at: "text"] , "|"] , [[be at: 0] at: "text"]]}}},
  {jsonrpc => "2.0", id => 8, method => "birnam/evaluate", params => {source => q{
    let ok := [true asCell];
    [[DiglaAccelerator coreHooksTracked] ifFalse: [| [ok set: false]]];
    [[SystemVt == [Reflect vtableOf: System]] ifFalse: [| [ok set: false]]];
    [[db hasValue: SystemVt attr: #print:] ifFalse: [| [ok set: false]]];
    [[db hasValue: SystemVt attr: #println:] ifFalse: [| [ok set: false]]];
    [[db hasValue: BlockVt attr: #whileTrue:] ifFalse: [| [ok set: false]]];
    [[db hasValue: BlockVt attr: #whileFalse:] ifFalse: [| [ok set: false]]];
    [[db hasValue: IntegerVt attr: #to:do:] ifFalse: [| [ok set: false]]];
    [ok value]}}},
  {jsonrpc => "2.0", id => 9, method => "birnam/evaluate", params => {source => q{
    let diglaVt := [Reflect vtableOf: Digla];
    let noTermbox := [[Digla runtimeNamed: "Termbox"] isNil];
    let noEditor := [[db hasValue: diglaVt attr: #editorStateFor:] not];
    let noBrowser := [[db hasValue: diglaVt attr: #browserStateFor:] not];
    [[noTermbox & noEditor] & noBrowser]}}},
  {jsonrpc => "2.0", id => 10, method => "birnam/close", params => {}}
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
die "expected thirteen Digla messages\n" unless @messages == 13;

my ($open, $first_output, $second_output, $evaluated, $large_output, $large_evaluated, $timed_out,
  $range_timed_out, $compile_failed, $isolated, $tracked, $headless, $closed) = @messages;
my $capabilities = $open->{result}{capabilities};
die "output events not advertised\n" unless $capabilities->{outputEvents};
die "output notifications not advertised\n" unless $capabilities->{outputNotifications};
die "real-time output not advertised\n" unless $capabilities->{streamedOutput};
die "cooperative deadlines not advertised\n" unless $capabilities->{cooperativeDeadline};
die "protocol interrupt incorrectly advertised\n" if $capabilities->{interrupt};

die "first output notification missing\n" unless $first_output->{method} eq "birnam/output";
die "wrong first output request correlation\n" unless $first_output->{params}{requestId} == 2;
die "first output text changed\n" unless $first_output->{params}{event}{text} eq "hello\n";
die "first output sequence changed\n" unless $first_output->{params}{event}{sequence} == 1;
die "first output marked truncated\n" if $first_output->{params}{event}{truncated};
die "second output notification missing\n" unless $second_output->{method} eq "birnam/output";
die "wrong second output request correlation\n" unless $second_output->{params}{requestId} == 2;
die "second output text changed\n" unless $second_output->{params}{event}{text} eq "there";
die "second output sequence changed\n" unless $second_output->{params}{event}{sequence} == 2;
die "second output marked truncated\n" if $second_output->{params}{event}{truncated};
die "evaluation value changed\n" unless $evaluated->{result}{value}{value} == 42;
die "buffered event fallback missing\n" unless @{$evaluated->{result}{events}} == 1;
die "buffered event fallback changed\n" unless $evaluated->{result}{events}[0]{text} eq "hello\nthere";
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
die "compile failure phase missing\n" unless $compile_failed->{result}{phase} eq "compile";
die "compile diagnostic missing\n" unless @{$compile_failed->{result}{diagnostics}} == 1;
die "compile diagnostic kind changed\n"
  unless $compile_failed->{result}{diagnostics}[0]{kind} eq "syntaxError";
die "compile diagnostic byte position changed\n"
  unless $compile_failed->{result}{diagnostics}[0]{start} == 5;
die "evaluation contexts shared output state\n"
  unless $isolated->{result}{value}{value} eq "a1a2|b";
die "Digla core hooks bypassed the Store MOP\n"
  unless $tracked->{result}{value}{value};
die "headless Digla loaded client-only state\n"
  unless $headless->{result}{value}{value};
die "session did not close\n" unless $closed->{result}{closed};
' "$tmp/replies"

# Prove that a captured write crosses TCP before its evaluation completes. A
# buffered-after-eval implementation sees essentially no gap between these two
# frames; the application-local sink delivers the first one before the long loop.
port="$(free_port)"
"$bin" run "$project" -- --session-listen-once --port="$port" >"$tmp/stream-server" 2>&1 &
remote_pid=$!
ready=0
for _ in $(seq 1 100); do
  if grep -q "listening on 127.0.0.1:$port" "$tmp/stream-server" 2>/dev/null; then
    ready=1
    break
  fi
  if ! kill -0 "$remote_pid" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if [[ "$ready" -ne 1 ]]; then
  echo "Digla streaming session server did not become ready" >&2
  exit 1
fi

perl -MIO::Socket::INET -MJSON::PP -MTime::HiRes=time -e '
use bytes;
my $port = shift;
my $socket = IO::Socket::INET->new(PeerAddr => "127.0.0.1", PeerPort => $port,
  Proto => "tcp") or die $!;
sub send_request {
  my ($request) = @_;
  my $body = encode_json($request);
  print $socket "Content-Length: ", length($body), "\r\n\r\n", $body;
}
sub receive_message {
  my $header = "";
  while ($header !~ /\r\n\r\n\z/) {
    my $byte;
    sysread($socket, $byte, 1) == 1 or die "streaming session EOF\n";
    $header .= $byte;
  }
  $header =~ /Content-Length: (\d+)/ or die "streaming response length missing\n";
  my $length = $1;
  my $body = "";
  while (length($body) < $length) {
    my $chunk;
    sysread($socket, $chunk, $length - length($body)) or die "streaming body EOF\n";
    $body .= $chunk;
  }
  decode_json($body);
}
send_request({jsonrpc => "2.0", id => 1, method => "birnam/session/open", params => {}});
my $open = receive_message();
die "streamed output capability missing\n" unless $open->{result}{capabilities}{streamedOutput};
die "cooperative interrupt capability missing\n" unless $open->{result}{capabilities}{interrupt};
die "interrupt contract not marked cooperative\n"
  unless $open->{result}{capabilities}{cooperativeInterrupt};
my $started = time;
send_request({jsonrpc => "2.0", id => 2, method => "birnam/evaluate", params => {
  source => q{[System println: "early"]; [1 to: 250000 do: [:i | nil]]; 42}}});
my $output = receive_message();
my $output_ms = (time - $started) * 1000;
die "first streaming frame is not output\n" unless $output->{method} eq "birnam/output";
die "streaming output text changed\n" unless $output->{params}{event}{text} eq "early\n";
my $response = receive_message();
my $response_ms = (time - $started) * 1000;
die "streaming evaluation value changed\n" unless $response->{result}{value}{value} == 42;
die "output was buffered until evaluation completed\n" unless $response_ms - $output_ms > 100;
printf "output %.1f ms result %.1f ms gap %.1f ms\n",
  $output_ms, $response_ms, $response_ms - $output_ms;
send_request({jsonrpc => "2.0", id => 3, method => "birnam/evaluate", params => {
  source => q{[[| true] whileTrue: [| nil]]}, timeoutMs => 200}});
send_request({jsonrpc => "2.0", id => 4, method => "birnam/interrupt", params => {
  requestId => 999}});
my $stale_interrupt = receive_message();
die "stale interrupt response id changed: " . encode_json($stale_interrupt) . "\n"
  unless $stale_interrupt->{id} == 4;
die "stale request interrupted the active evaluation\n" if $stale_interrupt->{result}{interrupted};
my $stale_evaluation = receive_message();
die "stale-target evaluation response id changed\n" unless $stale_evaluation->{id} == 3;
die "stale target bypassed the evaluation deadline\n"
  unless $stale_evaluation->{result}{phase} eq "timeout";
send_request({jsonrpc => "2.0", id => 5, method => "birnam/evaluate", params => {
  source => q{[[| true] whileTrue: [| nil]]}}});
send_request({jsonrpc => "2.0", id => 6, method => "birnam/interrupt", params => {
  requestId => 5}});
my $interrupt = receive_message();
die "interrupt response id changed\n" unless $interrupt->{id} == 6;
die "active evaluation was not interrupted\n" unless $interrupt->{result}{interrupted};
die "interrupt response lost request correlation\n" unless $interrupt->{result}{requestId} == 5;
my $interrupted = receive_message();
die "interrupted evaluation response id changed\n" unless $interrupted->{id} == 5;
die "evaluation interruption phase missing\n" unless $interrupted->{result}{phase} eq "interrupted";
die "evaluation interruption flag missing\n" unless $interrupted->{result}{interrupted};
send_request({jsonrpc => "2.0", id => 7, method => "birnam/evaluate", params => {
  source => q{[20 + 22]}}});
my $healthy = receive_message();
die "session unusable after interrupt\n" unless $healthy->{result}{value}{value} == 42;
send_request({jsonrpc => "2.0", id => 8, method => "birnam/close", params => {}});
my $closed = receive_message();
die "interrupted session did not close\n" unless $closed->{result}{closed};
' "$port" >"$tmp/stream-timing" &
stream_client_pid=$!
for _ in $(seq 1 100); do
  if ! kill -0 "$stream_client_pid" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if kill -0 "$stream_client_pid" 2>/dev/null; then
  kill "$stream_client_pid" 2>/dev/null || true
  wait "$stream_client_pid" 2>/dev/null || true
  echo "Digla real-time output probe did not finish" >&2
  exit 1
fi
wait "$stream_client_pid"
wait "$remote_pid"
remote_pid=""

# Exercise the same lazy TCP adapter used by the terminal editor. The probe
# submits through the terminal-independent editor model so transport output,
# value insertion, state threading, and close semantics all cross the socket.
port="$(free_port)"
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

probe="let c := [Digla configAtPath: nil environment: [Map new] args: #[\"--port=$port\"]]; let s := [Digla editorStateFor: c]; let switched := [Digla submitIn: [s at: \"buffer\" put: \":connect remote\"]]; [System println: [\"REMOTE-ADAPTER:\" , [[switched at: \"session\"] at: \"adapter\"]]]; let r0 := [Digla submitIn: [switched at: \"buffer\" put: \"[System print: 7]; [System println: 8]; [6 * 7]\"]]; [System println: [\"REMOTE-PENDING:\" , [[[r0 at: \"evaluation\"] notNil] printString]]]; let rc := [r0 asCell]; [[| [[[rc value] at: \"evaluation\"] notNil]] whileTrue: [| [rc set: [Digla advanceEvaluationIn: [rc value]]]]]; let r := [rc value]; let lines := [r at: \"transcript\"]; [System println: [\"REMOTE-LINES:\" , [[lines size] printString]]]; [System println: [\"REMOTE-INPUT:\" , [lines at: 2]]]; [System println: [\"REMOTE-OUTPUT-1:\" , [lines at: 3]]]; [System println: [\"REMOTE-OUTPUT-2:\" , [lines at: 4]]]; [System println: [\"REMOTE-VALUE:\" , [lines at: 5]]]; let i0 := [Digla submitIn: [r at: \"buffer\" put: \"[[| true] whileTrue: [| nil]]\"]]; let ic := [[Digla interruptEvaluationIn: i0] asCell]; [[| [[[ic value] at: \"evaluation\"] notNil]] whileTrue: [| [ic set: [Digla advanceEvaluationIn: [ic value]]]]]; let interrupted := [ic value]; [System println: [\"REMOTE-INTERRUPTED:\" , [interrupted at: \"status\"]]]; let h0 := [Digla submitIn: [interrupted at: \"buffer\" put: \"[20 + 22]\"]]; let hc := [h0 asCell]; [[| [[[hc value] at: \"evaluation\"] notNil]] whileTrue: [| [hc set: [Digla advanceEvaluationIn: [hc value]]]]]; let healthy := [hc value]; [System println: [\"REMOTE-HEALTHY:\" , [[healthy at: \"lastResult\"] printString]]]; let closed := [Digla callSession: \"birnam/close\" params: [Map new] in: [healthy at: \"session\"]]; [System println: [\"REMOTE-CLOSED:\" , [[[Digla sessionResultOf: closed] at: \"closed\"] printString]]]"
{ sed -n '1,$p' scripts/digla-native-conformance.birnam; printf '%s\n:quit\n' "$probe"; } | "$bin" run "$client_project" --repl=tty -- \
  --headless-client --port="$port" >"$tmp/remote-client" 2>&1 &
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
  sed -n '1,160p' "$tmp/remote-client" >&2
  sed -n '1,160p' "$tmp/remote-server" >&2
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
  sed -n '1,200p' "$tmp/remote-client" >&2
  sed -n '1,200p' "$tmp/remote-server" >&2
  exit 1
fi
wait "$remote_pid"
remote_pid=""

if ! grep -q 'NATIVE-CONFORMANCE:true$' "$tmp/remote-client"; then
  echo "Digla native/reference conformance failed" >&2
  sed -n '1,220p' "$tmp/remote-client" >&2
  exit 1
fi
expect_remote() {
  local pattern="$1"
  local description="$2"
  if ! grep -q "$pattern" "$tmp/remote-client"; then
    echo "Digla TCP editor probe missing $description" >&2
    sed -n '1,220p' "$tmp/remote-client" >&2
    exit 1
  fi
}
expect_remote '^birnam> REMOTE-ADAPTER:tcp$' 'remote adapter selection'
expect_remote '^REMOTE-PENDING:true$' 'pending evaluation state'
expect_remote '^REMOTE-LINES:6$' 'transcript line count'
expect_remote '^REMOTE-INPUT:> \[System print: 7\]; \[System println: 8\]; \[6 \* 7\]$' 'input transcript row'
expect_remote '^REMOTE-OUTPUT-1:7$' 'first output row'
expect_remote '^REMOTE-OUTPUT-2:8$' 'second output row'
expect_remote '^REMOTE-VALUE:42$' 'evaluation value row'
expect_remote '^REMOTE-INTERRUPTED:interrupted$' 'interrupt status'
expect_remote '^REMOTE-HEALTHY:42$' 'post-interrupt evaluation'
expect_remote '^REMOTE-CLOSED:true$' 'session close result'

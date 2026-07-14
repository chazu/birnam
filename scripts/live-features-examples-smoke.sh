#!/usr/bin/env bash
set -euo pipefail

isolate_output=$(bash examples/isolates/run.sh)
grep -q 'replacement status: #replaced' <<<"$isolate_output"
grep -q '#version -> 2' <<<"$isolate_output"

live_output=$(bash examples/live-modules/run.sh)
grep -q 'rollout outcome: #activated' <<<"$live_output"
grep -q 'rollout digest changed: true' <<<"$live_output"
grep -q '#version -> 2' <<<"$live_output"
grep -q 'rollback outcome: #rolledBack' <<<"$live_output"
grep -q '#version -> 1' <<<"$live_output"

printf 'live-features-examples-smoke: I1b replacement and L2 rollout examples passed\n'

# Multicore prime counting

See the [concurrency guide](../../docs/guide/07-concurrency.md) for the task and
worker API, cancellation, outcomes, and shutdown.

From the repository root:

```sh
birnam examples/multicore/main.birnam
```

For a longer run, change `limit: 10000` in `main.birnam` to `limit: 100000`.

Requires a current Birnam installation on `PATH` with the authored Concurrency helper
(currently macOS/arm64). To build and install the current release in the normal prefix:

```sh
mise exec -- bash scripts/rust-install.sh install --root "$PWD" \
  --prefix "$HOME/.local/opt/birnam" --profile release \
  --build-cache "$PWD/.cargo-target/install"
```

The program counts primes through 10,000 by default (1,229 primes). It runs the
same fixed workload twice: first waiting for each worker sequentially, then
submitting both requests before awaiting either. Each worker counts alternating
odd candidates, keeping the work roughly balanced. The root domain also runs a
local task during the parallel phase.

- [worker.birnam](worker.birnam) contains the CPU work: trial division in Birnam.
- [main.birnam](main.birnam) prepares the source modules with `Concurrency load:`,
  runs the demonstration, and closes the program. The library owns the worker
  image and temporary directory.
- [coordinator.birnam](coordinator.birnam) contains sequential requests and parallel structured
  tasks using `worker request:timeout:`.
- [platform.birnam](platform.birnam) provides clock operations through `Alien`.
  All setup, coordination, and reporting are Birnam.

The output compares elapsed time and verifies that both runs return the same
count. Combined worker CPU time divided by parallel elapsed time measures average
CPU usage: a value above 1 demonstrates actual multicore execution. Speedup
depends on available cores and other system load; small inputs are dominated by
transport and scheduling costs. Run order, heap collection, and CPU scheduling
also affect these single-run measurements; parallel execution can take longer.
Setup and worker startup are outside the timings.

These are separate process domains with copied messages and independent heaps.
The workers implement ordinary `handle:` methods, each running in its own request
scope. They shut down after the demonstration. See
[task services and program setup](../../docs/design/80y-task-services-and-program-setup.md)
for cancellation and shutdown behavior.

# Multicore prime counting

From the repository root:

```sh
python3 examples/multicore/run.py
```

For a longer run:

```sh
python3 examples/multicore/run.py 100000
```

Requires a current Birnam installation on `PATH`, Python 3, and a supported
64-bit Unix host (macOS or Linux). `BIRNAM_BIN=/path/to/birnam` selects another
installation. To build and install the current release in the normal prefix:

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
- [main.birnam](main.birnam) contains sequential requests and parallel structured
  tasks using `TaskControl request:from:timeout:`.
- [run.py](run.py) prepares a temporary image, selects resumable execution for
  the coordinator, and formats the measurements. It removes its temporary files
  on exit. The counting runs entirely in Birnam worker processes.

The output compares elapsed time and verifies that both runs return the same
count. Combined worker CPU time divided by parallel elapsed time measures average
CPU usage: a value above 1 demonstrates actual multicore execution. Speedup
depends on available cores and other system load; small inputs are dominated by
transport and scheduling costs. Run order, heap collection, and CPU scheduling
also affect these single-run measurements; parallel execution can take longer.
Setup and worker startup are outside the timings.

These are separate process domains with copied messages and independent heaps.
The workers exit after the demonstration.

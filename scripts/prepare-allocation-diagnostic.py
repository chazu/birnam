#!/usr/bin/env python3
"""Create an isolated, instrumented copy of committed sources for Design 66.

Never install this runtime as the normal runtime or use its timings. All probes
are source overlays in a fresh archive; the production tree stays unchanged.
Build the printed root with rust-install.sh, then set BIRNAM_D66=1 only for the
diagnostic workload. stderr D66 records delimit explicit GC boundaries and
count generated Machine operands, Integers, Blocks and selected Rust scratch.
"""
import argparse
from pathlib import Path
import subprocess


def replace(root, name, old, new, count=1):
    path = root / name
    text = path.read_text()
    if text.count(old) != count:
        raise RuntimeError(f"overlay drift: {name}: expected {count} matches for {old!r}")
    path.write_text(text.replace(old, new))


def event(label, expression="1"):
    return ('if std::env::var_os("BIRNAM_D66").is_some() { '
            f'eprintln!("D66 {label} {{}}", {expression});' + ' }\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    root = args.destination.resolve()
    root.mkdir(parents=True, exist_ok=False)
    repo = Path(__file__).resolve().parent.parent
    archive = subprocess.Popen(["git", "archive", "HEAD"], cwd=repo, stdout=subprocess.PIPE)
    subprocess.run(["tar", "-x", "-C", str(root)], stdin=archive.stdout, check=True)
    archive.stdout.close()
    if archive.wait():
        raise RuntimeError("git archive failed")
    runtime = "rust/birnam-runtime/src/runtime.rs"
    execution = "rust/birnam-runtime/src/execution.rs"
    replace(root, runtime, "pub(crate) fn make_integer(&self, value: i64) -> Oop {",
            "pub(crate) fn make_integer(&self, value: i64) -> Oop {\n" + event("integer"))
    replace(root, runtime, "pub(crate) fn gc_collect(&self) -> usize {",
            "pub(crate) fn gc_collect(&self) -> usize {\n" + event("gc") +
            event("arena", "unsafe { (&*self.state.get()).execution.runtime_diagnostics().reserved_arena_bytes }"))
    needle = "let mut captures = Vec::with_capacity(capture_count);"
    replace(root, runtime, needle, event("capture_words", "capture_count") + needle)
    needle = "let mut arguments = Vec::with_capacity(argument_count);"
    replace(root, execution, needle, event("argument_words", "argument_count") + needle)
    needle = "        initial_oops.push((entry.receiver_slot, receiver));"
    replace(root, execution, needle, event("initial_pairs", "initial_oops.capacity()") + needle)
    codegen = "rust/birnam-codegen/src/lib.rs"
    needle = "#define MKV(x) (birnam_mkSmallInt((long)(x)))"
    replace(root, codegen, needle, needle + '''
extern char* getenv(const char*);
extern long write(int, const void*, unsigned long);
#define D66_MKO(x) ((getenv("BIRNAM_D66") ? (void)write(2,"D66 operand 1\\n",14) : (void)0), MKV(x))''')
    replace(root, "rust/birnam-codegen/src/resumable_c.rs", "MKV(", "D66_MKO(", count=3)
    portable = "rust/birnam-codegen-runtime/src/lib.rs"
    path = root / portable
    text = path.read_text()
    for name in ("birnam_rust_codegen_portable_block_new_v1",
                 "birnam_rust_codegen_portable_materialized_block_new_v2"):
        start = text.index('pub unsafe extern "C" fn ' + name + '(')
        brace = text.index("{", start)
        text = text[:brace + 1] + "\n" + event("block") + text[brace + 1:]
    path.write_text(text)
    print(root)


if __name__ == "__main__":
    main()

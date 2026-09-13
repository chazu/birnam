"""Real Id loop fixtures and differential assertions, shared by cost runners."""
import re


def source(operations, shape, precise=1):
    assert operations in (3, 16, 32) and shape in range(4) and precise in (0, 1)
    chain = "let first := [a + a]; let answer := [first + b]; "
    chain += " ".join("let answer := [answer + a];" for _ in range(operations - 3))
    chain += " [answer + a]"
    setup = f'''[System load: "modules/os.birnam"];
[System preciseGc: {'true' if precise else 'false'}];
let numericSource := [:a :b | {chain}];
let numeric := [Codegen materializeResumable: [[numericSource codeObject] withoutSourceProvenance] captures: #[]];
'''
    if shape == 3:
        body = '''let runSource := [:numeric |
  let result := [nil asCell];
  [128 timesRepeat: [| [result set: [numeric value: 11 value: 3]]]];
  [result value]
];
let run := [Codegen materializeResumable: [[runSource codeObject] withoutSourceProvenance] captures: #[]];
'''
    else:
        a, b = [("11", "3"), ("11", "index"), ("index", "index")][shape]
        body = f'''let stop := [:again :calc :index :previous | previous];
let loopSource := [:again :calc :index :previous |
  let result := [calc value: {a} value: {b}];
  [[result == previous] ifTrue: [| [OS exit: 1]] ifFalse: [| nil]];
  let next := [[index < 127] ifTrue: [| again] ifFalse: [| stop]];
  [next value: again value: calc value: [index + 1] value: result]
];
let loop := [Codegen materializeResumable: [[loopSource codeObject] withoutSourceProvenance] captures: #[stop]];
let runSource := [:numeric | [loop value: loop value: numeric value: 0 value: nil]];
let run := [Codegen materializeResumable: [[runSource codeObject] withoutSourceProvenance] captures: #[loop]];
'''
    return setup + body + f'''let batches := [[System getEnv: "BIRNAM_LOOP_BATCHES"] asInteger];
[UnboxedNumericProbe measureLoop: run numeric: numeric operations: {operations} shape: {shape} batches: batches];
'''


def parse_output(output):
    rows = [s for s in output.splitlines() if s.startswith("loop-measure ")]
    assert len(rows) == 1, output
    return {"measure": {k: int(v) for k, v in re.findall(r"(\w+)=(-?\d+)", rows[0])}}


def check_pair(off, on, batches, unboxed):
    a, b = off["measure"], on["measure"]
    shape, operations = a["shape"], a["operations"]
    assert (b["shape"], b["operations"]) == (shape, operations)
    assert a["iterations"] == b["iterations"] == batches * 128
    assert a["answer"] == b["answer"] == [11 * operations + 3, 11 * operations + 127, (operations + 1) * 127, 11 * operations + 3][shape]
    for key in ("allocation_bytes", "physical_saved", "completed", "fused", "native_units", "numeric_units"):
        assert a[key] == b[key], (shape, unboxed, key, a, b)
    assert a["allocation_bytes"] > 0
    assert a["reused"] == 0
    assert b["reused"] == [operations * 127, 127, 0, 0][shape] * batches
    assert a["owners"] == a["cached"] == a["cache_bytes"] == 0
    assert 0 <= b["cached"] <= b["owners"] and b["owners"] > 0 and b["cache_bytes"] > 0
    assert b["cached"] <= 1
    # Independent calls may retain a stamp but must not reuse its predictions.
    for row in (a, b):
        assert row["completed"] == (128 * operations * batches if unboxed else 0), row
        assert row["fused"] == (0 if unboxed else 128 * operations * batches), row
        assert row["physical_saved"] == (128 * (operations - 1) * batches if unboxed else 0), row
        assert row["cold_ns"] > 0 and row["ns"] > 0 and row["gc_ns"] > 0
        assert row["journal_retained_bytes"] <= row["journal_peak_bytes"]

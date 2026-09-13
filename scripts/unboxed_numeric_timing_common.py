"""Shared numeric cost fixtures and assertions; no timing qualification here."""
import re


def source(operations):
    assert operations in (3, 16, 32)
    bodies = []
    for shape in range(5):
        statements = []
        for index in range(operations):
            receiver = "a" if index == 0 else "answer"
            send = f"[{receiver} + b]"
            if shape == 4 and index == 0:
                statements += [f"let first := {send};", "let answer := first;"]
            elif shape < 3 and index + 1 == operations:
                statements.append(send)
            else:
                statements.append(f"let answer := {send};")
        if shape == 3:
            statements.append("[answer asCell]")
        if shape == 4:
            statements.append("first")
        bodies.append("[:a :b | " + " ".join(statements) + "]")
    return ('[System load: "modules/os.birnam"];\n[System preciseGc: true];\n'
            'let shape := [[System getEnv: "BIRNAM_NUMERIC_SHAPE"] asInteger];\n'
            'let batches := [[System getEnv: "BIRNAM_NUMERIC_BATCHES"] asInteger];\n'
            'let sources := #[\n' + '\n'.join(bodies) + '\n];\n'
            'let source := [sources at: shape];\n'
            'let block := [UnboxedNumericProbe prepare: [[source codeObject] withoutSourceProvenance] captures: #[]];\n'
            'let input := [[shape = 2] ifTrue: [| 11.5] ifFalse: [| 11]];\n'
            f'[UnboxedNumericProbe measure: block input: input other: 3 operations: {operations} shape: shape batches: batches];\n')


def parse_output(output):
    result = {}
    for prefix, key in (("unboxed-prepare ", "prepare"), ("unboxed-measure ", "measure")):
        rows = [s for s in output.splitlines() if s.startswith(prefix)]
        assert len(rows) == 1, (prefix, output)
        result[key] = {k: int(v) for k, v in re.findall(r"(\w+)=(-?\d+)", rows[0])}
    return result


def check_pair(off, on, batches):
    a, b = off["measure"], on["measure"]
    shape, operations = a["shape"], a["operations"]
    assert (b["shape"], b["operations"]) == (shape, operations)
    calls = batches * 128
    assert a["calls"] == b["calls"] == calls
    assert a["allocation_bytes"] == b["allocation_bytes"] > 0
    assert a["completed"] == a["materialized"] == a["physical_saved"] == 0
    assert b["completed"] == (calls * operations if shape in (0, 4) else 0)
    assert b["physical_saved"] == (calls * (operations - 1) if shape in (0, 4) else 0)
    assert b["materialized"] == (calls if shape == 4 else 0)
    assert a["restored_completed"] == 0
    assert b["restored_completed"] == (operations if shape == 1 else 0)
    assert a["native_units"] == b["native_units"]
    assert a["numeric_units"] == b["numeric_units"]
    assert a["maps"] == a["metadata_bytes"] == 0
    assert a["scratch_cached"] == a["scratch_bytes"] == 0
    if shape in (0, 1, 4):
        assert b["scratch_cached"] == 1 and 0 < b["scratch_bytes"] < 8192
    else:
        assert b["scratch_cached"] == b["scratch_bytes"] == 0
    assert (b["maps"] > 0 and b["metadata_bytes"] > 0) if shape != 3 else b["maps"] == b["metadata_bytes"] == 0
    x, y = off["prepare"], on["prepare"]
    assert x["attempts"] == x["preparations"] == x["preparation_ns"] == 0
    assert x["metadata_before"] == x["metadata_after"] == x["maps_before"] == x["maps_after"] == 0
    assert y["attempts"] > 0 and y["preparation_ns"] > 0
    if shape == 3:
        assert y["preparations"] == 0 and y["metadata_before"] == y["metadata_after"]
    else:
        assert y["preparations"] > 0 and y["metadata_after"] > y["metadata_before"]
    for row in (a, b):
        assert row["cold_ns"] > 0 and row["ns"] > 0 and row["gc_ns"] > 0
        assert row["journal_retained_bytes"] <= row["journal_peak_bytes"]


def peak_rss(output):
    """Darwin time -l reports peak resident bytes for the measured child."""
    rows = re.findall(r"^\s*(\d+)\s+maximum resident set size\s*$", output, re.MULTILINE)
    if len(rows) != 1 or int(rows[0]) <= 0:
        raise RuntimeError("missing Darwin peak RSS measurement")
    return int(rows[0])

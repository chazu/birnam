"""Parse and validate the source closure measurement probe independently of timing."""
import re


def source(uses, *, numeric=False):
    """Static source bodies keep each repeated call visible to canonical lowering."""
    assert 1 <= uses <= 32
    bodies = []
    for shape in range(4):
        callee = "[| [input value]]" if shape == 3 else "[| input]"
        if numeric and shape != 3:
            callee = "[| let first := [20 + 15]; let second := [first + 7]; input]"
        statements = [f"let closure := {callee};"]
        statements += ["let answer := [closure value];"] * uses
        statements.append("closure" if shape in (1, 2) else "answer")
        bodies.append("[:input | " + " ".join(statements) + "]")
    return ('[System load: "modules/os.birnam"];\n[System preciseGc: true];\n'
            'let shape := [[System getEnv: "BIRNAM_CLOSURE_SHAPE"] asInteger];\n'
            'let batches := [[System getEnv: "BIRNAM_CLOSURE_BATCHES"] asInteger];\n'
            'let sources := #[\n' + '\n'.join(bodies) + '\n];\n'
            'let source := [sources at: shape];\n'
            'let block := [ClosureControlProbe prepare: [[source codeObject] withoutSourceProvenance] captures: #[]];\n'
            'let input := [73 asCell];\n'
            '[ClosureControlProbe measure: block input: input shape: shape batches: batches];\n')


def parse_output(output):
    result = {}
    for prefix, key in (("closure-prepare ", "prepare"), ("closure-measure ", "measure")):
        rows = [line for line in output.splitlines() if line.startswith(prefix)]
        assert len(rows) == 1, (prefix, output)
        result[key] = {name: int(value) for name, value in re.findall(r"(\w+)=(\d+)", rows[0])}
    return result


def check_pair(off, on, batches, *, numeric=False):
    calls = batches * 128
    a, b = off["measure"], on["measure"]
    for row in (a, b):
        assert row["private_native_units"] >= 0
        assert (row["private_relocated_bytes"] > 0) == (row["private_native_units"] > 0)
        assert row["private_relocated_bytes"] >= row["private_native_units"]
    assert a["shape"] == b["shape"]
    shape = a["shape"]
    uses = a["uses"]
    assert b["uses"] == uses and 1 <= uses <= 32
    assert a["calls"] == b["calls"] == calls
    assert a["constructor_discoveries"] == b["constructor_discoveries"] == 0
    assert a["allocation_bytes"] == b["allocation_bytes"] > 0
    assert a["attempts"] == a["hits"] == a["materialized"] == a["physical_saved"] == 0
    assert b["attempts"] == (calls if shape < 3 else 0)
    assert a["restored_hits"] == 0
    assert b["restored_hits"] == (1 if shape == 2 else 0)
    assert a["constructor_queries"] == 0
    assert b["constructor_queries"] == (calls if shape < 2 else 0)
    assert b["hits"] == (calls if shape < 2 else 0)
    assert b["materialized"] == (calls if shape == 1 else 0)
    assert b["physical_saved"] == (calls if shape == 0 else 0)
    assert a["completed_calls"] == 0
    assert b["completed_calls"] == (calls * uses if shape < 2 else 0)
    assert a["entry_prediction_kernels"] == 0
    assert b["entry_prediction_kernels"] == (calls if numeric and shape < 2 else 0)
    assert a["effects"] == b["effects"] == (calls * uses if shape == 2 else 0)
    assert off["prepare"]["floor_preparations"] == off["prepare"]["floor_preparation_ns"] == 0
    assert off["prepare"]["metadata_payload_before"] == off["prepare"]["metadata_payload_after"] == 0
    assert on["prepare"]["floor_preparations"] == on["prepare"]["floor_preparation_ns"] == 0
    before, after = (on["prepare"][name] for name in ("metadata_payload_before", "metadata_payload_after"))
    assert after > before
    assert a["cold_floor_preparations"] == 0
    assert b["cold_floor_preparations"] == (1 if shape < 3 else 0)
    for key in ("cold_pending_bytes_before", "cold_pending_bytes_after", "pending_bytes"):
        assert a[key] == 0 and 0 <= b[key] <= 4 * 1024 * 1024
    assert on["prepare"]["pending_bytes_after"] > on["prepare"]["pending_bytes_before"]
    assert b["cold_pending_bytes_after"] < b["cold_pending_bytes_before"] if shape < 3 else b["cold_pending_bytes_after"] == b["cold_pending_bytes_before"]
    assert b["cold_pending_records_before"] - b["cold_pending_records_after"] == (1 if shape < 3 else 0)
    for row in (a, b):
        assert row["cold_ns"] > 0 and row["ns"] > 0 and row["gc_ns"] > 0

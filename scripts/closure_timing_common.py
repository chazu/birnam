"""Parse and validate the source closure measurement probe independently of timing."""
import re


def parse_output(output):
    result = {}
    for prefix, key in (("closure-prepare ", "prepare"), ("closure-measure ", "measure")):
        rows = [line for line in output.splitlines() if line.startswith(prefix)]
        assert len(rows) == 1, (prefix, output)
        result[key] = {name: int(value) for name, value in re.findall(r"(\w+)=(\d+)", rows[0])}
    return result


def check_pair(off, on, batches):
    calls = batches * 128
    a, b = off["measure"], on["measure"]
    assert a["shape"] == b["shape"]
    shape = a["shape"]
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
    assert a["effects"] == b["effects"] == (calls if shape == 2 else 0)
    assert off["prepare"]["floor_preparations"] == off["prepare"]["floor_preparation_ns"] == 0
    assert off["prepare"]["metadata_payload_before"] == off["prepare"]["metadata_payload_after"] == 0
    assert on["prepare"]["floor_preparations"] > 0
    before, after = (on["prepare"][name] for name in ("metadata_payload_before", "metadata_payload_after"))
    assert after > before if shape < 3 else after == before
    for row in (a, b):
        assert row["cold_ns"] > 0 and row["ns"] > 0 and row["gc_ns"] > 0

#!/usr/bin/env python3
"""Reduce Design 60H longevity epoch evidence into one plateau verdict."""

from __future__ import annotations

import json
import pathlib
import sys


def bounded(
    values: list[int], spread_slack: int, name: str, age_drift_slack: int | None = None
) -> None:
    assert max(values) - min(values) <= spread_slack, (name, values, spread_slack)
    if len(values) >= 4:
        drift_slack = spread_slack if age_drift_slack is None else age_drift_slack
        assert abs(values[-1] - values[0]) <= drift_slack, (
            name,
            values,
            drift_slack,
        )


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: check-stackless-longevity.py EVIDENCE_DIR EPOCHS")
    root = pathlib.Path(sys.argv[1])
    count = int(sys.argv[2])
    epochs = []
    for epoch in range(1, count + 1):
        with (root / f"epoch-{epoch:03d}.json").open(encoding="utf-8") as handle:
            epochs.append(json.load(handle))

    for expected, evidence in enumerate(epochs, 1):
        assert evidence["epoch"] == expected
        assert evidence["recovery_mode"] == (
            "mapped" if expected % 2 else "rebuild"
        )
        controls = [row[0] for row in evidence["initial"]["CONTROL"]]
        assert len(controls) == 3 and controls[0] < controls[1] < controls[2], controls

    warm = epochs[min(2, len(epochs) - 1) :]
    method = [entry["continued"]["METHOD"][-1] for entry in warm]
    semantic_columns = {
        "symbol_count": (1, 0),
        "symbol_name_bytes": (2, 0),
        "vtable_count": (3, 0),
        "current_bindings": (4, 0),
        "retiring_bindings": (5, 0),
        "generated_binding_bytes": (6, 0),
        "retained_fact_bytes": (7, 0),
        "live_heap_bytes": (0, 1024 * 1024),
    }
    for name, (column, slack) in semantic_columns.items():
        bounded([row[column] for row in method], slack, name)
    bounded([entry["resaved_image_bytes"] for entry in warm], 64 * 1024, "image_bytes")
    rss = [entry["maximum_resident_bytes"] for entry in warm]
    bounded(rss, 192 * 1024 * 1024, "rss", 64 * 1024 * 1024)

    summary = {
        "schema": 1,
        "epochs": count,
        "warmup_epochs": count - len(warm),
        "heap_slack_bytes": 1024 * 1024,
        "image_slack_bytes": 64 * 1024,
        "rss_spread_slack_bytes": 192 * 1024 * 1024,
        "rss_age_drift_slack_bytes": 64 * 1024 * 1024,
        "observed_rss": {
            "first_bytes": rss[0],
            "last_bytes": rss[-1],
            "minimum_bytes": min(rss),
            "maximum_bytes": max(rss),
        },
        "first_image_sha256": epochs[0]["input_image_sha256"],
        "final_image_sha256": epochs[-1]["output_image_sha256"],
        "result": "passed",
    }
    with (root / "summary.json").open("w", encoding="utf-8") as handle:
        json.dump(summary, handle, sort_keys=True, indent=2)
        handle.write("\n")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Lifecycle field/alias/annotation preservation in source-free images."""
import json
import sys
import tempfile
from pathlib import Path

from validation_support import ROOT, clean_environment, command, save_json, sha

prefix = Path(sys.argv[1]).resolve()
work = Path(tempfile.mkdtemp(prefix="birnam-lifecycle-records-"))
env = clean_environment()
env["BIRNAM_GC_PRECISE"] = "1"
writer = work / "save.birnam"
saved_image = work / "lifecycle.image"
fixture = ROOT / "tests/fixtures/lifecycle-record-image.birnam"
writer.write_text(fixture.read_text().replace("IMAGE_PATH", json.dumps(str(saved_image))))
receipt = {"passed": False, "prefix": str(prefix), "fixture_sha256": sha(fixture), "runs": []}
for name, args, expected in [
    ("save", [writer], "#saved\n"),
    ("mapped", [saved_image], "lifecycle-record-image: passed\n"),
    ("rebuilt", ["--rebuild", saved_image], "lifecycle-record-image: passed\n"),
]:
    result = command([prefix / "bin/birnam", *args], cwd=work, env=env, timeout=900)
    receipt["runs"].append(dict(name=name, **result))
    save_json(work / "results.json", receipt)
    (work / (name + ".log")).write_text(result["output"])
    assert result["exit_code"] == 0 and not result["stderr"] and result["stdout"] == expected, (work, name, result)
    if name == "save":
        receipt["image_sha256"] = sha(saved_image)
        writer.unlink()
receipt["passed"] = True
save_json(work / "results.json", receipt)
print("lifecycle record images: passed; " + str(work / "results.json"))

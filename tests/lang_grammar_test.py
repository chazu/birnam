#!/usr/bin/env python3
"""CST grammar acceptance, including the authored modules' own source."""
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PREFIX = [str(ROOT / "build/siphon"), str(ROOT / "build/seed.dylib"),
          "--eval", str(ROOT / "modules/peg.birnam"),
          "--eval", str(ROOT / "modules/lang.birnam")]


def main():
    with tempfile.TemporaryDirectory(prefix="birnam-lang-grammar-") as tmp:
        result = subprocess.run([*PREFIX, "--eval", str(ROOT / "tests/lang/grammar.birnam")],
                                cwd=tmp, text=True, capture_output=True, timeout=60)
        assert result.returncode == 0, result
        assert result.stdout == "PASS Lang grammar and CST spans\n", result
        source = Path(tmp) / "self.birnam"
        forms, expected = [], []
        for name in ("modules/peg.birnam", "modules/lang.birnam"):
            text = (ROOT / name).read_text()
            forms.append('[System println: [[Lang parse: ' + json.dumps(text, ensure_ascii=False) + '] end]];')
            expected.append(str(len(text.encode())))
        source.write_text("\n".join(forms))
        result = subprocess.run([*PREFIX, "--eval", str(source)], cwd=tmp,
                                text=True, capture_output=True, timeout=180)
        assert result.returncode == 0, result
        assert result.stdout.splitlines() == expected, result
    print("PASS Lang CST grammar: spans, valid/rejected syntax, PEG and Lang source; unrelated working directory")


if __name__ == "__main__":
    main()

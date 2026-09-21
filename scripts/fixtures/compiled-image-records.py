#!/usr/bin/env python3
"""Independently inspect native Code framing and derive admission/recovery cases."""
import importlib.util
from pathlib import Path
import struct
import sys

root = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("portable_oracle", root / "abi/fixtures/portable-code-v1/verify.py")
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)
path = Path(sys.argv[1])
data = path.read_bytes()
assert data[:12] == b"BIRNAMIM\x08\x00\x00\x00"
offset = 28
while True:
    kind, length = struct.unpack_from("<IQ", data, offset)
    if kind == 4:
        break
    offset += 12 + length
assert offset + 12 + length == len(data)
payload = data[offset + 12:]
bundle = payload[72:]
assert bundle[:12] == b"BIRNAMIC\x01\x00\x00\x00"
lengths = struct.unpack_from("<QQQ", bundle, 12)
assert all(lengths) and 36 + sum(lengths) == len(bundle)
assert oracle.wyhash(bundle) == struct.unpack_from("<Q", payload)[0]
portable_end = 36 + lengths[0]
generation_end = portable_end + lengths[1]
assert bundle[36:40] == b"\xcf\xfa\xed\xfe", "portable unit must be Mach-O"
assert bundle[portable_end:portable_end + 4] == b"\xcf\xfa\xed\xfe", "generation must be Mach-O"

def write(name, code):
    record = struct.pack("<Q", oracle.wyhash(code)) + payload[8:72] + code
    path.with_name(name + ".image").write_bytes(data[:offset] + struct.pack("<IQ", 4, len(record)) + record)

# Prior v8 saves contained only this unit. Their missing generation code must
# recover from embedded source and report SourceRecovery, not Mapped.
write("missing", bundle[36:portable_end])
bad_manifest = bytearray(bundle)
bad_manifest[generation_end] ^= 0x80
write("bad-manifest", bad_manifest)
bad_size = bytearray(bundle)
struct.pack_into("<Q", bad_size, 20, 0)
write("bad-size", bad_size)
write("trailing", bundle + b"\0")
print("compiled image Code: two native units, manifest, framing and checksum verified")

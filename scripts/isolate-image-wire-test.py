#!/usr/bin/env python3
"""Check the real image-backed worker using an independent wire-v1 reader.

The external reader allows debug image boot to finish. It does not alter the
production Isolate parent's ten-second ready-handshake deadline.
"""

import argparse
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import time


def frame(kind, request, payload):
    return struct.pack(">4sHBBQI", b"BISO", 1, kind, 0, request, len(payload)) + payload


def sized(tag, value):
    return bytes([tag]) + struct.pack(">I", len(value)) + value


def integer(value):
    return b"\x03" + struct.pack(">q", value)


def vector(values):
    return b"\x07" + struct.pack(">I", len(values)) + b"".join(values)


def mapping(pairs):
    return b"\x08" + struct.pack(">I", len(pairs)) + b"".join(k + v for k, v in pairs)


VALUES = [
    b"\x00", b"\x01", b"\x02",
    integer(-(1 << 63)), integer((1 << 63) - 1),
    b"\x04" + struct.pack(">d", 3.125),
    sized(5, b"a\0b"), sized(6, b"probe"), vector([]), mapping([]),
    vector([integer(7), b"\x02", sized(5, b"text")]),
    mapping([(sized(6, b"key"), vector([b"\x00", integer(-7)]))]),
]


def check(prefix, root, label):
    binary = prefix / "bin/birnam"
    source = root / (label + ".birnam")
    image = root / (label + ".image")
    source.write_text(
        "[Globals at: #ImportWireEcho put: [Object clone]];\n"
        "[Reflect defineMethod: #echo: on: ImportWireEcho as: [:self :value | value]];\n"
        "[DispatchGeneration initializeCurrent];\n"
        f"[System saveImage: {json.dumps(str(image))}];\n"
        "[System println: [[System imageSaveReport] status]];\n"
    )
    saved = subprocess.run([binary, source], capture_output=True, text=True, check=True)
    assert saved.stdout == "#saved\n" and not saved.stderr, saved
    source.unlink()

    channel, child = socket.socketpair()
    channel.settimeout(240)
    transcript = []

    def read_exact(length):
        data = b""
        while len(data) < length:
            chunk = channel.recv(length - len(data))
            assert chunk, "worker closed channel"
            data += chunk
        return data

    def receive():
        header = read_exact(20)
        magic, version, kind, flags, request, length = struct.unpack(">4sHBBQI", header)
        assert magic == b"BISO" and version == 1 and flags == 0
        assert length <= 16 * 1024 * 1024
        payload = read_exact(length)
        transcript.append(header + payload)
        return kind, request, payload

    started = time.monotonic()
    output = root / (label + "-worker.out")
    with output.open("w") as log:
        process = subprocess.Popen(
            [binary, "--worker", image, "ImportWireEcho", "echo:",
             "--channel-fd", str(child.fileno())],
            pass_fds=(child.fileno(),), stdout=log, stderr=subprocess.STDOUT,
            env={**os.environ, "BIRNAM_TASK_SERVICE": "0"},
        )
        child.close()
        try:
            assert receive() == (1, 0, b"")
            print(f"{label}: ready after {time.monotonic() - started:.2f}s", flush=True)
            for request, value in enumerate(VALUES, 1):
                channel.sendall(frame(3, request, value))
                assert receive() == (4, request, value), (label, request, transcript[-1])
            channel.sendall(frame(6, len(VALUES) + 1, b""))
            assert receive() == (7, len(VALUES) + 1, b"")
            assert process.wait(timeout=30) == 0
        finally:
            channel.close()
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=30)
    assert not output.read_text(), output.read_text()
    print(f"{label}: twelve exact wire round trips and orderly stop passed", flush=True)
    return transcript


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    parser.add_argument("--baseline", type=Path, help="Compare complete worker replies byte for byte")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="birnam-isolate-wire-") as directory:
        root = Path(directory)
        baseline = check(args.baseline.resolve(), root, "baseline") if args.baseline else None
        current = check(args.prefix.resolve(), root, "current")
        if baseline is not None:
            assert current == baseline
            print("Baseline/current image-backed worker transcripts match byte for byte")


if __name__ == "__main__":
    main()

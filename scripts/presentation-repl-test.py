#!/usr/bin/env -S uv run --script
# /// script
# dependencies = ["pyte==0.8.2"]
# ///
"""Real controlling-PTY acceptance and output-observed latency (not GUI paint).

uv run scripts/presentation-repl-test.py [--output path.json] [--samples 3]
Uses the installed BIRNAM or birnam on PATH and the opt-in prepared launcher.
"""
import argparse
import codecs
import ctypes
import fcntl
import hashlib
import json
import math
import os
import platform
from pathlib import Path
import pty
import select
import shutil
import signal
import statistics
import struct
import subprocess
import termios
import tempfile
import time

import pyte

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "examples/presentation-repl"


class Terminal:
    def __init__(self, locale="en_US.UTF-8", directory=APP, command=None, ready=None):
        self.screen = pyte.Screen(120, 32)
        self.stream = pyte.Stream(self.screen)
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.raw = bytearray()
        self.started = time.perf_counter()
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.chdir(directory)
            os.environ["TERM"] = "xterm-256color"
            os.environ["LC_ALL"] = locale
            argv = command or ["/bin/sh", "play.sh"]
            os.execvp(argv[0], argv)
        self.original = termios.tcgetattr(self.fd)
        self.resize(120, 32)
        try:
            self.until(lambda: ready(self) if ready else "Birnam presentations" in self.text and self.input == ">")
        except BaseException:
            self.close()
            raise
        self.startup_ms = (time.perf_counter() - self.started) * 1000

    @property
    def text(self):
        return "\n".join(self.screen.display)

    @property
    def input(self):
        return self.screen.display[-2].rstrip()

    @property
    def selection(self):
        for row in range(1, self.screen.lines - 3):
            if self.screen.buffer[row][0].bg == "blue":
                return self.screen.display[row].rstrip()
        return None

    def resize(self, width, height):
        self.screen.resize(height, width)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))

    def read(self, timeout):
        if select.select([self.fd], [], [], timeout)[0]:
            try:
                data = os.read(self.fd, 65536)
            except OSError as exc:
                raise AssertionError(f"PTY closed: {self.raw[-3000:]!r}") from exc
            assert data, f"PTY EOF: {self.raw[-3000:]!r}"
            self.raw.extend(data)
            self.stream.feed(self.decoder.decode(data))

    def until(self, predicate, timeout=45):
        deadline = time.perf_counter() + timeout
        while not predicate():
            assert time.perf_counter() < deadline, f"Timed out:\n{self.text}\n{self.raw[-1000:]!r}"
            self.read(min(0.05, max(0, deadline - time.perf_counter())))

    def send(self, text):
        os.write(self.fd, text.encode())

    def edit(self, text):
        self.send(text)
        self.until(lambda: self.input == "> " + text)

    def evaluate(self, source, expected):
        self.edit(source)
        self.send("\r")
        self.until(lambda: expected in self.text and self.input == ">")

    def stop(self, expected_status=0, send_quit=True):
        if send_quit:
            self.send("\x03\x04")
        deadline = time.perf_counter() + 10
        while time.perf_counter() < deadline:
            done, status = os.waitpid(self.pid, os.WNOHANG)
            if done:
                self.pid = None
                while select.select([self.fd], [], [], 0)[0]:
                    try:
                        data = os.read(self.fd, 65536)
                    except OSError:
                        break
                    if not data:
                        break
                    self.raw.extend(data)
                assert os.waitstatus_to_exitcode(status) == expected_status, self.raw[-2000:]
                restored = termios.tcgetattr(self.fd)
                assert restored == self.original, "terminal mode was not restored"
                return
            if select.select([self.fd], [], [], 0.05)[0]:
                try:
                    self.raw.extend(os.read(self.fd, 65536))
                except OSError:
                    pass
        raise AssertionError("REPL did not exit")

    def close(self):
        if self.pid:
            os.kill(self.pid, signal.SIGKILL)
            os.waitpid(self.pid, 0)
        os.close(self.fd)


def latency(terminal, count=60):
    """Closed loop: send one key, wait for its changed input row, repeat."""
    samples = []
    text = ""
    for i in range(count):
        ch = chr(ord("a") + i % 26)
        text += ch
        started = time.perf_counter()
        terminal.send(ch)
        terminal.until(lambda: terminal.input == "> " + text)
        samples.append((time.perf_counter() - started) * 1000)
    terminal.send("\x03")
    terminal.until(lambda: terminal.input == ">")
    return samples


def stats(values):
    ordered = sorted(values)
    return {"n": len(values), "median_ms": statistics.median(values),
            "p95_ms": ordered[math.ceil(len(values) * 0.95) - 1], "max_ms": max(values)}


def navigation(terminal):
    samples = []
    parent = terminal.selection
    index = next(i for i, row in enumerate(terminal.screen.display) if row.rstrip() == parent)
    child = terminal.screen.display[index + 1].rstrip()
    for _ in range(30):
        for key, predicate in (("\x1b[B", lambda: terminal.selection == child),
                               ("\x1b[A", lambda: terminal.selection == parent)):
            started = time.perf_counter()
            terminal.send(key)
            terminal.until(predicate)
            samples.append((time.perf_counter() - started) * 1000)
    return samples


def plain_startup():
    started = time.perf_counter()
    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(ROOT)
        os.environ["TERM"] = "xterm-256color"
        os.environ["LC_ALL"] = "en_US.UTF-8"
        binary = os.environ.get("BIRNAM", "birnam")
        os.execvp(binary, [binary, "--repl"])
    try:
        data = bytearray()
        deadline = started + 30
        while b"birnam> " not in data:
            assert time.perf_counter() < deadline, "ordinary REPL startup timed out"
            if select.select([fd], [], [], 0.05)[0]:
                data.extend(os.read(fd, 65536))
        elapsed = (time.perf_counter() - started) * 1000
        os.write(fd, b":quit\n")
        while time.perf_counter() < deadline:
            done, status = os.waitpid(pid, os.WNOHANG)
            if done:
                pid = None
                assert os.waitstatus_to_exitcode(status) == 0
                return elapsed
            if select.select([fd], [], [], 0.05)[0]:
                try:
                    os.read(fd, 65536)
                except OSError:
                    pass
        raise AssertionError("ordinary REPL quit timed out")
    finally:
        if pid:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        os.close(fd)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--samples", type=int, default=3)
    args = parser.parse_args()
    assert args.samples > 0
    subprocess.run(["sh", str(APP / "build.sh")], check=True)
    native_probe = tempfile.TemporaryDirectory(prefix="birnam-presentation-native-")
    native_path = Path(native_probe.name) / "libpresentation_terminal.dylib"
    subprocess.run(["cc", "-O2", "-dynamiclib", str(APP / "native.c"), "-o", str(native_path)], check=True)
    native = ctypes.CDLL(str(native_path))
    native.pr_capture_end.restype = ctypes.c_char_p
    assert native.pr_capture_begin() == 0
    assert native.pr_capture_begin() == -1, "nested capture must not leak descriptors"
    try:
        os.write(1, b"x" * 20000)
        os.write(2, b"stderr")
    finally:
        captured = native.pr_capture_end()
    assert len(captured) == 16384 and captured.endswith(b" [output truncated] ")
    assert native.pr_capture_end() == b"", "capture end is idempotent"
    print("Native output: bounded retention, no pipe deadlock, nested rejection, restoration passed", flush=True)
    subprocess.run([os.environ.get("BIRNAM", "birnam"), str(APP / "test.birnam")], cwd=ROOT, check=True)
    startups, empty, populated, expansion, arrows, full, scrolling, plain = [], [], [], [], [], [], [], []
    for sample in range(args.samples):
        plain.append(plain_startup())
        terminal = Terminal()
        try:
            startups.append(terminal.startup_ms)
            empty.extend(latency(terminal))
            print(f"PTY {sample + 1}: startup {terminal.startup_ms:.0f} ms, empty editing {stats(empty)}", flush=True)
            terminal.evaluate("let cell := [Cell with: 41]", "#1 Cell")
            terminal.evaluate("#[cell 7]", "#2 Vector (2 items)")
            terminal.send("\x1bOQ")  # F2
            terminal.until(lambda: "#3 [0] Cell" in terminal.text)
            terminal.send("\x1b[B\x1bOS")  # Down, F4
            terminal.until(lambda: terminal.input == "> [Presentations valueAt: 3]")
            terminal.send("\x01[\x05 value]\r")
            terminal.until(lambda: "#5 41" in terminal.text and terminal.input == ">")
            terminal.evaluate("[cell == [Presentations valueAt: 3]]", "#6 true")
            terminal.evaluate("[System gc]; [[Presentations valueAt: 3] set: 42]; [cell value]", "#7 42")
            terminal.evaluate("[0 to: 999999]", "#8 Range (1000000 items)")
            started = time.perf_counter()
            terminal.send("\x1bOQ")
            terminal.until(lambda: "#9 [0] 0" in terminal.text and "Showing 0..20 of 1000000" in terminal.text)
            expansion.append((time.perf_counter() - started) * 1000)
            populated.extend(latency(terminal))
            arrows.extend(navigation(terminal))
            print(f"PTY {sample + 1}: expanded editing {stats(populated)}, navigation {stats(arrows)}, expansion {expansion[-1]:.1f} ms", flush=True)
            terminal.evaluate('[System println: "captured stdout"]; 23', "#29 23")
            assert "captured stdout" in terminal.text
            terminal.evaluate("a]", "Compile error")
            terminal.evaluate("[1 noSuchMethod]", "#30 error(")
            terminal.edit('"hé界!"')
            terminal.send("\x1b[D\x7f")
            terminal.until(lambda: terminal.input == '> "hé界"')
            terminal.send("\x7f")
            terminal.until(lambda: terminal.input == '> "hé"')
            terminal.send("界\r")
            terminal.until(lambda: '#31 "hé界"' in terminal.text and terminal.input == ">")
            terminal.edit("[1 +")
            terminal.send("\r2]\r")
            terminal.until(lambda: "#32 3" in terminal.text and terminal.input == ">")
            terminal.resize(80, 24)
            terminal.until(lambda: "Enter eval" in terminal.screen.display[-1])
            terminal.resize(120, 32)
            terminal.until(lambda: "Enter eval" in terminal.screen.display[-1])
            terminal.send("\x1b[15~")  # F5 forget
            terminal.until(lambda: "Forgot transcript" in terminal.text)
            terminal.evaluate("[Presentations valueAt: 3]", "#33 error(#expiredPresentation)")
            terminal.evaluate("[[1 to: 300] do: [:n | [Presentations appendValue: n]]]; nil", "#334 nil")
            full.extend(latency(terminal))
            for step in range(1, 81):
                started = time.perf_counter()
                terminal.send("\x1b[A")
                expected = f"#{334-step} {301-step}"
                terminal.until(lambda: terminal.selection == expected)
                if step >= 28:
                    scrolling.append((time.perf_counter() - started) * 1000)
            terminal.stop()
            print(f"PTY {sample + 1}: identity, GC, paging, output, errors, Unicode, multiline, resize, expiry, cleanup passed", flush=True)
        finally:
            terminal.close()
    terminal = Terminal(locale="C")
    try:
        terminal.evaluate('"é界"', '#1 "é界"')
        terminal.evaluate('"a\\0b"', '#2 "a␀b"')
        terminal.edit("[Reflect defineMethod: #hostStep on: Repl as: [:self | [Error kind: #forcedTerminalFault]]]")
        terminal.send("\r")
        terminal.stop(expected_status=1, send_quit=False)
        assert b"forcedTerminalFault" in terminal.raw
        print("C locale: Unicode fallback passed; Host failure: terminal restored on a faulting turn", flush=True)
    finally:
        terminal.close()
    sources = sorted(p for p in APP.glob("*.birnam") if p.is_file())
    sources += [APP / name for name in ("native.c", "build.sh", "play.sh", "birnam.toml")]
    binary = Path(shutil.which(os.environ.get("BIRNAM", "birnam"))).resolve()
    bootstrap = binary.parent.parent / "lib/birnam-bootstrap.image"
    report = {"binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "bootstrap_sha256": hashlib.sha256(bootstrap.read_bytes()).hexdigest(),
              "platform": platform.platform(),
              "sources": {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},
              "terminal": "xterm-256color, 120x32 PTY; pyte screen assertions",
              "startup": stats(startups), "ordinary_repl_startup": stats(plain), "editing_empty": stats(empty),
              "editing_after_expansion": stats(populated), "expand_20_of_million": stats(expansion),
              "navigation": stats(arrows), "editing_full_transcript": stats(full),
              "scrolling_full_transcript": stats(scrolling),
              "limits": "Warm filesystem; output-observed PTY latency, not graphical paint. Prepared launcher; initial build excluded. Synchronous evaluation."}
    print(json.dumps(report, indent=2))
    report["samples_ms"] = {"startup": startups, "ordinary_repl_startup": plain,
                            "editing_empty": empty, "editing_after_expansion": populated,
                            "expand_20_of_million": expansion, "navigation": arrows,
                            "editing_full_transcript": full, "scrolling_full_transcript": scrolling}
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()

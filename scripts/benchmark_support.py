"""Benchmark orchestration shared across families; no semantic parsing here."""
from contextlib import contextmanager
from pathlib import Path
import subprocess
import time
from validation_support import command, clean_environment, save_json, sha


class Commands:
    def __init__(self, root, env, *, deadline=900, timeout=300):
        self.root, self.env = root, env
        self.stop_at = time.monotonic() + deadline
        self.timeout = timeout

    def __call__(self, argv, extra=None):
        remaining = self.stop_at - time.monotonic()
        if remaining <= 0: raise TimeoutError('benchmark deadline exhausted')
        return command(argv, cwd=self.root, env=dict(self.env, **(extra or {})),
                       timeout=min(self.timeout, remaining))


def source_checker(root, sources, artifacts):
    def check():
        for name, digest in sources.items():
            if sha(Path(root) / name) != digest:
                raise RuntimeError('source changed since verification: ' + name)
        for path, digest in artifacts.items():
            if sha(path) != digest:
                raise RuntimeError('artifact changed since verification: ' + str(path))
    return check


@contextmanager
def host_monitor(path):
    """Retain interval samples while guaranteeing monitor teardown."""
    with Path(path).open('w') as stream:
        process = subprocess.Popen(['top', '-l', '0', '-s', '1', '-n', '0', '-stats', 'pid'],
                                   stdout=stream, stderr=subprocess.DEVNULL)
        try:
            yield
        finally:
            process.terminate()
            try: process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()

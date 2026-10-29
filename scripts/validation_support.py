"""Shared execution and evidence mechanics; semantic oracles stay in their suites."""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import threading
from concurrent.futures import CancelledError
from measurement_archive import read_bytes

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(read_bytes(path)).hexdigest()


def save_json(path, value):
    """Readers see either the previous complete receipt or the new complete one."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode='w', dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        try:
            json.dump(value, stream, indent=2, sort_keys=True)
            stream.write('\n')
            stream.flush()
            os.fsync(stream.fileno())
        except BaseException:
            temporary.unlink(missing_ok=True)
            raise
    try:
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def clean_environment():
    return {k: v for k, v in os.environ.items()
            if k == 'BIRNAM_VALIDATION_CHILD' or not k.startswith(('BIRNAM_', 'DYLD_', 'BENCH_HOST_'))}


def environment_binding(env):
    return {k: env[k] for k in (
        "PATH", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET",
        "CC", "CFLAGS", "CXX", "CXXFLAGS", "SDKROOT", "MACOSX_DEPLOYMENT_TARGET",
        "RUSTUP_TOOLCHAIN", "CARGO_TARGET_DIR", "LANG", "LC_ALL") if k in env}


def file_hashes(patterns, root=ROOT):
    """Include membership, so adding/removing a matching input invalidates reuse."""
    paths = set()
    for pattern in patterns:
        path = Path(pattern)
        if path.is_absolute():
            paths.update(p for p in ([path] if path.is_file() else path.rglob('*')) if p.is_file())
        else:
            paths.update(p for p in root.glob(str(pattern)) if p.is_file())
    return {str(p): sha(p) for p in sorted(paths)}


def command(argv, *, cwd=ROOT, env=None, timeout=300, log=None, cancel_event=None):
    """Run with a deadline; stop the owned process group on timeout/cancellation."""
    argv = [str(a) for a in argv]
    environment = dict(os.environ if env is None else env)
    # A top-level runner owns the entire group. Nested helpers stay in it.
    isolated = not environment.get('BIRNAM_VALIDATION_CHILD')
    environment['BIRNAM_VALIDATION_CHILD'] = '1'
    started = time.monotonic_ns()
    stream = open(log, 'w') if log else None
    process = subprocess.Popen(argv, cwd=cwd, env=environment,
                               stdout=stream or subprocess.PIPE,
                               stderr=stream or subprocess.PIPE, text=True,
                               start_new_session=isolated)
    def group_is_live():
        groups = subprocess.check_output(['ps', '-axo', 'pgid=,stat='], text=True)
        return any(parts[0] == str(process.pid) and not parts[1].startswith('Z')
                   for line in groups.splitlines() if len(parts := line.split()) == 2)
    def signal_group(sig):
        try:
            os.killpg(process.pid, sig)
        except ProcessLookupError:
            pass
        except PermissionError:
            # Darwin may report EPERM after the last member dies between the
            # census and signal. Ignore it only after confirming no live member.
            if group_is_live():
                raise
    def stop():
        if isolated:
            signal_group(signal.SIGTERM)
        elif process.poll() is None:
            process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
        if isolated and group_is_live():
            signal_group(signal.SIGKILL)
        process.wait()
    try:
        if cancel_event is None:
            stdout, stderr = process.communicate(timeout=timeout)
        else:
            end = time.monotonic() + timeout
            while True:
                if cancel_event.is_set():
                    raise CancelledError('validation lane cancelled')
                remaining = end - time.monotonic()
                if remaining <= 0:
                    raise subprocess.TimeoutExpired(argv, timeout)
                try:
                    stdout, stderr = process.communicate(timeout=min(.2, remaining))
                    break
                except subprocess.TimeoutExpired:
                    continue
    except BaseException:
        stop()
        raise
    finally:
        if stream: stream.close()
        if process.stdout: process.stdout.close()
        if process.stderr: process.stderr.close()
    return dict(argv=argv, exit_code=process.returncode,
                wall_ns=time.monotonic_ns() - started,
                stdout=stdout or '', stderr=stderr or '',
                output=(stdout or '') + (stderr or ''))


def audit_steps(rows, required):
    """Validate complete evidence, including mutations after a step finished."""
    visited=[]
    def visit(name):
        if name in visited: return
        row=rows.get(name)
        if not row or row.get('status')!='passed' or row.get('exit_code')!=0:
            raise ValueError('missing passed phase: '+name)
        path=Path(row['log']).with_name('receipt.json')
        if sha(path)!=row['evidence_sha256']:
            raise ValueError('phase receipt changed: '+name)
        stored=json.loads(path.read_text())
        recorded={k:v for k,v in row.items() if k not in ('evidence_sha256','reused')}
        if stored!=recorded: raise ValueError('phase summary differs from receipt: '+name)
        binding=row['binding']
        if not binding.get('patterns') or file_hashes(binding['patterns'])!=binding['inputs']:
            raise ValueError('phase inputs changed: '+name)
        if file_hashes(row.get('artifact_patterns',()))!=row.get('artifacts',{}):
            raise ValueError('phase outputs changed: '+name)
        if sha(row['log'])!=row['log_sha256']: raise ValueError('phase log changed: '+name)
        for dependency,digest in binding['dependencies'].items():
            visit(dependency)
            if rows[dependency]['evidence_sha256']!=digest:
                raise ValueError('phase prerequisite changed: '+dependency)
        visited.append(name)
    for name in required: visit(name)
    return visited


def audit_plan(rows, steps):
    """Bind complete receipts to the requested plan, not just passing step names."""
    for step in steps:
        row = rows.get(step['name'], {})
        binding = row.get('binding', {})
        if binding.get('argv') != [str(a) for a in step['argv']]:
            raise ValueError('phase command differs from plan: ' + step['name'])
        if set(binding.get('dependencies', {})) != set(step.get('depends', ())):
            raise ValueError('phase prerequisites differ from plan: ' + step['name'])
        patterns = list(step['inputs']) + ['scripts/validation_support.py', 'scripts/validate.py']
        if binding.get('patterns') != patterns:
            raise ValueError('phase inputs differ from plan: ' + step['name'])
        if row.get('artifact_patterns') != list(step.get('artifacts', ())):
            raise ValueError('phase outputs differ from plan: ' + step['name'])
    return audit_steps(rows, [step['name'] for step in steps])


class PlanRunner:
    """One plan owns execution; cache entries bind commands, inputs and outputs."""
    def __init__(self, directory, *, deadline=900, resume=True):
        self.lock = threading.RLock()
        self.cancel_event = threading.Event()
        self.started = time.monotonic_ns()
        self.directory = Path(directory)
        self.directory.mkdir(parents=True, exist_ok=True)
        self.stop_at = time.monotonic() + deadline
        self.resume = resume
        self.rows = {}
        self.env = clean_environment()
        self.env['CARGO_TARGET_DIR'] = str(ROOT / '.cargo-target')
        self.toolchain = {name: command([name, '--version'], env=self.env)['output'].strip()
                          for name in ('cargo', 'rustc', 'cc', 'python3')}
        self.receipt = dict(schema=1, status='running', steps=self.rows, toolchain=self.toolchain)
        self.save()

    def save(self):
        with self.lock:
            self.receipt['wall_ns'] = time.monotonic_ns() - self.started
            save_json(self.directory / 'run.json', self.receipt)

    def run(self, name, argv, *, inputs, artifacts=(), depends=(), timeout=300):
        remaining = self.stop_at - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('validation deadline exhausted before ' + name)
        if self.cancel_event.is_set():
            raise CancelledError('validation lane cancelled before ' + name)
        paths = tuple(inputs) + ('scripts/validation_support.py', 'scripts/validate.py')
        before = file_hashes(paths)
        with self.lock:
            dependencies = {n: self.rows[n]['evidence_sha256'] for n in depends}
        binding = dict(argv=[str(a) for a in argv], inputs=before, patterns=list(paths),
                       toolchain=self.toolchain, environment=environment_binding(self.env),
                       dependencies=dependencies)
        key = hashlib.sha256(json.dumps(binding, sort_keys=True).encode()).hexdigest()
        directory = self.directory / name / key
        directory.mkdir(parents=True, exist_ok=True)
        receipt_path = directory / 'receipt.json'
        row = dict(status='running', binding=binding, key=key, artifact_patterns=list(artifacts))
        if self.resume and receipt_path.exists():
            prior = json.loads(receipt_path.read_text())
            outputs = file_hashes(artifacts)
            if (prior.get('status') == 'passed' and prior.get('binding') == binding
                    and prior.get('artifacts') == outputs
                    and (not artifacts or outputs)
                    and Path(prior['log']).is_file()
                    and sha(prior['log']) == prior['log_sha256']):
                with self.lock:
                    self.rows[name] = dict(prior, reused=True, evidence_sha256=sha(receipt_path))
                    self.save()
                print(f'validate: {name}: reused matching passed evidence', flush=True)
                return
        log = directory / 'output.log'
        with self.lock:
            self.rows[name] = row
            self.save()
        save_json(receipt_path, row)
        print(f'validate: {name}: running (limit {min(timeout, remaining):.0f}s; {log})', flush=True)
        started = time.monotonic_ns()
        try:
            result = command(argv, env=self.env, timeout=min(timeout, remaining), log=log,
                             cancel_event=self.cancel_event)
            with self.lock:
                row.update({k: result[k] for k in ('argv', 'exit_code', 'wall_ns')})
            if result['exit_code']:
                raise RuntimeError(f'{name} failed ({result["exit_code"]}); see {log}')
            if before != file_hashes(paths):
                raise RuntimeError(name + ': inputs changed during execution')
            outputs = file_hashes(artifacts)
            if artifacts and not outputs:
                raise RuntimeError(name + ': expected outputs are missing')
            with self.lock:
                row.update(artifacts=outputs, status='passed')
        except BaseException as error:
            with self.lock:
                row.update(status='failed', error=str(error))
            raise
        finally:
            with self.lock:
                row.setdefault('wall_ns', time.monotonic_ns() - started)
                row['log'] = str(log)
                if log.is_file(): row['log_sha256'] = sha(log)
                save_json(receipt_path, row)
                row['evidence_sha256'] = sha(receipt_path)
                self.save()
        print(f'validate: {name}: passed in {row["wall_ns"] / 1e9:.1f}s', flush=True)

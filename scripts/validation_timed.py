"""Record each installed check while preserving its output and exit status."""
import json
from pathlib import Path
import subprocess
import sys
import time
from validation_support import save_json


def run(path, label, argv):
    path = Path(path)
    data = json.loads(path.read_text()) if path.exists() else dict(schema=1, checks=[])
    row = dict(label=label, argv=argv, status='running', started_unix_ns=time.time_ns())
    data['checks'].append(row)
    save_json(path, data)
    start = time.monotonic_ns()
    print('check: ' + label + ': running', flush=True)
    try:
        result = subprocess.run(argv)
        row.update(exit_code=result.returncode, status='passed' if result.returncode == 0 else 'failed')
    except BaseException as error:
        row.update(status='failed', error=str(error))
        raise
    finally:
        row['wall_ns'] = time.monotonic_ns() - start
        save_json(path, data)
        print(f'check: {label}: {row["status"]} in {row["wall_ns"]/1e9:.1f}s', flush=True)
    return result.returncode


if __name__ == '__main__':
    path, label, separator, *argv = sys.argv[1:]
    if separator != '--' or not argv: raise SystemExit('expected TIMINGS LABEL -- COMMAND ...')
    raise SystemExit(run(path, label, argv))

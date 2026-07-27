#!/usr/bin/env bash
# Verify that production, test, and rollback prefixes have distinct contents.
set -euo pipefail

cd "$(dirname "$0")/.."

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

canonical=(
  libbirnam_id.dylib
  libbirnam_store.dylib
  libbirnam_id_artifact.dylib
  libbirnam_id_peg.dylib
  libbirnam_id_lang.dylib
  libbirnam_id_tcc.dylib
  libbirnam_id_codegen.dylib
  libbirnam_id_prelude.dylib
  libbirnam_id_collection.dylib
  libbirnam_id_error.dylib
  libbirnam_id_isolate.dylib
  libbirnam_id_relational.dylib
  libbirnam_id_alien.dylib
  libbirnam_id_image.dylib
)

production="$work/production"
zig build install -Doptimize=ReleaseSafe -Ddistribution=production --prefix "$production" >/dev/null

if [[ -e "$production/bin/birnam-zig-oracle" ]]; then
  echo "distribution-selection: production contains birnam-zig-oracle" >&2
  exit 1
fi
zig_fixtures="$(find "$production/lib" -type f -name '*_zig.dylib' -print)"
if [[ -n "$zig_fixtures" ]]; then
  echo "distribution-selection: production contains Zig fixtures:" >&2
  printf '%s\n' "$zig_fixtures" >&2
  exit 1
fi
for name in "${canonical[@]}"; do
  [[ -f "$production/lib/$name" ]] || {
    echo "distribution-selection: production missing $name" >&2
    exit 1
  }
done
[[ -x "$production/bin/birnam" ]] || {
  echo "distribution-selection: production missing bin/birnam" >&2
  exit 1
}

rollback="$work/rollback"
zig build install -Doptimize=ReleaseSafe -Ddistribution=rollback --prefix "$rollback" >/dev/null
[[ -x "$rollback/bin/birnam" ]] || {
  echo "distribution-selection: rollback missing canonical Zig host" >&2
  exit 1
}
[[ -x "$rollback/bin/birnam-zig-oracle" ]] || {
  echo "distribution-selection: rollback missing explicit Zig oracle" >&2
  exit 1
}
for name in "${canonical[@]}"; do
  fixture="${name%.dylib}_zig.dylib"
  [[ -f "$rollback/lib/$name" ]] || {
    echo "distribution-selection: rollback missing $name" >&2
    exit 1
  }
  [[ -f "$rollback/lib/$fixture" ]] || {
    echo "distribution-selection: rollback missing $fixture" >&2
    exit 1
  }
  canonical_digest="$(shasum -a 256 "$rollback/lib/$name" | awk '{print $1}')"
  fixture_digest="$(shasum -a 256 "$rollback/lib/$fixture" | awk '{print $1}')"
  [[ "$canonical_digest" == "$fixture_digest" ]] || {
    echo "distribution-selection: rollback canonical $name is not its Zig fixture" >&2
    exit 1
  }
done

echo "distribution-selection: production omits Zig; rollback is a complete Zig vector"

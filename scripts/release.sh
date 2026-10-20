#!/usr/bin/env bash
# Cut a release: bump the version, build the tarball, tag, push, publish.
#
#   scripts/release.sh patch    # 0.1.3 -> 0.1.4
#   scripts/release.sh minor    # 0.1.3 -> 0.2.0
#   scripts/release.sh major    # 0.1.3 -> 1.0.0
#   scripts/release.sh --resume # retry the prepared candidate without bumping
#
# VERSION advances when the release candidate is prepared. On failure, --resume
# continues that candidate; its tag is created only after acceptance passes.
#
# Requires a CHANGELOG.md entry for the new version: either write `## [x.y.z]`
# yourself, or put notes under `## [Unreleased]` and this promotes+dates it.
# The version's section becomes the GitHub release notes.
set -euo pipefail
cd "$(dirname "$0")/.."

# Serialize preparation, qualification and publication; fcntl releases the lock
# even after an interrupted process, without a stale lock-directory protocol.
if [[ ${BIRNAM_RELEASE_LOCKED:-} != "$PWD" ]]; then
  exec python3 - "$PWD" "$@" <<'PY_LOCK'
import fcntl, os, pathlib, subprocess, sys
root = pathlib.Path(sys.argv[1])
(root / '.cargo-target').mkdir(exist_ok=True)
with (root / '.cargo-target/release.lock').open('w') as lock:
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        sys.exit('another release is active in this checkout')
    env = dict(os.environ, BIRNAM_RELEASE_LOCKED=str(root))
    sys.exit(subprocess.run(['bash', 'scripts/release.sh', *sys.argv[2:]], env=env).returncode)
PY_LOCK
fi

python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 12) else "error: release tooling requires Python 3.12 or newer on PATH")'
level="${1:-}"
case "$level" in
  patch|minor|major|--resume) ;;
  *) echo "usage: $(basename "$0") patch|minor|major|--resume" >&2; exit 2 ;;
esac

# Cheap preflight precedes the candidate commit and all qualification work.
[ -z "$(git status --porcelain)" ] || { echo "error: working tree dirty — commit or stash first" >&2; exit 1; }
branch="$(git rev-parse --abbrev-ref HEAD)"
[ "$branch" = main ] || { echo "error: releases are cut from main (on '$branch')" >&2; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "error: gh not authenticated (run: gh auth login)" >&2; exit 1; }

if [[ $(uname -s) != Darwin || $(uname -m) != arm64 ]]; then
  echo "error: releases require arm64 macOS" >&2; exit 1
fi

if [[ "$level" == --resume ]]; then
  new="$(cat VERSION)"
  tag="v${new}"
  [[ "$(git log -1 --format=%s)" == "release ${tag}" ]] || {
    echo "error: --resume requires the unchanged prepared release commit" >&2; exit 1;
  }
else
  cur="$(cat VERSION)"
  IFS=. read -r ma mi pa <<<"$cur"
  case "$level" in
    major) ma=$((ma + 1)); mi=0; pa=0 ;;
    minor) mi=$((mi + 1)); pa=0 ;;
    patch) pa=$((pa + 1)) ;;
  esac
  new="${ma}.${mi}.${pa}"
  tag="v${new}"
  git rev-parse -q --verify "refs/tags/$tag" >/dev/null && {
    echo "error: tag already exists: $tag" >&2; exit 1;
  }
  echo "==> preparing ${cur} -> ${new} (${tag})"
  today="$(date +%F)"
  changelog_status=$(python3 scripts/release-changelog.py CHANGELOG.md "$new" "$today")
  echo "==> CHANGELOG: $changelog_status"
  echo "$new" > VERSION
  git add CHANGELOG.md VERSION
  git commit -m "release ${tag}"
fi
candidate_commit="$(git rev-parse HEAD)"
# After any failure, this exact commit can resume without another version bump.
trap 'status=$?; if [[ $status != 0 ]]; then echo "Release incomplete; retry this candidate with scripts/release.sh --resume" >&2; fi' EXIT

# Release notes = this version's CHANGELOG section (fallback to a one-liner).
notes_body="$(awk -v v="$new" '/^## \[/{ if (p) exit } $0 ~ "^## \\[" v "\\]"{ p=1; next } p' CHANGELOG.md)"
[ -n "$notes_body" ] || notes_body="See CHANGELOG.md."

evidence_dir="dist/${tag}-id-message-acceptance"
echo "==> qualify one cold release candidate (matching passed gates resume)"
python3 scripts/validate.py --suite release --candidate --output "$evidence_dir/qualification"
prefix=$(python3 - "$evidence_dir/qualification/run.json" <<'PY_PREFIX'
import json, sys
print(json.load(open(sys.argv[1]))['prefix'])
PY_PREFIX
)
tarball="$(scripts/dist.sh --prefix "$prefix")"
echo "==> packaged qualified candidate ${tarball}"
python3 scripts/release-acceptance.py "$tarball" "$evidence_dir" \
  --qualification "$evidence_dir/qualification/run.json"
python3 - "$evidence_dir/receipt.json" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as handle:
    receipt = json.load(handle)
assert receipt["result"] == "passed"
assert receipt["promotable"] is True
PY
evidence_archive="${evidence_dir}.tar.gz"
COPYFILE_DISABLE=1 tar -C dist -czf "$evidence_archive" "$(basename "$evidence_dir")"
archive_digest=$(shasum -a 256 "$tarball" | awk '{print $1}')
receipt_digest=$(shasum -a 256 "$evidence_dir/receipt.json" | awk '{print $1}')
echo "==> accepted immutable archive ${archive_digest}"

[[ "$(git rev-parse HEAD)" == "$candidate_commit" && -z "$(git status --porcelain)" ]] || {
  echo "error: candidate checkout changed before publication" >&2; exit 1;
}
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  [[ "$(git rev-parse "$tag^{}")" == "$candidate_commit" ]] || {
    echo "error: existing tag points to a different candidate" >&2; exit 1;
  }
else
  git tag -a "$tag" -m "birnam ${tag}"
fi
git push origin HEAD
git push origin "$tag"

notes_file="$evidence_dir/release-notes.md"
cat >"$notes_file" <<NOTES
${notes_body}

---
Portable arm64 macOS build.

\`\`\`
curl -L <this release's tarball URL> | tar xz
xattr -dr com.apple.quarantine birnam-${new}-*   # clear Gatekeeper quarantine
birnam-${new}-*/bin/birnam                        # zero-config REPL
\`\`\`

Other platforms are not supported yet. They require target-specific libtcc and
shared-library-loader ports before source or release builds can work.

Archive SHA-256: ${archive_digest}
Acceptance receipt SHA-256: ${receipt_digest}
NOTES
gh release create "$tag" "$tarball" "$evidence_archive" "$evidence_dir/receipt.json" \
  --title "birnam ${tag}" --notes-file "$notes_file"

echo "==> released ${tag}"
echo "==> next: capture the Rust-floor snapshot on this tagged commit (quiet machine):"
echo "      mise run bench-release \\"
echo "        && git add bench/results/v${new}-rust-*.json \\"
echo "        && git commit -m 'bench: v${new} snapshot' && git push"

#!/usr/bin/env bash
# Cut a release: bump the version, build the tarball, tag, push, publish.
#
#   scripts/release.sh patch    # 0.1.3 -> 0.1.4
#   scripts/release.sh minor    # 0.1.3 -> 0.2.0
#   scripts/release.sh major    # 0.1.3 -> 1.0.0
#
# The version in ./VERSION is the last RELEASED version; this bumps to the
# next one. So from a fresh 0.0.0, `release.sh minor` cuts v0.1.0.
#
# Requires a CHANGELOG.md entry for the new version: either write `## [x.y.z]`
# yourself, or put notes under `## [Unreleased]` and this promotes+dates it.
# The version's section becomes the GitHub release notes.
set -euo pipefail
cd "$(dirname "$0")/.."

level="${1:-}"
case "$level" in
  patch|minor|major) ;;
  *) echo "usage: $(basename "$0") patch|minor|major" >&2; exit 2 ;;
esac

# Preflight: clean tree, on main, gh authed, and fully verified.
[ -z "$(git status --porcelain)" ] || { echo "error: working tree dirty — commit or stash first" >&2; exit 1; }
branch="$(git rev-parse --abbrev-ref HEAD)"
[ "$branch" = main ] || { echo "error: releases are cut from main (on '$branch')" >&2; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "error: gh not authenticated (run: gh auth login)" >&2; exit 1; }

echo "==> verify"
scripts/rust-cutover-verify.sh

cur="$(cat VERSION)"
IFS=. read -r ma mi pa <<<"$cur"
case "$level" in
  major) ma=$((ma + 1)); mi=0; pa=0 ;;
  minor) mi=$((mi + 1)); pa=0 ;;
  patch) pa=$((pa + 1)) ;;
esac
new="${ma}.${mi}.${pa}"
tag="v${new}"

echo "==> release: ${cur} -> ${new} (${tag})"

# CHANGELOG gate: every release must be documented. Either the version section
# already exists, or an `## [Unreleased]` section is promoted to it (dated).
today="$(date +%F)"
if grep -qE "^## \[${new}\]" CHANGELOG.md; then
  : # entry already written
elif grep -qE "^## \[Unreleased\]" CHANGELOG.md; then
  echo "==> CHANGELOG: promoting [Unreleased] -> [${new}] — ${today}"
  sed -i.bak -E "s/^## \[Unreleased\].*/## [${new}] — ${today}/" CHANGELOG.md
  rm -f CHANGELOG.md.bak
  git add CHANGELOG.md
else
  echo "error: CHANGELOG.md has no '## [${new}]' section and no '## [Unreleased]' to promote." >&2
  echo "       Add release notes under '## [Unreleased]' (or '## [${new}]') first." >&2
  exit 1
fi

# Release notes = this version's CHANGELOG section (fallback to a one-liner).
notes_body="$(awk -v v="$new" '/^## \[/{ if (p) exit } $0 ~ "^## \\[" v "\\]"{ p=1; next } p' CHANGELOG.md)"
[ -n "$notes_body" ] || notes_body="See CHANGELOG.md."

# Commit the bump (+ promoted CHANGELOG) and tag it BEFORE building, so the
# tarball carries the new version (dist.sh reads ./VERSION).
echo "$new" > VERSION
git add VERSION
git commit -m "release ${tag}"
git tag -a "$tag" -m "birnam ${tag}"

tarball="$(scripts/dist.sh)"
echo "==> built ${tarball}"

git push origin HEAD
git push origin "$tag"

gh release create "$tag" "$tarball" \
  --title "birnam ${tag}" \
  --notes "$(cat <<NOTES
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
NOTES
)"

echo "==> released ${tag}"
echo "==> next: capture the Rust-floor snapshot on this tagged commit (quiet machine):"
echo "      mise run bench-release \\"
echo "        && git add bench/results/v${new}-rust-*.json \\"
echo "        && git commit -m 'bench: v${new} snapshot' && git push"

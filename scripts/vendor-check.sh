#!/bin/sh
# vendor/syntect is syntect 5.3.0 and vendor/syntect.patch, and nothing else: a copy of it with the patch taken
# off must be, file for file, what vendor/syntect.sha256 lists (the sha256 of upstream's files), and no file may
# be there that the list does not have (a link or a special file included). See vendor/README.md. POSIX sh; sha256sum, or `shasum -a 256` (macOS).
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
export LC_ALL=C

cp -R "$ROOT/vendor/syntect" "$tmp/syntect"
(cd "$tmp/syntect" && patch -R -p1 -f -s < "$ROOT/vendor/syntect.patch") || {
  echo "vendor-check: vendor/syntect.patch does not come off vendor/syntect" >&2
  exit 1
}

sha256() {
  if command -v sha256sum > /dev/null 2>&1; then sha256sum "$@"; else shasum -a 256 "$@"; fi
}
# Every file is a regular one: a link or a special file is not what upstream's crate has, and `sha256sum`
# would read what a link points to.
(cd "$tmp/syntect" && find . ! -type d ! -type f) > "$tmp/odd"
if [ -s "$tmp/odd" ]; then
  echo "vendor-check: vendor/syntect has files that are not regular files:" >&2
  cat "$tmp/odd" >&2
  exit 1
fi
awk '{ print $2 }' "$ROOT/vendor/syntect.sha256" | sort > "$tmp/listed"
(cd "$tmp/syntect" && find . ! -type d | sed 's|^\./||' | sort) > "$tmp/found"
if ! cmp -s "$tmp/listed" "$tmp/found"; then
  echo "vendor-check: the files of vendor/syntect are not the files of upstream's list:" >&2
  diff "$tmp/listed" "$tmp/found" >&2 || true
  exit 1
fi
(cd "$tmp/syntect" && sha256 $(cat "$tmp/listed")) > "$tmp/got"
if ! cmp -s "$tmp/got" "$ROOT/vendor/syntect.sha256"; then
  echo "vendor-check: vendor/syntect is not upstream's syntect 5.3.0 plus vendor/syntect.patch:" >&2
  diff "$ROOT/vendor/syntect.sha256" "$tmp/got" >&2 || true
  exit 1
fi
echo "vendor/syntect: $(wc -l < "$tmp/listed" | tr -d ' ') files, upstream's plus the patch"

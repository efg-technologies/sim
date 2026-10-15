#!/bin/sh
# Checks that SPEC, code and tests line up (see "Checks" in CLAUDE.md).
#
# For every requirement ID in SPEC.md it counts the tests whose name starts with the
# ID (Rust `fn safe_1_...` in src/ and tests/, shell `safe_1_...()` in scripts/e2e.sh)
# and looks for the ID in the `//!` header of some module in src/.
# It fails when, for IDs up to the milestone in .milestone:
#   - an ID has no test,
#   - an ID is not listed in any `//!` header,
# or when any test is named after an ID that is not in SPEC.
# POSIX sh + awk only, so it also runs on macOS.
set -eu

cd "$(dirname "$0")/.."

for f in SPEC.md .milestone; do
  [ -f "$f" ] || { echo "trace: $f not found" >&2; exit 1; }
done

milestone=$(tr -d ' \t\r\n' < .milestone)
case $milestone in
  M*) current=${milestone#M} ;;
  *) current= ;;
esac
case $current in
  "" | *[!0-9]*)
    echo "trace: .milestone must look like M1, found '$milestone'" >&2
    exit 1
    ;;
esac

tmp=$(mktemp -d "${TMPDIR:-/tmp}/si-trace.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

# Test-name families; IDs look like SAFE-1 and test names like safe_1_...
families='cli|safe|prop|vis|vi|ai|perf|val'

# 1. IDs and milestones from SPEC.md, in file order. Indented sub-items are skipped.
awk '
  /^- \*\*[A-Z]+-[0-9]+\*\* / {
    id = $2
    gsub(/\*/, "", id)
    if (match($0, /\[M[0-9]+\]/)) {
      print "spec", id, substr($0, RSTART + 2, RLENGTH - 3)
    } else {
      print "nomilestone", id
    }
  }
' SPEC.md > "$tmp/spec"

# 2. Test functions (mode rs/sh) and `//!` header IDs (mode hdr). Prints one tagged line each.
# claim <ID> <counted 1|0> <file:line>   a function named after an ID; counted if `_` follows the number
# impl <ID>                              an ID found as a whole token in a `//!` line
extract='
function claim(tok, loc,    fam, rest, num, delim) {
  fam = tok
  sub(/_.*/, "", fam)
  rest = substr(tok, length(fam) + 2)
  num = rest
  sub(/[^0-9].*/, "", num)
  delim = substr(rest, length(num) + 1, 1)
  print "claim", toupper(fam) "-" num, (delim == "_") ? 1 : 0, loc
}
mode == "rs" && $0 !~ /^[ \t]*\/\// {
  s = " " $0 " "
  while (match(s, "[^A-Za-z0-9_]fn[ \t]+(" families ")_[0-9]+[^A-Za-z0-9]")) {
    tok = substr(s, RSTART, RLENGTH)
    s = substr(s, RSTART + RLENGTH)
    sub(/^[^A-Za-z0-9_]fn[ \t]+/, "", tok)
    claim(tok, FILENAME ":" FNR)
  }
}
mode == "sh" && /^[ \t]*[A-Za-z0-9_]+[ \t]*\(\)/ {
  name = $0
  sub(/^[ \t]*/, "", name)
  sub(/[^A-Za-z0-9_].*/, "", name)
  if ((name " ") ~ ("^(" families ")_[0-9]+[^A-Za-z0-9]")) claim(name " ", FILENAME ":" FNR)
}
mode == "hdr" && /^[ \t]*\/\/!/ {
  s = $0
  while (match(s, /[A-Z]+-[0-9]+/)) {
    before = (RSTART > 1) ? substr(s, RSTART - 1, 1) : " "
    after = substr(s, RSTART + RLENGTH, 1)
    if (before !~ /[A-Za-z0-9_]/ && after !~ /[A-Za-z0-9_]/) print "impl", substr(s, RSTART, RLENGTH)
    s = substr(s, RSTART + RLENGTH)
  }
}
'

dirs=src
[ -d tests ] && dirs="src tests"
{
  # shellcheck disable=SC2086 # $dirs is a fixed, space-separated list of directories
  find $dirs -type f -name '*.rs' -exec awk -v mode=rs -v families="$families" "$extract" {} +
  find src -type f -name '*.rs' -exec awk -v mode=hdr -v families="$families" "$extract" {} +
  if [ -f scripts/e2e.sh ]; then
    awk -v mode=sh -v families="$families" "$extract" scripts/e2e.sh
  fi
} > "$tmp/found"

# 3. Join everything, print the table and decide.
cat "$tmp/spec" "$tmp/found" | awk -v current="$current" -v milestone="$milestone" '
  $1 == "nomilestone" { bad[++nbad] = "SPEC line for " $2 " has no [Mn] milestone" }
  $1 == "spec" {
    if ($2 in ms) bad[++nbad] = "SPEC lists " $2 " twice"
    ids[++n] = $2
    ms[$2] = $3
  }
  $1 == "claim" {
    claims[++nclaims] = $2 " " $3 " " $4
    if ($3 == 1) tests[$2]++
  }
  $1 == "impl" { impl[$2] = 1 }
  END {
    printf "%-9s %-9s %5s  %s\n", "ID", "milestone", "tests", "impl"
    for (i = 1; i <= n; i++) {
      id = ids[i]
      note = ""
      if (ms[id] + 0 <= current + 0) {
        required++
        if (tests[id] + 0 == 0) { notest++; note = note "  <- no test" }
        if (!(id in impl)) { noimpl++; note = note "  <- not in any //! header" }
      }
      printf "%-9s M%-8s %5d  %s%s\n", id, ms[id], tests[id] + 0, (id in impl) ? "yes" : "no", note
    }
    for (i = 1; i <= nclaims; i++) {
      split(claims[i], c, " ")
      if (!(c[1] in ms)) {
        unknown++
        bad[++nbad] = "test " c[3] " is named after " c[1] ", which is not in SPEC"
      }
    }
    print ""
    for (i = 1; i <= nbad; i++) print "FAIL: " bad[i]
    printf "trace: %d IDs, milestone %s: %d required, %d without tests, %d not implemented, %d unknown\n",
      n, milestone, required + 0, notest + 0, noimpl + 0, unknown + 0
    if (nbad > 0 || notest > 0 || noimpl > 0) {
      print "trace: FAILED"
      exit 1
    }
    print "trace: ok"
  }
'

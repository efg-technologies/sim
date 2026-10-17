#!/bin/sh
# Checks that SPEC, code and tests line up (see "Checks" in CLAUDE.md).
#
# For every requirement ID in SPEC.md it counts the tests whose name starts with the ID (Rust
# `fn safe_1_...` in core/src, core/tests, sim/src and sim/tests; shell `safe_1_...()` in
# core/scripts/e2e.sh and sim/scripts/e2e.sh) and looks for the ID in the `//!` header of some
# module in core/src or sim/src.
# It fails when, for IDs up to the milestone in .milestone:
#   - an ID has no test,
#   - an ID is not listed in any `//!` header,
# or when any test is named after an ID that is not in SPEC.
# The milestones are two lines of work in one SPEC: M for the core and S for the editor, and
# .milestone holds the current one of each (`M5 S5`).
# POSIX sh + awk only, so it also runs on macOS.
set -eu

cd "$(dirname "$0")/.."

for f in SPEC.md .milestone; do
  [ -f "$f" ] || { echo "trace: $f not found" >&2; exit 1; }
done

# The current milestone of each line of work, e.g. `M5 S5` -> "M=5 S=5".
milestone=$(tr -s ' \t\r\n' ' ' < .milestone | sed 's/^ //; s/ $//')
current=
for m in $milestone; do
  case $m in
    [A-Z][0-9]*) n=${m#?} ;;
    *) n=x ;;
  esac
  case $n in
    "" | *[!0-9]*)
      echo "trace: .milestone must look like 'M5 S5', found '$milestone'" >&2
      exit 1
      ;;
  esac
  current="$current ${m%"$n"}=$n"
done

tmp=$(mktemp -d "${TMPDIR:-/tmp}/sim-trace.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

# Test-name families; IDs look like SAFE-1 and test names like safe_1_...
families=${TRACE_FAMILIES:-'cli|safe|disk|filt|vis|vi|perf|val|plain|state|tree|buf|find|grep|mark|syn|ptr|look|speed|ui|color|icon|pick|key'}

# 1. IDs and milestones from SPEC.md, in file order. Indented sub-items are skipped.
awk '
  /^- \*\*[A-Z]+-[0-9]+\*\* / {
    id = $2
    gsub(/\*/, "", id)
    if (match($0, /\[[MS][0-9]+\]/)) {
      print "spec", id, substr($0, RSTART + 1, 1), substr($0, RSTART + 2, RLENGTH - 3)
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

{
  find core/src core/tests sim/src sim/tests -type f -name '*.rs' -exec awk -v mode=rs -v families="$families" "$extract" {} +
  find core/src sim/src -type f -name '*.rs' -exec awk -v mode=hdr -v families="$families" "$extract" {} +
  for e2e in core/scripts/e2e.sh sim/scripts/e2e.sh; do
    if [ -f "$e2e" ]; then
      awk -v mode=sh -v families="$families" "$extract" "$e2e"
    fi
  done
} > "$tmp/found"

# 3. Join everything, print the table and decide.
cat "$tmp/spec" "$tmp/found" | awk -v current="$current" -v milestone="$milestone" '
  BEGIN {
    n = split(current, cur, " ")
    for (i = 1; i <= n; i++) {
      split(cur[i], kv, "=")
      now[kv[1]] = kv[2] + 0
    }
  }
  $1 == "nomilestone" { bad[++nbad] = "SPEC line for " $2 " has no [M1] or [S1] milestone" }
  $1 == "spec" {
    if ($2 in ms) bad[++nbad] = "SPEC lists " $2 " twice"
    if (!($3 in now)) bad[++nbad] = "SPEC gives " $2 " the milestone " $3 $4 ", and .milestone has no " $3
    ids[++n2] = $2
    ms[$2] = $4 + 0
    pre[$2] = $3
  }
  $1 == "claim" {
    claims[++nclaims] = $2 " " $3 " " $4
    if ($3 == 1) tests[$2]++
  }
  $1 == "impl" { impl[$2] = 1 }
  END {
    printf "%-9s %-9s %5s  %s\n", "ID", "milestone", "tests", "impl"
    for (i = 1; i <= n2; i++) {
      id = ids[i]
      note = ""
      if (ms[id] <= now[pre[id]]) {
        required++
        if (tests[id] + 0 == 0) { notest++; note = note "  <- no test" }
        if (!(id in impl)) { noimpl++; note = note "  <- not in any //! header" }
      }
      printf "%-9s %s%-8s %5d  %s%s\n", id, pre[id], ms[id], tests[id] + 0, (id in impl) ? "yes" : "no", note
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
      n2, milestone, required + 0, notest + 0, noimpl + 0, unknown + 0
    if (nbad > 0 || notest > 0 || noimpl > 0) {
      print "trace: FAILED"
      exit 1
    }
    print "trace: ok"
  }
'

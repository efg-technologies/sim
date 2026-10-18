#!/bin/sh
# Packs the lessons of `sim/tutor/<lang>.txt` into `sim/tutor/tutor.deflate`, which the binary holds and
# `:tutor` unpacks (TUTOR-4). The text in the files is what is read and reviewed; the test
# `tutor_4_the_packed_lessons_are_the_text_of_the_files_and_smaller_than_it` fails when the packed
# file is not exactly their text, so run this after a lesson changes. Needs only `gzip`: its stream
# is the header (10 bytes, no name or time with -n), the deflated data, and a trailer (8 bytes).
set -eu

cd "$(dirname "$0")/../sim/tutor"
tmp=$(mktemp "${TMPDIR:-/tmp}/sim-tutor.XXXXXX")
trap 'rm -f "$tmp" "$tmp.gz"' EXIT HUP INT TERM
# (the order of LANGS in sim/src/tutor.rs; \001 starts each language)
for lang in en ja zh-cn zh-tw ko es pt-br fr de ru; do
  printf '\001%s\n' "$lang"
  cat "$lang.txt"
done > "$tmp"
gzip -9 -n -c "$tmp" > "$tmp.gz"
size=$(wc -c < "$tmp.gz")
tail -c +11 "$tmp.gz" | head -c $((size - 18)) > tutor.deflate
echo "sim/tutor/tutor.deflate: $(wc -c < tutor.deflate) bytes from $(wc -c < "$tmp")"

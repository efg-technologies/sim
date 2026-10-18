#!/bin/sh
# Runs every check that must pass before a commit (see "Checks" in CLAUDE.md).
# Stops at the first failure and says which step failed.
set -eu

cd "$(dirname "$0")/.."

# The core (core/src) is the part that must never lose text: its own hard cap and its own allowlist
# of dependencies. The editor (sim/src) and the window (gui/src) have a cap each; the reasons for all
# three are in CONCEPT.md.
MAX_CORE_LINES=3200
MAX_SIM_LINES=11160
MAX_GUI_LINES=5100
CORE_DEPS="crossterm notify regex-lite ropey rustix similar unicode-width"
# The editor's own direct dependencies (the core included), as they were before the window: the window
# is never among them, and nothing it needs comes in through them.
SIM_DEPS="crossterm regex-lite ropey rustix signal-hook sim-editor-core syntect two-face unicode-width"
# The window's direct dependencies are a closed list (GUI-17): no webview, nothing that speaks to a network.
GUI_DEPS="bytemuck cosmic-text crossterm png pollster sim-editor sim-editor-core wgpu winit"
# Crates that exist for the window. None may be in the tree of the editor or of the core.
GUI_ONLY="winit wgpu wgpu-core wgpu-hal wgpu-types naga ash cosmic-text swash fontdb bytemuck pollster png sim-gui"
# Crates that speak to a network, or are a browser, or its toolkit. None may be in the window's tree
# (`mio` is: the poll primitive under crossterm and notify, which are sim's own dependencies).
GUI_FORBIDDEN="reqwest hyper ureq curl isahc surf attohttpc h2 http openssl openssl-sys rustls native-tls tokio async-std socket2 quinn tungstenite websocket smol webkit2gtk wry tao tauri gtk cef servo hickory-resolver trust-dns-resolver"
MAX_BYTES=$((5 * 1024 * 1024))

N=13
step() {
  n=$1
  shift
  printf '\n[%s/%s] %s\n' "$n" "$N" "$*"
  "$@" || {
    printf 'check.sh: step %s failed: %s\n' "$n" "$*" >&2
    exit 1
  }
}

# Lines of Rust under $1: blank lines and lines that start with `//` (`///` and `//!` too) don't count.
# Each file is followed by an empty line so that a missing final newline can't join two files.
count() {
  find "$1" -type f -name '*.rs' | while read -r f; do
    cat "$f"
    echo
  done | awk '!/^[ \t]*$/ && !/^[ \t]*\/\// { n++ } END { print n + 0 }'
}

step 1 cargo fmt --check
step 2 cargo clippy --workspace --all-targets -- -D warnings
# The tests that run git never read the user's git configuration (and never write it). Without
# --no-fail-fast cargo stops at the first test file that fails and says nothing of the files after it:
# a platform's first run would show one failure and hide the rest.
step 3 env GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null XDG_CONFIG_HOME=/nonexistent cargo test --workspace --no-fail-fast
step 4 sh scripts/trace.sh

printf '\n[5/%s] core/src has at most %s lines\n' "$N" "$MAX_CORE_LINES"
lines=$(count core/src)
echo "core/src: $lines lines (limit $MAX_CORE_LINES)"
if [ "$lines" -gt "$MAX_CORE_LINES" ]; then
  echo "check.sh: step 5 failed: core/src has $lines lines; propose features to cut" >&2
  exit 1
fi

printf '\n[6/%s] sim/src has at most %s lines\n' "$N" "$MAX_SIM_LINES"
lines=$(count sim/src)
echo "sim/src: $lines lines (limit $MAX_SIM_LINES)"
if [ "$lines" -gt "$MAX_SIM_LINES" ]; then
  echo "check.sh: step 6 failed: sim/src has $lines lines; propose features to cut" >&2
  exit 1
fi

printf '\n[7/%s] gui/src has at most %s lines\n' "$N" "$MAX_GUI_LINES"
lines=$(count gui/src)
echo "gui/src: $lines lines (limit $MAX_GUI_LINES)"
if [ "$lines" -gt "$MAX_GUI_LINES" ]; then
  echo "check.sh: step 7 failed: gui/src has $lines lines; propose features to cut, or a seam in sim for what it keeps" >&2
  exit 1
fi

# The direct dependencies of a package, sorted (byte order, whatever the locale), on one line.
direct() {
  cargo tree -p "$1" -e normal --depth 1 --prefix none | awk 'NR > 1 { print $1 }' | LC_ALL=C sort -u | tr '\n' ' ' | sed 's/ $//'
}
# Every crate in the resolved tree of a package (normal and build edges, for this host), one per line.
tree() {
  cargo tree -p "$1" -e normal,build --prefix none | awk '{ print $1 }' | LC_ALL=C sort -u
}

# 8. The core depends on nothing but its allowlist (tests/safe_9.rs reads the manifest; this reads
# what cargo resolved).
printf '\n[8/%s] the core depends on %s only\n' "$N" "$CORE_DEPS"
deps=$(direct sim-editor-core)
echo "core: $deps"
if [ "$deps" != "$CORE_DEPS" ]; then
  echo "check.sh: step 8 failed: the core depends on [$deps], not [$CORE_DEPS]; write the reason into SPEC first" >&2
  exit 1
fi

# 9. The window never leaks into the program: the editor's direct dependencies are what they were, and
# no crate that exists for the window is in the tree of the editor or of the core (so the binary `sim`
# is built of the same crates as before).
printf '\n[9/%s] the editor depends on %s only, and no window crate is in the tree of the editor or of the core\n' "$N" "$SIM_DEPS"
deps=$(direct sim-editor)
echo "editor: $deps"
if [ "$deps" != "$SIM_DEPS" ]; then
  echo "check.sh: step 9 failed: the editor depends on [$deps], not [$SIM_DEPS]; the window's crates live in gui/Cargo.toml (GUI-17)" >&2
  exit 1
fi
for pkg in sim-editor sim-editor-core; do
  all=$(tree "$pkg")
  for name in $GUI_ONLY; do
    if printf '%s\n' "$all" | grep -qx "$name"; then
      echo "check.sh: step 9 failed: $name, a crate of the window, is in the tree of $pkg (GUI-17)" >&2
      exit 1
    fi
  done
done

# 10. The window's own allowlist, and what may not be in its tree at all.
printf '\n[10/%s] the window depends on %s only, and no network crate or webview is in its tree\n' "$N" "$GUI_DEPS"
deps=$(direct sim-gui)
echo "window: $deps"
if [ "$deps" != "$GUI_DEPS" ]; then
  echo "check.sh: step 10 failed: the window depends on [$deps], not [$GUI_DEPS]; write the reason into SPEC (GUI-17) first" >&2
  exit 1
fi
all=$(tree sim-gui)
echo "window tree: $(printf '%s\n' "$all" | wc -l | tr -d ' ') crates"
for name in $GUI_FORBIDDEN; do
  if printf '%s\n' "$all" | grep -qx "$name"; then
    echo "check.sh: step 10 failed: $name is in the tree of the window (GUI-17: no webview, nothing that can speak to a network)" >&2
    exit 1
  fi
done

# 11. The name: sim is Secure vIM, and si, once a program, is only its core now. The crates are
# sim-editor and sim-editor-core. What this repository said before must not come back (this file
# names the patterns, so it is left out).
printf '\n[11/%s] the name is Secure vIM, the crates sim-editor and sim-editor-core\n' "$N"
old=$(grep -rIn -E 'SI iMproved|SIght|sim-core' . --exclude-dir=target --exclude-dir=.git --exclude-dir=vendor --exclude=check.sh || true)
if [ -n "$old" ]; then
  printf '%s\n' "$old" >&2
  echo "check.sh: step 11 failed: the name sim was given before (SI iMproved, SIght) or the crate sim-core is back" >&2
  exit 1
fi

# 12. Size of the release binary `sim` (the window is another binary and is not in it: only the
# editor is built, so no feature of the window's crates is unified into it).
step 12 cargo build --release -p sim-editor
bin=${CARGO_TARGET_DIR:-target}/release/sim
bytes=$(wc -c < "$bin" | tr -d ' ')
echo "$bin: $bytes bytes (limit $MAX_BYTES)"
if [ "$bytes" -gt "$MAX_BYTES" ]; then
  echo "check.sh: step 12 failed: $bin is $bytes bytes, over $MAX_BYTES" >&2
  exit 1
fi

# 13. The copy of syntect that the highlighter is built on is upstream's 5.3.0 and the patch of
# vendor/syntect.patch, and nothing else (vendor/README.md).
step 13 sh scripts/vendor-check.sh

printf '\ncheck.sh: all checks passed\n'

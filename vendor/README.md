# vendor/syntect

syntect 5.3.0, the syntax highlighter that sim's `Syn` is built on, with one change: a guard in the parser that
stops it going round in a circle on matches that consume nothing (SPEC SYN-3, NOTES). The workspace's
`[patch.crates-io]` points at this directory, so `two-face` (bat's syntaxes) and `sim-editor` use the same
copy (`cargo tree -i syntect` shows one). It is not a workspace member (`exclude` in the root Cargo.toml), so
`cargo fmt`, `cargo clippy --workspace` and `cargo test --workspace` do not run on it.

## What is here

- Origin: the crate `syntect` 5.3.0 from crates.io (<https://github.com/trishume/syntect>, tag v5.3.0), the file
  `syntect-5.3.0.crate`, whose sha256 is
  `656b45c05d95a5704399aeef6bd0ddec7b2b3531b7c9e900abbf7c4d2190c925` (it was `Cargo.lock`'s checksum for
  syntect before this directory).
- `syntect/`: the crate's files that are used, as upstream has them, but for `src/parsing/parser.rs`.
- `syntect.patch`: our change, a unified diff against upstream (`patch -R -p1` in `syntect/` gives upstream's file).
- `syntect.sha256`: the sha256 of every file of `syntect/` as upstream has it, and so as `patch -R` leaves it.
- Left out, since sim builds syntect with `default-features = false` and nothing reads them: the four
  `assets/*.packdump` and `.themedump` files (syntect's own default syntaxes and themes, behind the features
  `default-syntaxes` and `default-themes`), `.github/`, `.cargo_vcs_info.json`, `.git-blame-ignore-revs`,
  `.gitattributes`, `.gitignore`, `.gitmodules`, `Cargo.lock` and `Cargo.toml.orig`. The manifest still
  names `examples/`, `benches/` and `tests/`, so they stay (cargo would not read it without them).
- Licence: syntect is MIT; `syntect/LICENSE.txt` stays.

## The change

`MAX_EMPTY_RUN` (128) in `parser.rs`: more than that many matches that consume nothing at one position, and the
parser goes on one char without making the match, as Sublime Text does for the loop it detects (syntect's
`would_loop` knows only a push followed by a pop). The reason, the measurements and the number are in SPEC
SYN-3. The change is also written for upstream (a pull request is not made: that is the owner's to decide).

## `scripts/vendor-check.sh`

`scripts/check.sh` runs it: a copy of `syntect/` with the patch taken off must be, file for file, what
`syntect.sha256` lists, and no file may be there that the list does not have, nor one that is not a regular file (a link, a fifo). A change to this directory that is
not in `syntect.patch` fails it.

## Updating

1. Take the new crate from crates.io (`cargo download` or the registry's cache) and write its sha256 here.
2. Copy the files that are used into a clean directory, and write `syntect.sha256` from them
   (`LC_ALL=C sha256sum $(find . -type f | LC_ALL=C sort)` in it).
3. Apply the change again (`patch -p1 < syntect.patch`, or by hand if upstream's parser moved), and make
   `syntect.patch` with `diff -u --label a/<file> --label b/<file> upstream changed` for each changed file.
4. Put the new directory in `syntect/`, run `sh scripts/vendor-check.sh`, and the fuzz of the whole set
   (`cargo test -p sim-editor --test syn_hang -- --ignored`).
5. When upstream has a guard of its own, delete this directory, the `[patch]` and the `exclude`.

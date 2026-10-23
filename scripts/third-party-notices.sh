#!/usr/bin/env bash
# Generates THIRD-PARTY-NOTICES.md: the embedded font (Hanken Grotesk, OFL) and every third-party crate linked into the
# natikube binary on macOS arm64 or Windows x64 (normal dependencies only),
# with its license expression and the license/notice files shipped in its
# source. Identical license texts are written once and referenced by id.
#
# Needs: cargo, jq, shasum (or sha256sum), and the crate sources in the cargo
# registry (run `cargo fetch` first on a clean machine).
# Usage: scripts/third-party-notices.sh [output-file]
#   default output: packaging/THIRD-PARTY-NOTICES.md
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/Cargo.toml"
OUT="${1:-$ROOT/packaging/THIRD-PARTY-NOTICES.md}"
TARGETS=(aarch64-apple-darwin x86_64-pc-windows-msvc)

if command -v shasum >/dev/null 2>&1; then
  hash_file() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  hash_file() { sha256sum "$1" | cut -d' ' -f1; }
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/texts"

# 1. Crates linked for either target: "name version" per line. Path sources
#    (the workspace's own crates) are skipped; they are not third party.
: > "$WORK/linked.txt"
for target in "${TARGETS[@]}"; do
  cargo tree --manifest-path "$MANIFEST" -p natikube-app -e normal \
    --target "$target" --prefix none --format '{p}' \
    | grep -v ' (/' | awk '{ v = $2; sub(/^v/, "", v); print $1 " " v }' >> "$WORK/linked.txt"
done
sort -u "$WORK/linked.txt" -o "$WORK/linked.txt"

# 2. Package metadata (every platform), keyed by name + version.
cargo metadata --manifest-path "$MANIFEST" --format-version 1 > "$WORK/meta.json"
jq -r '.packages[] | [.name, .version, (.license // "UNKNOWN"), (.repository // "-"), .manifest_path, ((.authors // []) | join("; ") | if . == "" then "-" else . end)] | @tsv' \
  "$WORK/meta.json" > "$WORK/packages.tsv"

{
  echo "# Third-party notices"
  echo
  echo "NatiKube is released under the MIT license (see LICENSE). It links the"
  echo "open-source crates listed below, for the macOS arm64 and Windows x64"
  echo "builds. Each crate remains under its own license; the license texts"
  echo "are reproduced in the appendix and referenced by id."
  echo
  echo "## Crates"
  echo
} > "$OUT"

# Canonical Apache-2.0 text, taken from a crate that ships it verbatim.
APACHE_TEXT="$(jq -r '.packages[] | select(.name == "serde_json") | .manifest_path' "$WORK/meta.json" | head -1)"
APACHE_TEXT="$(dirname "$APACHE_TEXT")/LICENSE-APACHE"
[[ -f "$APACHE_TEXT" ]] || { echo "missing canonical Apache text at $APACHE_TEXT" >&2; exit 1; }
MIT_BODY='Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.'

missing=0
generated=0
count=0
while read -r name version; do
  row="$(awk -F'\t' -v n="$name" -v v="$version" '$1 == n && $2 == v { print; exit }' "$WORK/packages.tsv")"
  [[ -n "$row" ]] || { echo "no metadata for $name $version" >&2; exit 1; }
  IFS=$'\t' read -r _ _ license repo manifest_path authors <<< "$row"
  [[ "$repo" != "-" ]] || repo=""
  dir="$(dirname "$manifest_path")"
  ids=()
  for f in "$dir"/LICENSE* "$dir"/LICENCE* "$dir"/COPYING* "$dir"/NOTICE* "$dir"/UNLICENSE*; do
    [[ -f "$f" ]] || continue
    h="$(hash_file "$f")"
    id="L-${h:0:10}"
    if [[ ! -f "$WORK/texts/$id" ]]; then
      cp "$f" "$WORK/texts/$id"
    fi
    echo "$id" >> "$WORK/users-$id"
    ids+=("$id")
  done
  if ((${#ids[@]} == 0)); then
    # Some workspace crates are published without their license files. The
    # license is still declared in the manifest, so reproduce the standard text
    # for the declared MIT / Apache-2.0 option and say so.
    if [[ "$license" == *MIT* ]]; then
      printf 'MIT License\n\nCopyright (c) %s\n\n%s\n' "$authors" "$MIT_BODY" > "$WORK/mit.tmp"
      h="$(hash_file "$WORK/mit.tmp")"; id="L-${h:0:10}"
      [[ -f "$WORK/texts/$id" ]] || cp "$WORK/mit.tmp" "$WORK/texts/$id"
      echo "$id" >> "$WORK/users-$id"; ids+=("$id")
    fi
    if [[ "$license" == *Apache* ]]; then
      h="$(hash_file "$APACHE_TEXT")"; id="L-${h:0:10}"
      [[ -f "$WORK/texts/$id" ]] || cp "$APACHE_TEXT" "$WORK/texts/$id"
      echo "$id" >> "$WORK/users-$id"; ids+=("$id")
    fi
    [[ ${#ids[@]} -eq 0 ]] || generated=$((generated + 1))
  fi
  {
    echo "- **$name $version** - $license${repo:+ - <$repo>}"
    if ((${#ids[@]})); then
      printf '  - texts: %s\n' "$(printf '%s, ' "${ids[@]}" | sed 's/, $//')"
    else
      echo "  - texts: none shipped in the crate source; see the repository for the $license terms"
      missing=$((missing + 1))
    fi
  } >> "$OUT"
  count=$((count + 1))
done < "$WORK/linked.txt"

# Fonts embedded in the app (not crates): Hanken Grotesk, SIL OFL 1.1.
FONT_OFL="$ROOT/assets/icon/HankenGrotesk-OFL.txt"
[[ -f "$FONT_OFL" ]] || { echo "missing font license at $FONT_OFL" >&2; exit 1; }
{
  echo
  echo "## Fonts"
  echo
  echo "- **Hanken Grotesk** - SIL Open Font License 1.1 - <https://github.com/marcologous/hanken-grotesk>"
  echo "  - $(head -1 "$FONT_OFL")"
  echo
  echo "Full license text:"
  echo
  echo '````text'
  cat "$FONT_OFL"
  [[ -z "$(tail -c1 "$FONT_OFL")" ]] || echo
  echo '````'
} >> "$OUT"

{
  echo
  echo "## Appendix: license texts"
  for t in "$WORK"/texts/L-*; do
    id="$(basename "$t")"
    echo
    echo "### $id"
    echo
    echo "Used by: $(sort -u "$WORK/users-$id" | wc -l | tr -d ' ') crate file(s)."
    echo
    echo '````text'
    cat "$t"
    # Guarantee the closing fence starts on its own line.
    [[ -z "$(tail -c1 "$t")" ]] || echo
    echo '````'
  done
} >> "$OUT"

echo "wrote $OUT: $count crates, $(ls "$WORK/texts" | wc -l | tr -d ' ') distinct texts, $generated with standard texts generated from the manifest, $missing with none"

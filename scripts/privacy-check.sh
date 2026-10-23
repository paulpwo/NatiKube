#!/usr/bin/env bash
# Privacy check for the publishable tree (code, docs, mockups, scripts).
#
# Fails when any publishable file contains
#   - a word from the deny-list file named by $NATIKUBE_PRIVACY_DENYLIST
#     (one entry per line, case-insensitive, fixed string; the file stays
#     local and untracked, so no real name ever enters the repository), or
#   - a built-in sensitive pattern: an AWS ARN carrying a 12-digit account
#     id, an ECR registry host, or an "account id" label followed by 12 digits.
#
# Usage: NATIKUBE_PRIVACY_DENYLIST=/path/to/names.txt scripts/privacy-check.sh
# Exit status: 0 clean, 1 hits found, 2 usage error.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
hits=0

# Publishable files: tracked + untracked, minus .gitignore, minus generated
# notices/lockfiles and this script (it names the patterns it forbids).
list_files() {
    git -C "$root" ls-files --cached --others --exclude-standard -z |
        while IFS= read -r -d '' f; do
            case "$f" in
                Cargo.lock | THIRD-PARTY-NOTICES.md | packaging/THIRD-PARTY-NOTICES.md | scripts/privacy-check.sh) continue ;;
            esac
            [ -f "$root/$f" ] && printf '%s\n' "$f"
        done
}

files="$(list_files)"

builtin_patterns=(
    'arn:aws[a-z-]*:[a-z0-9-]*:[a-z0-9-]*:[0-9]{12}'
    '[0-9]{12}\.dkr\.ecr\.'
    'account[ _-]?id[^0-9]{0,6}[0-9]{12}'
)
for pat in "${builtin_patterns[@]}"; do
    while IFS= read -r f; do
        # An all-zero account id is the obvious placeholder used in fixtures.
        if grep -E -i -o -e "$pat" "$root/$f" 2>/dev/null | grep -v -E '(^|[^0-9])0{12}([^0-9]|$)' | grep -q .; then
            echo "privacy: built-in pattern /$pat/ in $f"
            hits=$((hits + 1))
        fi
    done <<<"$files"
done

denylist="${NATIKUBE_PRIVACY_DENYLIST:-}"
if [ -n "$denylist" ]; then
    if [ ! -r "$denylist" ]; then
        echo "privacy: NATIKUBE_PRIVACY_DENYLIST is not readable" >&2
        exit 2
    fi
    words="$(grep -v -E '^\s*(#|$)' "$denylist" || true)"
    while IFS= read -r f; do
        while IFS= read -r w; do
            [ -n "$w" ] || continue
            if grep -q -i -F -e "$w" "$root/$f"; then
                # Never echo the denied word itself: it is a real name.
                echo "privacy: deny-list entry hit in $f"
                hits=$((hits + 1))
            fi
        done <<<"$words"
    done <<<"$files"
else
    echo "privacy: NATIKUBE_PRIVACY_DENYLIST not set; only built-in patterns checked" >&2
fi

if [ "$hits" -gt 0 ]; then
    echo "privacy: $hits hit(s)"
    exit 1
fi
echo "privacy: clean"

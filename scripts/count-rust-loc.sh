#!/usr/bin/env bash
# ─── count-rust-loc.sh ───────────────────────────────────────────────
# Tallies lines of Rust across the OXIDE OS repo.
# Excludes target/, node_modules/, and external/ build artifacts.
# — NightDoc: because someone always asks "how big is this thing?"
# ─────────────────────────────────────────────────────────────────────
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

EXCLUDE_DIRS="target node_modules"

# ── helpers ──────────────────────────────────────────────────────────
colorize() { printf "\033[1;36m%s\033[0m" "$1"; }
bold()     { printf "\033[1m%s\033[0m" "$1"; }

# Build find exclusion flags
FIND_PRUNE=""
for d in $EXCLUDE_DIRS; do
    FIND_PRUNE="$FIND_PRUNE -path '*/$d' -prune -o"
done

# ── collect stats per top-level directory ────────────────────────────
declare -A dir_files dir_lines dir_blank dir_comment dir_code
total_files=0 total_lines=0 total_blank=0 total_comment=0 total_code=0

while IFS= read -r file; do
    # Determine top-level component (kernel, bootloader, userspace, …)
    rel="${file#$REPO_ROOT/}"
    component="${rel%%/*}"

    lines=$(wc -l < "$file")
    blank=$(grep -cE '^\s*$' "$file" || true)
    comment=$(grep -cE '^\s*//' "$file" || true)
    code=$((lines - blank - comment))

    dir_files[$component]=$(( ${dir_files[$component]:-0} + 1 ))
    dir_lines[$component]=$(( ${dir_lines[$component]:-0} + lines ))
    dir_blank[$component]=$(( ${dir_blank[$component]:-0} + blank ))
    dir_comment[$component]=$(( ${dir_comment[$component]:-0} + comment ))
    dir_code[$component]=$(( ${dir_code[$component]:-0} + code ))

    total_files=$((total_files + 1))
    total_lines=$((total_lines + lines))
    total_blank=$((total_blank + blank))
    total_comment=$((total_comment + comment))
    total_code=$((total_code + code))
done < <(eval "find '$REPO_ROOT' $FIND_PRUNE -name '*.rs' -print")

# ── render table ─────────────────────────────────────────────────────
printf "\n"
bold "OXIDE OS — Rust Lines of Code"
printf "\n"
printf "%-20s %8s %10s %8s %10s %10s\n" \
       "Component" "Files" "Total" "Blank" "Comment" "Code"
printf "%-20s %8s %10s %8s %10s %10s\n" \
       "────────────" "─────" "───────" "─────" "───────" "──────"

# Sort components alphabetically
for component in $(echo "${!dir_files[@]}" | tr ' ' '\n' | sort); do
    printf "%-20s %8d %10d %8d %10d %10d\n" \
        "$component" \
        "${dir_files[$component]}" \
        "${dir_lines[$component]}" \
        "${dir_blank[$component]}" \
        "${dir_comment[$component]}" \
        "${dir_code[$component]}"
done

printf "%-20s %8s %10s %8s %10s %10s\n" \
       "────────────" "─────" "───────" "─────" "───────" "──────"
printf "%-20s %8d %10d %8d %10d %10d\n" \
       "TOTAL" "$total_files" "$total_lines" "$total_blank" "$total_comment" "$total_code"
printf "\n"

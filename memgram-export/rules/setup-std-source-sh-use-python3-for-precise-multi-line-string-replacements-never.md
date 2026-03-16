# setup-std-source.sh: use python3 for precise multi-line string replacements, never sed for owned.rs/raw.rs

🟡 preference | ✅ do

| Field | Value |
|-------|-------|
| ID | `b1bb158d448c` |
| Reinforced | ×1 |
| Project | oxide-os |
| Branch | - |
| Created | 2026-03-04T11:31:05.190310+00:00 |
| Keywords | setup-std-source, sed, python3, owned.rs, raw.rs, os/fd, moto_rt, oxide_rt |
| Session | [1865cc6a7d76](../sessions/fix-setup-std-source-sh-so-clean-rebuild-works-oxide-must-have-separate-code-pat.md) |

## Details

The os/fd/raw.rs and os/fd/owned.rs files in Rust's std have multiple #[cfg(target_os = "motor")] lines at different indentation levels with different purposes (libc import, OwnedFd import, cvt exclusion, try_clone exclusion, Drop close). Using sed to pattern-match 'target_os = "motor"' changes ALL of them indiscriminately, causing oxide to incorrectly reference moto_rt. Instead, use python3 inline in bash with exact multi-line string.replace() to target specific occurrences by surrounding context. Key gotchas: (1) #[cfg(not(any(...)))] has THREE closing parens, not two. (2) owned.rs uses 4-space indent inside impl blocks, 8-space inside cfg gates. (3) oxide must have SEPARATE code paths from motor — never share moto_rt references.

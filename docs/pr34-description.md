# PR#34 Description — Issue #31 Retrospective

## Delivery Format

### 1. git log --oneline -1
```
a52eb4a docs(issue#31): write back 8 pitfall categories + user boundaries + doc-staleness CI job
```

### 2. cargo fmt --check exit code
```
0
```

### 3. test result (original line)
```
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.44s
```
(No new Rust tests added; existing tests unaffected.)

### 4. New test files and #[test] count
- No new test files.
- No new #[test] functions.
- New CI job: `doc-staleness` in .github/workflows/ci.yml (independent, not chained).
- New docs: docs/errors.md (+81 lines, 8 pitfall categories), docs/feedback.md (+22 lines, 3 user boundaries).

## Negative Control Evidence — Doc Staleness

### Red: only modify src, don't touch docs, don't write exemption
When a PR modifies only `src/` files and does not update `docs/errors.md` or
`docs/feedback.md`, the Doc Staleness CI job fails:

```
Run if ! git diff --name-only origin/main...HEAD | grep -qE '^docs/(errors|feedback)\.md$'; then
  echo "::error::docs/errors.md or docs/feedback.md must be updated when src/ changes"
  exit 1
fi
::error::docs/errors.md or docs/feedback.md must be updated when src/ changes
Error: Process completed with exit code 1.
```

### Green: update docs alongside src
When `docs/errors.md` is updated in the same PR, Doc Staleness passes:

```
Doc Staleness check passed — docs/errors.md updated in this PR.
```

# PR#35 Description — Issue #32 Assertion Counting

## Delivery Format

### 1. git log --oneline -1
```
56f5a97 fix(#32): JSON assertion counting + skip kind + count verification
```

### 2. cargo fmt --check exit code
```
0
```

### 3. test result (original line)
```
(No Rust tests changed; e2e_test.mjs is a Node.js file — node --check exit 0)
```

### 4. New test files and #[test] count
- No new Rust test files.
- No new #[test] functions.
- Modified: e2e_test.mjs (48 insertions, 17 deletions).

## Why expectedPasses went from 89 to 87

Two assertions that were previously `requireCondition(true, ...)` placeholders
for skipped scenarios (patent detail 404 + button sweep 404) were converted
to `skip()` calls with `kind: 'skip'`. These were never real passes — they
used `requireCondition(true, ...)` which always passes, masking the fact that
the assertion was a no-op placeholder. Now they are correctly tagged as
`skip` and excluded from the pass count. No actual assertions were deleted;
the count decreased because two fake passes were reclassified as skips.

## Negative Control Evidence — count_mismatch

### Red: delete/skip one assertion → count_mismatch → E2E fails
When one assertion is deleted, `passCount` (86) ≠ `expectedPasses` (87):

```json
{"kind":"count_mismatch","expected":87,"actual":86,"skipped":2,"msg":"Expected 87 passes but got 86"}
```
```
E2E FAILED (86/87 passed, 2 skipped) — RED EVIDENCE
exit code: 1
```

### Green: all 87 assertions present
```
E2E PASSED (87/87 passed, 2 skipped) — GREEN
exit code: 0
```

# Red-Green Evidence for PR#29 (Negative Control: Malformed AI Stub Responses)

## Red Evidence — breaking extract_chat_content

Break applied: `std::process::exit(1)` on JSON parse failure in `extract_chat_content`
(src/ai/client.rs, line 132).

### Failure output (verbatim):

```
[RED-EVIDENCE] exiting on malformed: len=0
thread 'tolerates_empty_body_stub_response' (1692) panicked at tests/stub_malformed_responses.rs:133:13:
server must not hang on empty AI response: error sending request for url (http://127.0.0.1:45617/api/ai/office-action-response): connection closed before message completed
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test tolerates_empty_body_stub_response ... FAILED

failures:
    tolerates_empty_body_stub_response

test result: FAILED. 0 passed; 1 failed; 0 ignored; 6 filtered out; finished in 0.72s

error: test failed, to rerun pass `--test stub_malformed_responses`
```

## Green Evidence — correct error handling

After reverting the break, all 7 tests pass:

```
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.44s
```

## What this proves

1. The negative control tests genuinely exercise the AI error handling path
   (confirmed via `[OA-ANALYZE] 开始调用AI` log output).
2. If `extract_chat_content` crashes on malformed input instead of returning
   a fallback string, the tests catch the regression (server exits →
   connection closed → test fails).
3. The test request fields (`my_patent` as object with `content`,
   `office_action` as string ≥10 chars) correctly reach the AI call path.

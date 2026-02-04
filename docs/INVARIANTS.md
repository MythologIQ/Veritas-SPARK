# Main Branch Invariants

## Security

- [ ] No dependencies outside approved list (see Cargo.toml FORBIDDEN DEPENDENCIES)
- [ ] No forbidden modules (auth/, vault/, synapse/, plugins/, network/)
- [ ] IPC validation before allocation
- [ ] No prompt/token logging in production code
- [ ] Constant-time comparison for authentication tokens
- [ ] No hardcoded secrets or tokens

## Limits

| Constant | Value | Location |
|----------|-------|----------|
| MAX_MESSAGE_SIZE | 16 MB | ipc/protocol.rs |
| MAX_PROMPT_TOKENS | TBD | engine/inference.rs |
| MAX_OUTPUT_TOKENS | TBD | engine/inference.rs |
| MAX_QUEUE_DEPTH | 256 | scheduler/queue.rs |
| MAX_CONCURRENT_SESSIONS | TBD | ipc/auth.rs |

## Behavior

- [ ] Fail closed on error (reject invalid input, don't crash)
- [ ] Restartable without state loss (no persistent runtime state)
- [ ] Models treated as untrusted (validate before loading)
- [ ] Graceful degradation on OOM

## Section 4 Razor

- [ ] All functions <= 40 lines
- [ ] All files <= 250 lines
- [ ] All nesting <= 3 levels
- [ ] No nested ternaries

## Build Path Integrity

- [ ] All source files reachable from lib.rs
- [ ] All test files discoverable by cargo test
- [ ] All benchmark files declared in Cargo.toml [[bench]]
- [ ] No orphan files in src/

## CI Gates (Future)

- [ ] cargo build --release succeeds
- [ ] cargo test succeeds
- [ ] cargo clippy --all-targets -- -D warnings succeeds
- [ ] cargo bench compiles (runtime optional)
- [ ] No regression in benchmark baselines (when established)

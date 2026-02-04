# Plan: Tandem Experimental Framework

## Open Questions

1. **Fixture models**: What small model should be used for fast iteration benchmarks? (Need concrete model file path or download source)
2. **CI platform**: GitHub Actions assumed - confirm if different CI system required
3. **Varint library**: Use existing `integer-encoding` crate or implement minimal varint inline?

---

## Phase 1: Benchmark Harness Foundation

### Affected Files

- `core-runtime/Cargo.toml` - Add criterion dev-dependency
- `core-runtime/benches/mod.rs` - Benchmark entry point (NEW)
- `core-runtime/benches/ipc_throughput.rs` - IPC encoding/decoding benchmarks (NEW)
- `core-runtime/benches/scheduler_throughput.rs` - Queue operations benchmarks (NEW)
- `core-runtime/fixtures/prompts/small.json` - 100 token prompt (NEW)
- `core-runtime/fixtures/prompts/medium.json` - 1000 token prompt (NEW)
- `core-runtime/fixtures/prompts/large.json` - 4000 token prompt (NEW)
- `docs/INVARIANTS.md` - Main branch invariant checklist (NEW)

### Changes

**Cargo.toml**: Add benchmark dependencies

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "ipc_throughput"
harness = false

[[bench]]
name = "scheduler_throughput"
harness = false
```

**benches/ipc_throughput.rs**: Measure encode/decode throughput

```rust
// Criterion benchmark measuring:
// - encode_message() bytes/sec for small/medium/large prompts
// - decode_message() bytes/sec for small/medium/large prompts
// - roundtrip latency p50/p95
// Output: machine-readable JSON for CI comparison
```

**benches/scheduler_throughput.rs**: Measure queue operations

```rust
// Criterion benchmark measuring:
// - enqueue operations/sec
// - dequeue operations/sec
// - priority queue reordering overhead
```

**fixtures/prompts/*.json**: Static test prompts

```json
{
  "model_id": "test-model",
  "prompt_tokens": [/* N tokens */],
  "parameters": { "max_tokens": 256, "temperature": 0.7 }
}
```

**docs/INVARIANTS.md**: Enforceable checklist

```markdown
# Main Branch Invariants

## Security
- [ ] No dependencies outside approved list
- [ ] No forbidden modules (auth/, vault/, synapse/, plugins/, network/)
- [ ] IPC validation before allocation
- [ ] No prompt/token logging

## Limits
- [ ] MAX_MESSAGE_SIZE = 16MB
- [ ] MAX_PROMPT_TOKENS defined
- [ ] MAX_OUTPUT_TOKENS defined
- [ ] MAX_QUEUE_DEPTH defined
- [ ] MAX_CONCURRENT_SESSIONS defined

## Behavior
- [ ] Fail closed on error
- [ ] Restartable without state loss
- [ ] Models treated as untrusted
```

### Unit Tests

- `tests/bench_fixtures_test.rs` - Verify fixture files parse correctly, token counts match expected

---

## Phase 2: Protocol Versioning Infrastructure

### Affected Files

- `core-runtime/src/ipc/protocol.rs` - Add version negotiation, keep v1 as default
- `core-runtime/src/ipc/encoding.rs` - Token encoding strategies (NEW)
- `core-runtime/src/ipc/mod.rs` - Export new encoding module
- `core-runtime/tests/protocol_version_test.rs` - Version negotiation tests (NEW)

### Changes

**src/ipc/protocol.rs**: Add protocol version to handshake

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolVersion {
    V1,  // Current: Vec<u32> tokens, JSON encoding
    V2,  // Future: packed tokens
}

impl Default for ProtocolVersion {
    fn default() -> Self { Self::V1 }
}

// Modify Handshake message:
Handshake { token: String, protocol_version: Option<ProtocolVersion> }

// Modify HandshakeAck:
HandshakeAck { session_id: String, protocol_version: ProtocolVersion }
```

**src/ipc/encoding.rs**: Token encoding trait and v1 implementation

```rust
pub trait TokenEncoder {
    fn encode(&self, tokens: &[u32]) -> Vec<u8>;
    fn decode(&self, bytes: &[u8]) -> Result<Vec<u32>, ProtocolError>;
}

pub struct V1Encoder;  // Direct Vec<u32> JSON serialization

impl TokenEncoder for V1Encoder {
    fn encode(&self, tokens: &[u32]) -> Vec<u8> {
        serde_json::to_vec(tokens).unwrap_or_default()
    }
    fn decode(&self, bytes: &[u8]) -> Result<Vec<u32>, ProtocolError> {
        serde_json::from_slice(bytes).map_err(|e| ProtocolError::InvalidFormat(e.to_string()))
    }
}
```

### Unit Tests

- `tests/protocol_version_test.rs` - V1 handshake works unchanged, V2 request falls back gracefully if not implemented
- `tests/encoding_roundtrip_test.rs` - Property test: encode(decode(x)) == x for all token sequences

---

## Phase 3: Packed Token Encoding (Experiment 001)

### Affected Files

- `core-runtime/src/ipc/encoding.rs` - Add V2Encoder with varint packing
- `core-runtime/benches/encoding_comparison.rs` - Compare V1 vs V2 encoding (NEW)
- `core-runtime/tests/encoding_property_test.rs` - Property tests for V2 (NEW)

### Changes

**src/ipc/encoding.rs**: Add V2 encoder

```rust
pub struct V2Encoder;

impl TokenEncoder for V2Encoder {
    fn encode(&self, tokens: &[u32]) -> Vec<u8> {
        // Varint encoding: tokens < 128 use 1 byte, < 16384 use 2 bytes, etc.
        // Prefix with token count as varint
        let mut buf = Vec::with_capacity(tokens.len() * 2);
        write_varint(&mut buf, tokens.len() as u64);
        for &token in tokens {
            write_varint(&mut buf, token as u64);
        }
        buf
    }

    fn decode(&self, bytes: &[u8]) -> Result<Vec<u32>, ProtocolError> {
        let mut cursor = 0;
        let count = read_varint(bytes, &mut cursor)?;
        let mut tokens = Vec::with_capacity(count as usize);
        for _ in 0..count {
            tokens.push(read_varint(bytes, &mut cursor)? as u32);
        }
        Ok(tokens)
    }
}

fn write_varint(buf: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

fn read_varint(bytes: &[u8], cursor: &mut usize) -> Result<u64, ProtocolError> {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        if *cursor >= bytes.len() {
            return Err(ProtocolError::InvalidFormat("varint truncated".into()));
        }
        let byte = bytes[*cursor];
        *cursor += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 { break; }
        shift += 7;
        if shift > 63 {
            return Err(ProtocolError::InvalidFormat("varint overflow".into()));
        }
    }
    Ok(result)
}
```

**benches/encoding_comparison.rs**: A/B comparison

```rust
// Criterion benchmark comparing:
// - V1Encoder.encode() vs V2Encoder.encode() bytes/sec
// - V1Encoder.decode() vs V2Encoder.decode() bytes/sec
// - Output size ratio (V2 bytes / V1 bytes) for small/medium/large
// Success gate: V2 size <= 70% of V1, latency within 3%
```

### Unit Tests

- `tests/encoding_property_test.rs` - Property: V2.decode(V2.encode(tokens)) == tokens for random token sequences
- `tests/encoding_edge_cases_test.rs` - Empty tokens, single token, max u32 token, boundary values (127, 128, 16383, 16384)

---

## Section 4 Compliance Pre-Check

| File | Estimated Lines | Status |
|------|-----------------|--------|
| `benches/ipc_throughput.rs` | ~80 | OK |
| `benches/scheduler_throughput.rs` | ~60 | OK |
| `benches/encoding_comparison.rs` | ~70 | OK |
| `src/ipc/encoding.rs` | ~100 | OK |
| `docs/INVARIANTS.md` | ~50 | OK |

All files well under 250 line limit.

---

## Risk Grade: L2

**Rationale**:
- Adds logic (encoding strategies, version negotiation)
- Does not modify security boundaries
- IPC authentication unchanged
- No new external dependencies (varint is inline)

**Gate Required**: `/ql-audit` MANDATORY before Phase 2/3 implementation

---

_Plan follows Simple Made Easy principles: separated concerns, incremental phases, value-oriented encoding abstraction_

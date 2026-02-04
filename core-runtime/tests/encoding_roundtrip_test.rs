//! Property-style tests for token encoding roundtrip correctness.

use core_runtime::ipc::{get_encoder, ProtocolVersion, TokenEncoder, V1Encoder};

#[test]
fn v1_roundtrip_empty() {
    let encoder = V1Encoder;
    let tokens: Vec<u32> = vec![];
    let encoded = encoder.encode(&tokens);
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(tokens, decoded);
}

#[test]
fn v1_roundtrip_single_token() {
    let encoder = V1Encoder;
    let tokens = vec![42];
    let encoded = encoder.encode(&tokens);
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(tokens, decoded);
}

#[test]
fn v1_roundtrip_small_sequence() {
    let encoder = V1Encoder;
    let tokens: Vec<u32> = (1..=100).collect();
    let encoded = encoder.encode(&tokens);
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(tokens, decoded);
}

#[test]
fn v1_roundtrip_large_sequence() {
    let encoder = V1Encoder;
    let tokens: Vec<u32> = (0..4000).collect();
    let encoded = encoder.encode(&tokens);
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(tokens, decoded);
}

#[test]
fn v1_roundtrip_boundary_values() {
    let encoder = V1Encoder;
    // Test boundary values: 0, 127, 128, 16383, 16384, max u32
    let tokens = vec![0, 127, 128, 16383, 16384, u32::MAX];
    let encoded = encoder.encode(&tokens);
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(tokens, decoded);
}

#[test]
fn v1_roundtrip_repeated_values() {
    let encoder = V1Encoder;
    let tokens = vec![42; 1000]; // 1000 repetitions of 42
    let encoded = encoder.encode(&tokens);
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(tokens, decoded);
}

#[test]
fn get_encoder_v1_returns_functional_encoder() {
    let encoder = get_encoder(ProtocolVersion::V1);
    let tokens = vec![1, 2, 3, 4, 5];
    let encoded = encoder.encode(&tokens);
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(tokens, decoded);
}

#[test]
fn get_encoder_v2_falls_back_to_v1() {
    // V2 not yet implemented, should fall back to V1 behavior
    let encoder = get_encoder(ProtocolVersion::V2);
    let tokens = vec![1, 2, 3, 4, 5];
    let encoded = encoder.encode(&tokens);
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(tokens, decoded);
}

#[test]
fn v1_decode_invalid_json_returns_error() {
    let encoder = V1Encoder;
    let result = encoder.decode(b"not valid json");
    assert!(result.is_err());
}

#[test]
fn v1_decode_wrong_type_returns_error() {
    let encoder = V1Encoder;
    // Valid JSON but not an array of u32
    let result = encoder.decode(b"\"hello\"");
    assert!(result.is_err());
}

#[test]
fn v1_encoding_is_deterministic() {
    let encoder = V1Encoder;
    let tokens = vec![100, 200, 300];
    let encoded1 = encoder.encode(&tokens);
    let encoded2 = encoder.encode(&tokens);
    assert_eq!(encoded1, encoded2);
}

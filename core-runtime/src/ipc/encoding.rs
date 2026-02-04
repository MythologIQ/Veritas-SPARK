//! Token encoding strategies for IPC protocol versioning.

use super::protocol::ProtocolError;

/// Trait for encoding/decoding token sequences.
pub trait TokenEncoder {
    /// Encode tokens to bytes.
    fn encode(&self, tokens: &[u32]) -> Vec<u8>;

    /// Decode bytes back to tokens.
    fn decode(&self, bytes: &[u8]) -> Result<Vec<u32>, ProtocolError>;
}

/// V1 Encoder: JSON serialization of token arrays.
/// This is the default encoding for backward compatibility.
#[derive(Debug, Clone, Copy, Default)]
pub struct V1Encoder;

impl TokenEncoder for V1Encoder {
    fn encode(&self, tokens: &[u32]) -> Vec<u8> {
        serde_json::to_vec(tokens).unwrap_or_default()
    }

    fn decode(&self, bytes: &[u8]) -> Result<Vec<u32>, ProtocolError> {
        serde_json::from_slice(bytes)
            .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))
    }
}

/// Get encoder for a given protocol version.
pub fn get_encoder(version: super::protocol::ProtocolVersion) -> Box<dyn TokenEncoder + Send + Sync> {
    match version {
        super::protocol::ProtocolVersion::V1 => Box::new(V1Encoder),
        // V2 encoder will be added in Phase 3
        super::protocol::ProtocolVersion::V2 => Box::new(V1Encoder), // Fallback until implemented
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_encode_empty() {
        let encoder = V1Encoder;
        let encoded = encoder.encode(&[]);
        assert_eq!(encoded, b"[]");
    }

    #[test]
    fn v1_encode_single() {
        let encoder = V1Encoder;
        let encoded = encoder.encode(&[42]);
        assert_eq!(encoded, b"[42]");
    }

    #[test]
    fn v1_roundtrip() {
        let encoder = V1Encoder;
        let tokens = vec![1, 2, 3, 100, 1000, 65535];
        let encoded = encoder.encode(&tokens);
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(tokens, decoded);
    }

    #[test]
    fn v1_decode_invalid() {
        let encoder = V1Encoder;
        let result = encoder.decode(b"not json");
        assert!(result.is_err());
    }
}

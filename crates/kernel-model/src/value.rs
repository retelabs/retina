use crate::error::ModelError;

/// Mirrors OTLP's `AnyValue` oneof (opentelemetry-proto v1.11.0,
/// `common/v1/common.proto`), minus the Profiling-signal-only Alpha variants
/// (`string_value_strindex`, `key_strindex`). Per docs/interfaces/otlp-ingestion.md,
/// a traces receiver must treat those as a non-fatal anomaly, not model them —
/// so there is no variant for them here.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    String(String),
    Bool(bool),
    /// `int64` on the wire (see [`ModelError::NegativeTokenCount`] for why this
    /// matters for token counts specifically).
    Int(i64),
    Double(f64),
    Bytes(Vec<u8>),
    Array(Vec<AttributeValue>),
    KeyValueList(Vec<(String, AttributeValue)>),
}

/// A single OTLP-shaped attribute: `(key, AnyValue)` (`common/v1/common.proto`,
/// `KeyValue`). Kernel events keep unrecognized/provider-specific attributes as
/// a flat `Vec` of these rather than a `Map`, because OTLP itself only
/// guarantees uniqueness by convention, not by the wire format (duplicate keys
/// are explicitly "unpredictable behavior" per the spec) — deduplication is a
/// kernel-side policy decision, not something to bake into the storage type.
pub type Attribute = (String, AttributeValue);

/// Non-negative token count. Only constructible via `TryFrom<i64>`, since
/// `gen_ai.usage.*` attributes are signed `int64` on the wire — this type
/// exists specifically so nothing in the kernel ever does a bare `as u64` cast
/// that would silently wrap a malformed negative value into a huge count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TokenCount(u64);

impl TokenCount {
    pub fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<i64> for TokenCount {
    type Error = ModelError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        u64::try_from(value)
            .map(TokenCount)
            .map_err(|_| ModelError::NegativeTokenCount(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_count_accepts_non_negative_values() {
        assert_eq!(TokenCount::try_from(0).unwrap().get(), 0);
        assert_eq!(TokenCount::try_from(100).unwrap().get(), 100);
    }

    #[test]
    fn token_count_rejects_negative_values_instead_of_wrapping() {
        let err = TokenCount::try_from(-1).unwrap_err();
        assert_eq!(err, ModelError::NegativeTokenCount(-1));
    }
}

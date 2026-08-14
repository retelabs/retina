use crate::error::ModelError;

/// OTLP `Span.trace_id` (trace.proto): exactly 16 bytes, non-all-zero.
/// docs/interfaces/otlp-ingestion.md leaves the *policy* for handling invalid
/// ids to the receiver (reject span vs. whole batch) — this type only encodes
/// the wire-level validity check itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceId([u8; 16]);

impl TryFrom<&[u8]> for TraceId {
    type Error = ModelError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; 16] = bytes.try_into().map_err(|_| ModelError::InvalidTraceId {
            got_len: bytes.len(),
        })?;
        if arr == [0u8; 16] {
            return Err(ModelError::ZeroTraceId);
        }
        Ok(TraceId(arr))
    }
}

/// OTLP `Span.span_id` / `parent_span_id` (trace.proto): exactly 8 bytes,
/// non-all-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanId([u8; 8]);

impl TryFrom<&[u8]> for SpanId {
    type Error = ModelError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; 8] = bytes.try_into().map_err(|_| ModelError::InvalidSpanId {
            got_len: bytes.len(),
        })?;
        if arr == [0u8; 8] {
            return Err(ModelError::ZeroSpanId);
        }
        Ok(SpanId(arr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_id_rejects_wrong_length() {
        let err = TraceId::try_from(&[1u8; 15][..]).unwrap_err();
        assert_eq!(err, ModelError::InvalidTraceId { got_len: 15 });
    }

    #[test]
    fn trace_id_rejects_all_zero() {
        let err = TraceId::try_from(&[0u8; 16][..]).unwrap_err();
        assert_eq!(err, ModelError::ZeroTraceId);
    }

    #[test]
    fn trace_id_accepts_valid_bytes() {
        assert!(TraceId::try_from(&[1u8; 16][..]).is_ok());
    }

    #[test]
    fn span_id_rejects_wrong_length() {
        let err = SpanId::try_from(&[1u8; 7][..]).unwrap_err();
        assert_eq!(err, ModelError::InvalidSpanId { got_len: 7 });
    }
}

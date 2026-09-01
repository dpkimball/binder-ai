//! Base64url (no padding) encoding for canonical `knowledge_uri` path segments (spec 015 S2).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecordRefError {
    #[error("empty record_ref")]
    Empty,
    #[error("invalid base64url record_ref")]
    InvalidEncoding,
    #[error("record_ref is not valid UTF-8")]
    InvalidUtf8,
}

/// Encode a canonical `knowledge_uri` for use in `/knowledge/{domain}/records/{record_ref}/…`.
pub fn encode_record_ref(knowledge_uri: &str) -> String {
    URL_SAFE_NO_PAD.encode(knowledge_uri.as_bytes())
}

/// Decode `{record_ref}` back to canonical `knowledge_uri`.
pub fn decode_record_ref(record_ref: &str) -> Result<String, RecordRefError> {
    let trimmed = record_ref.trim();
    if trimmed.is_empty() {
        return Err(RecordRefError::Empty);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(trimmed.as_bytes())
        .map_err(|_| RecordRefError::InvalidEncoding)?;
    String::from_utf8(bytes).map_err(|_| RecordRefError::InvalidUtf8)
}

/// Engineering domain parent URI for a question bank row.
pub fn engineering_question_uri(repo: &str, id: &str) -> String {
    let repo_seg = if repo.is_empty() { "cross" } else { repo };
    format!("code://question/{repo_seg}/{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_engineering_uri() {
        let uri = engineering_question_uri("codeintel", "C1");
        let enc = encode_record_ref(&uri);
        assert!(!enc.contains('='));
        assert!(!enc.contains('/'));
        assert_eq!(decode_record_ref(&enc).unwrap(), uri);
    }

    #[test]
    fn rejects_empty_ref() {
        assert_eq!(decode_record_ref("").unwrap_err(), RecordRefError::Empty);
    }
}

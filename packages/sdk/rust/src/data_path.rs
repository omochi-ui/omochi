//! Validate patch paths before SDK snapshot code can allocate sparse arrays.
use serde_json::Value;
use std::io;

/// Validate data-model path depth, decoded keys and combined numeric indices.
///
/// Call before materializing a patch from an untrusted stream.
// Keep these bounds aligned with `crates/protocol/src/data_path.rs` and the browser
// data-path validator so untrusted downstream frames have the same allocation limit.
pub fn validate(message: &Value) -> io::Result<()> {
    let Some(path) = message.pointer("/updateDataModel/path") else {
        return Ok(());
    };
    let invalid = |reason| io::Error::new(io::ErrorKind::InvalidData, reason);
    let path = path
        .as_str()
        .ok_or_else(|| invalid("data model path must be a string"))?;
    if path.split('/').count() > 128 {
        return Err(invalid("data model path too deep"));
    }
    let mut indices = 0u64;
    for segment in path.split('/') {
        let segment = segment.replace("~1", "/").replace("~0", "~");
        if matches!(segment.as_str(), "__proto__" | "constructor" | "prototype") {
            return Err(invalid("unsafe data model path"));
        }
        if !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_digit()) {
            indices = indices.saturating_add(segment.parse::<u64>().unwrap_or(u64::MAX));
            if indices > 100_000 {
                return Err(invalid("data model array indices exceed allocation budget"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn rejects_combined_sparse_expansion_before_applying_a_patch() {
        for path in [
            "/100001",
            "/60000/60000",
            "/9999999999999999999999999",
            "/constructor/x",
            "/x/__proto__",
            "/prototype",
        ] {
            assert!(
                validate(&json!({"updateDataModel":{"path":path}})).is_err(),
                "{path}"
            );
        }
        assert!(validate(&json!({"updateDataModel":{"path":"/x".repeat(128)}})).is_err());
        assert!(validate(&json!({"updateDataModel":{"path":42}})).is_err());
        for path in [
            "",
            "/",
            "/100000",
            "/50000/50000",
            "/names/0",
            "/a~1b/~0key",
        ] {
            validate(&json!({"updateDataModel":{"path":path}})).unwrap();
        }
        validate(&json!({"updateDataModel":{"value":{}}})).unwrap();
    }
}

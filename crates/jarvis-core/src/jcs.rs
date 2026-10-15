//! RFC 8785 canonical arguments, backed by serde_jcs (ECMAScript numbers).
use serde_json::Value;
use sha2::{Digest, Sha256};
pub fn canonical_digest(value: &Value) -> Result<String, serde_json::Error> {
    let bytes = serde_jcs::to_vec(value)?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}
#[cfg(test)]
mod tests {
    #[test]
    fn shared_vectors() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/contract-vectors/canonicalization.json"
        ))
        .unwrap();
        for v in fixture["vectors"].as_array().unwrap() {
            assert_eq!(
                serde_jcs::to_string(&v["input"]).unwrap(),
                v["canonical"].as_str().unwrap(),
                "{}",
                v["name"]
            );
        }
    }
}

//! LunarAST Cryptographic Token (LCT) – Ed25519-signed, resource-bound, bearer credential.
//! Format: base64url(payload).base64url(signature)

use chrono::Utc;
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

/// Contents of an LCT.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LctPayload {
    pub exp: u64,       // Unix timestamp
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub scope: String,  // e.g. "readonly"
}

/// Sign and encode a payload into a token string.
pub fn generate_lct(payload: &LctPayload, signing_key: &SigningKey) -> Result<String, String> {
    let json = serde_json::to_string(payload)
        .map_err(|e| format!("Serialize error: {}", e))?;
    let payload_b64 = URL_SAFE_NO_PAD.encode(json.as_bytes());
    let sig = signing_key.sign(json.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig.to_bytes());
    Ok(format!("{}.{}", payload_b64, sig_b64))
}

/// Verify a token string. If `expected_*` are provided, resource binding is enforced.
pub fn verify_lct(
    token: &str,
    verifying_key: &VerifyingKey,
    expected_owner: Option<&str>,
    expected_repo: Option<&str>,
    expected_branch: Option<&str>,
) -> Result<LctPayload, String> {
    let parts: Vec<&str> = token.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err("Invalid token format".into());
    }
    let payload_bytes = URL_SAFE_NO_PAD.decode(parts[0].as_bytes())
        .map_err(|e| format!("Base64 decode: {}", e))?;
    let sig_bytes = URL_SAFE_NO_PAD.decode(parts[1].as_bytes())
        .map_err(|e| format!("Base64 decode: {}", e))?;
    let signature = Signature::from_slice(&sig_bytes)
        .map_err(|e| format!("Invalid signature: {}", e))?;
    verifying_key.verify(&payload_bytes, &signature)
        .map_err(|e| format!("Signature: {}", e))?;

    let payload: LctPayload = serde_json::from_slice(&payload_bytes)
        .map_err(|e| format!("Payload parse: {}", e))?;

    let now = Utc::now().timestamp() as u64;
    if payload.exp < now {
        return Err("Token expired".into());
    }
    if let Some(o) = expected_owner {
        if payload.owner != o { return Err("Owner mismatch".into()); }
    }
    if let Some(r) = expected_repo {
        if payload.repo != r { return Err("Repo mismatch".into()); }
    }
    if let Some(b) = expected_branch {
        if payload.branch != b { return Err("Branch mismatch".into()); }
    }
    Ok(payload)
}

/// Load a 32-byte seed (base64 or hex) from disk and build a SigningKey.
pub fn load_signing_key(path: &str) -> Result<SigningKey, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Read key file: {}", e))?;
    let content = content.trim();
    let seed = URL_SAFE_NO_PAD.decode(content.as_bytes())
        .or_else(|_| crate::utils::hex_decode(content))
        .map_err(|e| format!("Decode seed: {}", e))?;
    if seed.len() != 32 {
        return Err("Seed must be 32 bytes".into());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&seed);
    Ok(SigningKey::from_bytes(&arr))
}

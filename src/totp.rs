//! Time-based One-Time Password verification via totp-lite (HMAC-SHA1, 6 digits).
//! Expects a Base32-encoded secret in `.lunar/totp.secret`.
//! Decodes the secret before computing TOTP to match authenticator app behavior.

use chrono::Utc;
use std::fs;

/// Validate a 6-digit code against the Base32 secret stored in `.lunar/totp.secret`.
/// Returns `Ok(true)` on match, `Ok(false)` on mismatch, or `Err` on I/O error.
pub fn verify_totp(code: &str) -> Result<bool, String> {
    let raw = fs::read_to_string(".lunar/totp.secret")
        .map_err(|e| format!("Read totp.secret: {}", e))?;
    // Trim whitespace and normalize to uppercase for robust Base32 decoding
    let raw_upper = raw.trim().to_uppercase();
    // Decode Base32 to raw bytes
    let secret_bytes = data_encoding::BASE32_NOPAD
        .decode(raw_upper.as_bytes())
        .map_err(|e| format!("Invalid Base32 secret: {}", e))?;

    let now = Utc::now().timestamp() as u64;
    // Check current window and adjacent windows (each 30 seconds)
    // Use totp_custom to force 6-digit output (default is 8)
    for offset in [-1i64, 0, 1].iter() {
        let t = (now as i64 + offset * 30) as u64;
        let expected = totp_lite::totp_custom::<totp_lite::Sha1>(30, 6, &secret_bytes, t);
        if expected == code {
            return Ok(true);
        }
    }
    Ok(false)
}

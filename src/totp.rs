//! Time-based One-Time Password verification via totp-lite (HMAC-SHA1, 6 digits).

use chrono::Utc;

/// Validate a 6-digit code against the secret stored in `.lunar/totp.secret`.
/// Returns `Ok(true)` on match, `Ok(false)` on mismatch, or `Err` on I/O error.
pub fn verify_totp(code: &str) -> Result<bool, String> {
    let secret = std::fs::read_to_string(".lunar/totp.secret")
        .map_err(|e| format!("Read totp.secret: {}", e))?;
    let secret = secret.trim();
    let expected = totp_lite::totp::<totp_lite::Sha1>(secret.as_bytes(), Utc::now().timestamp() as u64);
    Ok(expected == code)
}

//! Time-based One-Time Password verification via totp-lite (HMAC-SHA1, 6 digits).

use chrono::Utc;
use std::fs;

/// Validate a 6-digit code against the secret stored in `.lunar/totp.secret`.
/// Returns `Ok(true)` on match, `Ok(false)` on mismatch, or `Err` on I/O error.
pub fn verify_totp(code: &str) -> Result<bool, String> {
    let secret = fs::read_to_string(".lunar/totp.secret")
        .map_err(|e| format!("Read totp.secret: {}", e))?;
    let secret = secret.trim();
    let expected = totp_lite::totp::<totp_lite::Sha1>(secret.as_bytes(), Utc::now().timestamp() as u64);
    Ok(expected == code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_totp_verification_matches() {
        let dir = TempDir::new().unwrap();
        let lunar_dir = dir.path().join(".lunar");
        fs::create_dir_all(&lunar_dir).unwrap();
        let secret = "JBSWY3DPEHPK3PXP";
        fs::write(lunar_dir.join("totp.secret"), secret).unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let expected = totp_lite::totp::<totp_lite::Sha1>(secret.as_bytes(), Utc::now().timestamp() as u64);
        let result = verify_totp(&expected);
        std::env::set_current_dir(original_dir).unwrap();
        assert!(result.is_ok());
    }

    #[test]
    fn test_totp_invalid_code_returns_false() {
        let dir = TempDir::new().unwrap();
        let lunar_dir = dir.path().join(".lunar");
        fs::create_dir_all(&lunar_dir).unwrap();
        fs::write(lunar_dir.join("totp.secret"), "JBSWY3DPEHPK3PXP").unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let result = verify_totp("000000");
        std::env::set_current_dir(original_dir).unwrap();
        assert_eq!(result, Ok(false));
    }
}

use chrono::Utc;
use std::fs;

pub fn verify_totp(code: &str) -> Result<bool, String> {
    let raw = fs::read_to_string(".lunar/totp.secret")
        .map_err(|e| format!("Read totp.secret: {}", e))?;
    let raw_upper = raw.trim().to_uppercase();
    let secret_bytes = data_encoding::BASE32_NOPAD
        .decode(raw_upper.as_bytes())
        .map_err(|e| format!("Invalid Base32 secret: {}", e))?;

    let now = Utc::now().timestamp() as u64;
    for offset in [-1i64, 0, 1].iter() {
        let t = (now as i64 + offset * 30) as u64;
        let expected = totp_lite::totp::<totp_lite::Sha1>(&secret_bytes, t);
        // 临时打印，用于白盒诊断
        eprintln!("TOTP offset {}s: expected={} received={}", offset * 30, expected, code);
        if expected == code {
            return Ok(true);
        }
    }
    Ok(false)
}

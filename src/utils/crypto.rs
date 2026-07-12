/// Decode a hex string into bytes (used by lct module).
pub fn hex_decode(hex_str: &str) -> Result<Vec<u8>, String> {
    let hex_str = hex_str.trim();
    if hex_str.len() % 2 != 0 {
        return Err("Odd hex string length".into());
    }
    (0..hex_str.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_str[i..i+2], 16).map_err(|e| e.to_string()))
        .collect()
}

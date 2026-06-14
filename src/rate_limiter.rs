//! Simple in-memory IP-based rate limiter.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{OnceLock, RwLock};
use std::time::Instant;

#[derive(Debug, Clone, Default)]
struct State {
    failures: u32,
    first_failure: Option<Instant>,
    blocked_until: Option<Instant>,
}

static STORE: OnceLock<RwLock<HashMap<IpAddr, State>>> = OnceLock::new();

fn store() -> &'static RwLock<HashMap<IpAddr, State>> {
    STORE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Check if `ip` is currently allowed. Returns `Ok(())` or `Err(reason)`.
pub fn check(ip: &str, _max_failures: u32, _cooldown_secs: u64, _block_secs: u64) -> Result<(), String> {
    let addr: IpAddr = ip.parse().map_err(|_| "Invalid IP".to_string())?;
    let mut store = store().write().unwrap();
    let state = store.entry(addr).or_default();
    let now = Instant::now();
    if let Some(until) = state.blocked_until {
        if now < until {
            return Err(format!("Try again in {} seconds", (until - now).as_secs()));
        }
        // Block expired
        state.failures = 0;
        state.first_failure = None;
        state.blocked_until = None;
    }
    Ok(())
}

/// Record a failure for `ip`. Enforces block after `max_failures` within `cooldown_secs` window.
pub fn record_failure(ip: &str, max_failures: u32, cooldown_secs: u64, block_secs: u64) {
    if let Ok(addr) = ip.parse::<IpAddr>() {
        let mut store = store().write().unwrap();
        let state = store.entry(addr).or_default();
        let now = Instant::now();
        if let Some(first) = state.first_failure {
            if now - first > std::time::Duration::from_secs(cooldown_secs) {
                state.failures = 0;
                state.first_failure = None;
            }
        }
        if state.failures == 0 {
            state.first_failure = Some(now);
        }
        state.failures += 1;
        if state.failures >= max_failures {
            state.blocked_until = Some(now + std::time::Duration::from_secs(block_secs));
        }
    }
}

/// Clear all rate limit state (used for testing).
pub fn clear_all_limits() {
    store().write().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_allows_initial() {
        clear_all_limits();
        assert!(check("127.0.0.1", 3, 5, 10).is_ok());
    }

    #[test]
    fn test_rate_limit_blocks_after_max_failures() {
        clear_all_limits();
        let ip = "192.168.1.1";
        record_failure(ip, 3, 5, 10);
        record_failure(ip, 3, 5, 10);
        record_failure(ip, 3, 5, 10); // 3 failures
        assert!(check(ip, 3, 5, 10).is_err()); // should be blocked
    }

    #[test]
    fn test_rate_limit_resets_after_cooldown() {
        clear_all_limits();
        let ip = "10.0.0.1";
        // Use a very short cooldown for testing
        record_failure(ip, 3, 1, 10); // 1 sec cooldown
        record_failure(ip, 3, 1, 10);
        // Not yet blocked (2 < 3)
        assert!(check(ip, 3, 1, 10).is_ok());
        std::thread::sleep(std::time::Duration::from_secs(2)); // wait for cooldown
        record_failure(ip, 3, 1, 10); // should start new window, count 1
        assert!(check(ip, 3, 1, 10).is_ok());
    }
}

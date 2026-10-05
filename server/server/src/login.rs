//! Account rules that need memory or the clock: waits
//! after wrong passwords, the proof-of-work shield, new accounts per IP, the
//! argon2 pool, and session and device tokens.
//! The database work itself is in `persistence`.

use base::AccountId;
use persistence::{LoginBlock, TokenHash};
use protocol::{DeviceToken, SessionToken};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

/// The longest wait after wrong passwords.
const MAX_WAIT_SECS: u64 = 60 * 60;
/// Each hour with no new failure takes one off a key's count, so a typo a
/// day never adds up; an attacker fails far faster than this.
const DECAY_SECS: u64 = 60 * 60;
pub const ACCOUNTS_PER_IP_PER_HOUR: usize = 3;
pub use persistence::CHARACTERS_PER_ACCOUNT;

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Keys of `Attempts`. The account and IP parts are already
/// normalized (lower case name, `persistence::ip_key`).
pub fn ip_attempt_key(ip: &str) -> String {
    format!("ip:{ip}")
}

pub fn account_attempt_key(account: &str) -> String {
    format!("account:{account}")
}

pub fn pair_attempt_key(account: &str, ip: &str) -> String {
    format!("pair:{account}@{ip}")
}

pub fn change_attempt_key(account: AccountId) -> String {
    format!("change:{}", account.get())
}

/// Wrong tries before waits start, by kind of key. An IP may be shared
/// (NAT), and a whole account is the target of distributed guessing, so
/// both get more room than one account from one IP.
fn free_failures(key: &str) -> u32 {
    if key.starts_with("ip:") || key.starts_with("account:") {
        10
    } else {
        4
    }
}

#[derive(Clone, Copy)]
struct Failures {
    count: u32,
    last: u64,
    blocked_until: u64,
}

/// Growing wait after wrong passwords, per key: past the free
/// tries, 2^n seconds up to an hour; each hour without a failure takes one
/// off the count. A try counts the moment it is
/// accepted, before the password is checked, so parallel connections cannot
/// slip past the wait; a right password gives the try back (`refund`) or
/// clears the key (`succeeded`). Unix seconds, so the state can be saved.
#[derive(Default)]
pub struct Attempts {
    keys: BTreeMap<String, Failures>,
    changed: bool,
}

impl Attempts {
    /// Seconds still to wait, if the key is blocked.
    pub fn blocked(&self, key: &str, now: u64) -> Option<u64> {
        let f = self.keys.get(key)?;
        (f.blocked_until > now).then(|| (f.blocked_until - now).max(1))
    }

    pub fn failed(&mut self, key: &str, now: u64) {
        let f = self.keys.entry(key.to_owned()).or_insert(Failures {
            count: 0,
            last: now,
            blocked_until: 0,
        });
        let quiet = now.saturating_sub(f.last) / DECAY_SECS;
        f.count = f.count.saturating_sub(u32::try_from(quiet).unwrap_or(u32::MAX));
        f.count = f.count.saturating_add(1);
        f.last = now;
        let free = free_failures(key);
        if f.count > free {
            let n = (f.count - free).min(16);
            f.blocked_until = now + (1u64 << n).min(MAX_WAIT_SECS);
        }
        self.changed = true;
    }

    /// Takes back one try counted by `failed` (the password was right).
    pub fn refund(&mut self, key: &str) {
        let Some(f) = self.keys.get_mut(key) else {
            return;
        };
        f.count = f.count.saturating_sub(1);
        if f.count == 0 {
            self.keys.remove(key);
        } else if f.count <= free_failures(key) {
            f.blocked_until = 0;
        }
        self.changed = true;
    }

    pub fn succeeded(&mut self, key: &str) {
        self.changed |= self.keys.remove(key).is_some();
    }

    pub fn forget_old(&mut self, now: u64) {
        let before = self.keys.len();
        // Gone once the decay took every failure and no wait is left.
        self.keys.retain(|_, f| {
            now.saturating_sub(f.last) < u64::from(f.count) * DECAY_SECS || f.blocked_until > now
        });
        self.changed |= self.keys.len() != before;
    }

    /// The state to save, if it changed since the last call.
    pub fn take_changed(&mut self) -> Option<Vec<LoginBlock>> {
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        Some(
            self.keys
                .iter()
                .map(|(key, f)| LoginBlock {
                    key: key.clone(),
                    count: f.count,
                    last_at: f.last,
                    blocked_until: f.blocked_until,
                })
                .collect(),
        )
    }

    /// The state saved before a restart.
    pub fn load(blocks: Vec<LoginBlock>) -> Attempts {
        Attempts {
            keys: blocks
                .into_iter()
                .map(|b| {
                    let f = Failures {
                        count: b.count,
                        last: b.last_at,
                        blocked_until: b.blocked_until,
                    };
                    (b.key, f)
                })
                .collect(),
            changed: false,
        }
    }
}

/// Wrong passwords across the whole server in the last minute.
/// Past `per_min`, every `Login` and `Register` needs a proof of work
/// (`SHIELD_BITS`) for `SHIELD_SECS`.
pub struct Shield {
    per_min: usize,
    recent: VecDeque<u64>,
    raised_until: u64,
}

pub const SHIELD_BITS: u8 = 18;
const SHIELD_SECS: u64 = 10 * 60;

impl Shield {
    /// `per_min` 0: always raised.
    pub fn new(per_min: usize) -> Shield {
        Shield {
            per_min,
            recent: VecDeque::new(),
            raised_until: 0,
        }
    }

    /// `true` when this failure raised the shield.
    pub fn failed(&mut self, now: u64) -> bool {
        while self.recent.front().is_some_and(|t| now.saturating_sub(*t) >= 60) {
            self.recent.pop_front();
        }
        self.recent.push_back(now);
        if self.recent.len() < self.per_min {
            return false;
        }
        let was_up = self.raised_until > now;
        self.raised_until = now + SHIELD_SECS;
        !was_up
    }

    /// Leading zero bits the next proof needs.
    pub fn bits(&self, now: u64) -> u8 {
        if self.per_min == 0 || self.raised_until > now {
            SHIELD_BITS
        } else {
            0
        }
    }
}

/// At most `ACCOUNTS_PER_IP_PER_HOUR` new accounts per IP.
#[derive(Default)]
pub struct Registrations {
    by_ip: BTreeMap<IpAddr, VecDeque<Instant>>,
}

impl Registrations {
    /// Seconds until this IP may create an account, if it may not now.
    pub fn blocked(&mut self, ip: IpAddr, now: Instant) -> Option<u64> {
        let hour = Duration::from_secs(60 * 60);
        let times = self.by_ip.entry(ip).or_default();
        while times.front().is_some_and(|t| now.duration_since(*t) >= hour) {
            times.pop_front();
        }
        if times.len() < ACCOUNTS_PER_IP_PER_HOUR {
            return None;
        }
        times
            .front()
            .map(|t| (hour - now.duration_since(*t)).as_secs().max(1))
    }

    pub fn created(&mut self, ip: IpAddr, now: Instant) {
        self.by_ip.entry(ip).or_default().push_back(now);
    }

    pub fn forget_old(&mut self, now: Instant) {
        let hour = Duration::from_secs(60 * 60);
        self.by_ip
            .retain(|_, t| t.back().is_some_and(|last| now.duration_since(*last) < hour));
    }
}

/// argon2 off the network threads, with a limit of hashes at once so a
/// login storm cannot take every core.
#[derive(Clone)]
pub struct PasswordPool {
    permits: Arc<Semaphore>,
    /// Checked when the account does not exist, so both cases take as long.
    dummy: Arc<str>,
}

impl PasswordPool {
    pub async fn new(threads: usize) -> Result<PasswordPool, String> {
        let dummy = tokio::task::spawn_blocking(|| persistence::hash_password("not a real password"))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        Ok(PasswordPool {
            permits: Arc::new(Semaphore::new(threads.max(1))),
            dummy: dummy.into(),
        })
    }

    pub async fn hash(&self, password: Zeroizing<String>) -> Option<String> {
        let _permit = self.permits.acquire().await.ok()?;
        tokio::task::spawn_blocking(move || persistence::hash_password(&password).ok())
            .await
            .ok()
            .flatten()
    }

    /// `false` when `hash` is `None` (unknown account or `dev`), after the
    /// same work as a real check.
    pub async fn verify(&self, password: Zeroizing<String>, hash: Option<String>) -> bool {
        let Ok(_permit) = self.permits.acquire().await else {
            return false;
        };
        let real = hash.is_some();
        let hash = hash.unwrap_or_else(|| self.dummy.to_string());
        let ok = tokio::task::spawn_blocking(move || persistence::verify_password(&password, &hash))
            .await
            .unwrap_or(false);
        ok && real
    }
}

pub fn new_token() -> Option<SessionToken> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).ok()?;
    Some(SessionToken(bytes))
}

pub fn token_hash(token: &SessionToken) -> TokenHash {
    TokenHash(Sha256::digest(token.0).into())
}

pub fn new_device() -> Option<DeviceToken> {
    new_token().map(|t| DeviceToken(t.0))
}

pub fn device_hash(token: &DeviceToken) -> TokenHash {
    TokenHash(Sha256::digest(token.0).into())
}

/// A fresh seed for `Challenge`.
pub fn new_seed() -> [u8; 16] {
    let mut seed = [0u8; 16];
    // A zero seed still works; it only makes proofs reusable, so a failed
    // `getrandom` (never seen) is logged by nobody and costs little.
    let _ = getrandom::fill(&mut seed);
    seed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_grow_after_the_free_tries() {
        let mut a = Attempts::default();
        let t = 1000;
        let pair = pair_attempt_key("kai", "10.0.0.1");
        for _ in 0..4 {
            a.failed(&pair, t);
        }
        assert_eq!(a.blocked(&pair, t), None);
        a.failed(&pair, t);
        assert_eq!(a.blocked(&pair, t), Some(2));
        a.failed(&pair, t);
        assert_eq!(a.blocked(&pair, t), Some(4));
        for _ in 0..20 {
            a.failed(&pair, t);
        }
        assert_eq!(a.blocked(&pair, t), Some(3600));
        assert_eq!(a.blocked(&pair, t + 3601), None);
        a.succeeded(&pair);
        assert_eq!(a.blocked(&pair, t), None);
        // IP and whole account: ten free tries.
        let acc = account_attempt_key("kai");
        for _ in 0..10 {
            a.failed(&acc, t);
        }
        assert_eq!(a.blocked(&acc, t), None);
        a.failed(&acc, t);
        assert_eq!(a.blocked(&acc, t), Some(2));
    }

    #[test]
    fn a_right_password_gives_the_try_back() {
        let mut a = Attempts::default();
        let ip = ip_attempt_key("10.0.0.1");
        for _ in 0..11 {
            a.failed(&ip, 5);
        }
        assert!(a.blocked(&ip, 5).is_some());
        a.refund(&ip);
        assert_eq!(a.blocked(&ip, 5), None);
        for _ in 0..10 {
            a.refund(&ip);
        }
        assert!(a.take_changed().unwrap().is_empty());
        assert!(a.take_changed().is_none(), "nothing new");
    }

    #[test]
    fn waits_survive_a_restart_and_decay_away() {
        let mut a = Attempts::default();
        let pair = pair_attempt_key("kai", "10.0.0.1");
        for _ in 0..6 {
            a.failed(&pair, 100);
        }
        let mut b = Attempts::load(a.take_changed().unwrap());
        assert_eq!(b.blocked(&pair, 100), Some(4));
        b.forget_old(100 + 5 * DECAY_SECS);
        assert!(b.take_changed().is_none(), "one failure left");
        b.forget_old(100 + 6 * DECAY_SECS);
        assert!(b.take_changed().unwrap().is_empty());
    }

    #[test]
    fn a_typo_a_day_never_adds_up() {
        // Several accounts from one IP, one wrong password a day for a
        // month: the IP never waits.
        let mut a = Attempts::default();
        let ip = ip_attempt_key("10.0.0.1");
        for day in 0..30 {
            let t = day * 24 * DECAY_SECS;
            a.failed(&ip, t);
            assert_eq!(a.blocked(&ip, t), None, "day {day}");
        }
        // A guesser waiting out each hour-long wait stays at an hour.
        let mut b = Attempts::default();
        let mut t = 0;
        for _ in 0..40 {
            t = b.blocked(&ip, t).map_or(t, |w| t + w);
            b.failed(&ip, t);
        }
        assert_eq!(b.blocked(&ip, t), Some(3600));
    }

    #[test]
    fn the_shield_rises_with_many_failures_and_falls_later() {
        let mut s = Shield::new(3);
        assert_eq!(s.bits(0), 0);
        assert!(!s.failed(0));
        assert!(!s.failed(10));
        // Older than a minute: not counted.
        assert!(!s.failed(65));
        assert!(s.failed(66));
        assert_eq!(s.bits(66), SHIELD_BITS);
        assert!(!s.failed(67), "already up");
        assert_eq!(s.bits(67 + SHIELD_SECS), 0);
        assert_eq!(Shield::new(0).bits(0), SHIELD_BITS);
    }

    #[test]
    fn three_accounts_per_ip_per_hour() {
        let mut r = Registrations::default();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let t = Instant::now();
        for _ in 0..3 {
            assert_eq!(r.blocked(ip, t), None);
            r.created(ip, t);
        }
        assert_eq!(r.blocked(ip, t), Some(3600));
        assert!(r.blocked("10.0.0.2".parse().unwrap(), t).is_none());
        assert_eq!(r.blocked(ip, t + Duration::from_secs(3600)), None);
    }

    #[test]
    fn tokens_are_random_and_hashed() {
        let a = new_token().unwrap();
        let b = new_token().unwrap();
        assert_ne!(a, b);
        assert_eq!(token_hash(&a), token_hash(&a));
        assert_ne!(token_hash(&a), token_hash(&b));
        assert_ne!(token_hash(&a).0, a.0);
    }
}

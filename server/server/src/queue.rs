//! Entry queue: first
//! come, first in. The estimate is the position times the mean interval
//! between openings of the last minutes.

use server_net::ConnectionId;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const ESTIMATE_WINDOW: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
pub struct EntryQueue {
    waiting: VecDeque<ConnectionId>,
    openings: VecDeque<Instant>,
}

impl EntryQueue {
    pub fn push(&mut self, c: ConnectionId) {
        self.waiting.push_back(c);
    }

    pub fn remove(&mut self, c: ConnectionId) {
        self.waiting.retain(|w| *w != c);
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }

    /// Next to enter; counts as an opening for the estimate.
    pub fn pop(&mut self, now: Instant) -> Option<ConnectionId> {
        let c = self.waiting.pop_front()?;
        self.openings.push_back(now);
        Some(c)
    }

    /// `(connection, position from 1, estimate in seconds or 0)`.
    pub fn positions(&mut self, now: Instant) -> Vec<(ConnectionId, u32, u32)> {
        while self
            .openings
            .front()
            .is_some_and(|t| now.duration_since(*t) > ESTIMATE_WINDOW)
        {
            self.openings.pop_front();
        }
        let per_opening = match (self.openings.front(), self.openings.len()) {
            (Some(first), n) if n >= 2 => now.duration_since(*first).as_secs_f64() / n as f64,
            _ => 0.0,
        };
        self.waiting
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let pos = i as u32 + 1;
                (*c, pos, (per_opening * f64::from(pos)).ceil() as u32)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_and_estimate() {
        let mut q = EntryQueue::default();
        let t = Instant::now();
        for i in 1..=3 {
            q.push(ConnectionId(i));
        }
        assert_eq!(
            q.positions(t),
            vec![
                (ConnectionId(1), 1, 0),
                (ConnectionId(2), 2, 0),
                (ConnectionId(3), 3, 0)
            ]
        );
        assert_eq!(q.pop(t), Some(ConnectionId(1)));
        q.push(ConnectionId(4));
        assert_eq!(q.pop(t + Duration::from_secs(10)), Some(ConnectionId(2)));
        // Two openings in 20 s seen at t+20: 10 s each.
        let p = q.positions(t + Duration::from_secs(20));
        assert_eq!(p, vec![(ConnectionId(3), 1, 10), (ConnectionId(4), 2, 20)]);
        q.remove(ConnectionId(3));
        assert_eq!(q.pop(t), Some(ConnectionId(4)));
        assert!(q.is_empty());
    }
}

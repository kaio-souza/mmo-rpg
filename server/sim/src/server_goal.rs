//! Server goals: what the zone's players add to each shared goal
//! (`goals/<id>.ron`). The zone only counts; the gateway sums every zone,
//! decides when a goal is reached and says so (`Command::GoalsDone`).

use crate::types::TickOutput;
use crate::zone::Zone;

/// One count of `(family, key)` by a player: added to every goal it feeds
/// that is not reached yet, in `out.goals` (summed within the tick).
pub(crate) fn contribute(z: &Zone, family: &str, key: &str, n: u64, out: &mut TickOutput) {
    for g in z.content.catalog.goals() {
        if !g.counts(family, key) || z.goals_done.contains(&g.id) {
            continue;
        }
        match out.goals.iter_mut().find(|(id, _)| *id == g.id) {
            Some((_, total)) => *total = total.saturating_add(n),
            None => out.goals.push((g.id.clone(), n)),
        }
    }
}

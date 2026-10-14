//! A simple battle AI. Deliberately readable rather than clever: it's a
//! starting point to replace or mod.
//!
//! - Infantry advance on the nearest enemy that's still fighting.
//! - Cavalry hang back until the lines are engaged, then hunt for an enemy
//!   unit that's busy fighting (a flank or rear charge), and avoid charging
//!   spears head-on.

use crate::battle::{Battle, Order, UnitId, UnitState};
use crate::data::Team;

pub fn think(b: &mut Battle, team: Team) {
    let ids: Vec<UnitId> = b
        .units
        .iter()
        .filter(|u| u.team == team && !u.state.is_broken())
        .map(|u| u.id)
        .collect();
    let lines_engaged = b
        .units
        .iter()
        .any(|u| u.team == team && !u.stats.mounted && u.state == UnitState::Fighting);

    for uid in ids {
        let current = match b.units[uid].order {
            Order::Attack { target } => Some(target),
            _ => None,
        };
        let target_fighting_fit = current.is_some_and(|t| !b.units[t].state.is_broken());
        let mounted = b.units[uid].stats.mounted;

        if mounted {
            if target_fighting_fit {
                continue;
            }
            if let Some(t) = pick_cavalry_target(b, uid, lines_engaged) {
                b.order(uid, Order::Attack { target: t });
            } else if current.is_some() {
                // Chased a broken unit far enough; come back to the fight.
                b.order(uid, Order::Hold);
            }
            continue;
        }

        // Infantry: keep fighting a healthy target; switch off routers if
        // someone dangerous is close, otherwise chase them a while.
        if target_fighting_fit {
            continue;
        }
        if let Some(t) = b.nearest_enemy_unit(uid, f32::INFINITY, false) {
            b.order(uid, Order::Attack { target: t });
        }
    }
}

fn pick_cavalry_target(b: &Battle, uid: UnitId, lines_engaged: bool) -> Option<UnitId> {
    let me = &b.units[uid];
    let mut best: Option<(f32, UnitId)> = None;
    for e in &b.units {
        if e.team == me.team || e.alive_count == 0 || e.state.is_broken() {
            continue;
        }
        let d = e.centre.distance(me.centre);
        let mut score = d;
        if e.state == UnitState::Fighting {
            score *= 0.4; // engaged enemies can't turn to face us
        } else if e.stats.bonus_vs_mounted > 0.0 {
            score *= 4.0; // don't charge a spear wall head-on
        }
        if e.stats.mounted {
            score *= 0.7; // see off their horse first
        }
        let worth_it = lines_engaged || e.stats.mounted || d < 40.0;
        if worth_it && best.is_none_or(|(s, _)| score < s) {
            best = Some((score, e.id));
        }
    }
    best.map(|(_, id)| id)
}

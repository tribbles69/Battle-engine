//! The battle simulation: soldiers, units, formations, melee, charges, morale.
//!
//! Two levels, like Total War:
//! - A **unit** holds an order and a formation *anchor* (centre + facing).
//!   Each soldier has a slot relative to that anchor.
//! - A **soldier** steers towards its slot, or towards the nearest enemy when
//!   its unit is fighting, and resolves its own attacks.
//!
//! The tick runs in phases (plan -> attack -> integrate -> collide -> morale) so
//! every soldier reads a consistent snapshot and results are deterministic.

use crate::data::{Battlefield, Scenario, Team, UnitType, UnitTypes};
use crate::grid::SpatialGrid;
use crate::math::{angle_diff, turn_towards, Vec2};
use crate::rng::Rng;

pub type UnitId = usize;
pub type SoldierId = usize;

/// Fixed simulation step. The renderer interpolates between ticks if it wants.
pub const TICK: f32 = 1.0 / 30.0;

/// How long a charge bonus lasts after impact, seconds.
const CHARGE_WINDOW: f32 = 2.5;
/// Seconds a flank or rear attack keeps affecting morale.
const FLANK_MEMORY: f32 = 3.0;
/// A soldier must be moving at this fraction of run speed to count as charging.
const CHARGE_SPEED_FRACTION: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Order {
    /// Stand in formation. Soldiers still fight anyone who comes close.
    Hold,
    /// March to `to`. `facing` (radians) and `files` reshape the formation on arrival.
    Move {
        to: Vec2,
        facing: Option<f32>,
        files: Option<u32>,
    },
    /// Close with and fight another unit.
    Attack { target: UnitId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitState {
    Ready,
    Moving,
    Charging,
    Fighting,
    Routing,
    Shattered,
    Destroyed,
}

impl UnitState {
    /// True once the unit has stopped being a fighting force (possibly for now).
    pub fn is_broken(self) -> bool {
        matches!(self, UnitState::Routing | UnitState::Shattered | UnitState::Destroyed)
    }
}

/// Human-readable morale band, for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoraleLevel {
    Eager,
    Steady,
    Wavering,
    Routing,
    Shattered,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BattleEvent {
    ChargeImpact { unit: UnitId, target: UnitId, pos: Vec2 },
    UnitRouted(UnitId),
    UnitRallied(UnitId),
    UnitShattered(UnitId),
    UnitDestroyed(UnitId),
    Victory(Team),
}

#[derive(Debug, Clone)]
pub struct Soldier {
    pub unit: UnitId,
    pub pos: Vec2,
    pub vel: Vec2,
    /// Radians, 0 = east.
    pub facing: f32,
    pub hp: f32,
    pub alive: bool,
    /// Ran off the battlefield while routing. Gone from the battle.
    pub fled: bool,
    /// Offset in formation space: x = right, y = forward of the anchor.
    pub slot: Vec2,
    pub attack_timer: f32,
    /// Remaining charge bonus time.
    pub charge_timer: f32,
    /// Knocked down or reeling: can't attack, defends badly.
    pub stun_timer: f32,
    /// Seconds since this soldier last swung (for animation).
    pub since_attack: f32,
    /// Seconds since this soldier was last hit (for animation).
    pub since_hit: f32,
    /// Battle time of death, for corpse effects.
    pub death_time: f32,
    in_contact: bool,
}

impl Soldier {
    pub fn active(&self) -> bool {
        self.alive && !self.fled
    }
}

#[derive(Debug, Clone)]
pub struct Unit {
    pub id: UnitId,
    pub team: Team,
    /// Key of the unit type in the data file.
    pub kind: String,
    pub name: String,
    pub stats: UnitType,
    pub soldiers: Vec<SoldierId>,
    pub initial_men: u32,
    /// Formation centre and facing the soldiers are trying to hold.
    pub anchor: Vec2,
    pub anchor_facing: f32,
    pub files: u32,
    pub order: Order,
    /// Run instead of walk when moving (attacks always run the last stretch).
    pub running: bool,
    pub state: UnitState,
    /// Current morale. Routs at zero.
    pub morale: f32,
    /// Short-term fear from casualties and charges. Decays over time.
    pub shock: f32,
    /// 0-100. Running and fighting tire troops out.
    pub stamina: f32,
    /// Average position of active soldiers.
    pub centre: Vec2,
    pub alive_count: u32,
    pub kills: u32,
    pub rout_count: u32,
    /// Direction this unit flees in when it routs.
    pub retreat_dir: Vec2,
    flank_timer: f32,
    rear_timer: f32,
    charged_timer: f32,
    calm_timer: f32,
    /// Soldiers currently in weapon reach of an enemy.
    pub contacts: u32,
    /// Is the unit in "free melee" (soldiers pick their own enemies)?
    pub melee_mode: bool,
    /// Average distance of soldiers from their slots (formation cohesion).
    pub slot_error: f32,
    wait_timer: f32,
}

impl Unit {
    pub fn casualty_ratio(&self) -> f32 {
        1.0 - self.alive_count as f32 / self.initial_men.max(1) as f32
    }

    pub fn morale_level(&self) -> MoraleLevel {
        match self.state {
            UnitState::Shattered | UnitState::Destroyed => MoraleLevel::Shattered,
            UnitState::Routing => MoraleLevel::Routing,
            _ if self.morale < 15.0 => MoraleLevel::Wavering,
            _ if self.morale > 60.0 => MoraleLevel::Eager,
            _ => MoraleLevel::Steady,
        }
    }

    pub fn forward(&self) -> Vec2 {
        Vec2::from_angle(self.anchor_facing)
    }

    /// World position of a formation-space offset.
    pub fn slot_world(&self, slot: Vec2) -> Vec2 {
        let f = self.forward();
        self.anchor + f.perp_right() * slot.x + f * slot.y
    }

    /// Formation depth in metres (front rank to rear rank).
    pub fn depth(&self) -> f32 {
        let ranks = (self.alive_count.max(1) as f32 / self.files.max(1) as f32).ceil();
        ranks * self.stats.spacing
    }

    pub fn width(&self) -> f32 {
        self.files.min(self.alive_count.max(1)) as f32 * self.stats.spacing
    }

    fn fatigue_factor(&self) -> f32 {
        if self.stamina < 30.0 {
            0.85
        } else {
            1.0
        }
    }
}

/// Lay out `n` slots, `files` wide, centred on the anchor. Index 0 is the front
/// rank's leftmost man; ranks run front to back.
pub fn formation_slots(n: usize, files: u32, spacing: f32) -> Vec<Vec2> {
    let files = (files.max(1) as usize).min(n.max(1));
    let ranks = n.div_ceil(files);
    let mut out = Vec::with_capacity(n);
    for r in 0..ranks {
        let in_rank = files.min(n - r * files);
        let y = ((ranks as f32 - 1.0) / 2.0 - r as f32) * spacing;
        for f in 0..in_rank {
            let x = (f as f32 - (in_rank as f32 - 1.0) / 2.0) * spacing;
            out.push(Vec2::new(x, y));
        }
    }
    out
}

/// What one soldier intends to do this tick (computed from a frozen snapshot).
#[derive(Clone, Copy)]
struct Plan {
    desired_vel: Vec2,
    facing: f32,
    /// Enemy within weapon reach, if any.
    strike: Option<SoldierId>,
}

pub struct Battle {
    pub units: Vec<Unit>,
    pub soldiers: Vec<Soldier>,
    pub time: f32,
    pub field: Battlefield,
    pub winner: Option<Team>,
    /// Teams driven by the built-in AI.
    pub ai_teams: Vec<Team>,
    events: Vec<BattleEvent>,
    rng: Rng,
    grid: SpatialGrid,
    plans: Vec<Plan>,
    ai_timer: f32,
}

impl Battle {
    pub fn new(scenario: &Scenario, types: &UnitTypes) -> Result<Self, String> {
        let field = scenario.battlefield.clone();
        let mut b = Battle {
            units: Vec::new(),
            soldiers: Vec::new(),
            time: 0.0,
            grid: SpatialGrid::new(field.width, field.height, 2.0),
            field,
            winner: None,
            ai_teams: vec![scenario.player_team.enemy()],
            events: Vec::new(),
            rng: Rng::new(scenario.seed),
            plans: Vec::new(),
            ai_timer: 0.0,
        };
        for su in &scenario.units {
            let stats = types
                .types
                .get(&su.kind)
                .ok_or_else(|| format!("unknown unit type '{}'", su.kind))?
                .clone();
            let facing = su.facing.to_radians();
            let pos = Vec2::new(su.pos[0], su.pos[1]);
            let files = su.files.unwrap_or(stats.files);
            let name = su.name.clone().unwrap_or_else(|| stats.name.clone());
            b.spawn_unit(su.kind.clone(), name, su.team, stats, pos, facing, files);
        }
        Ok(b)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spawn_unit(
        &mut self,
        kind: String,
        name: String,
        team: Team,
        stats: UnitType,
        pos: Vec2,
        facing: f32,
        files: u32,
    ) -> UnitId {
        let id = self.units.len();
        let n = stats.men as usize;
        let slots = formation_slots(n, files, stats.spacing);
        let fwd = Vec2::from_angle(facing);
        // Flee back the way the unit faces at deployment.
        let retreat_dir = -fwd;
        let mut soldier_ids = Vec::with_capacity(n);
        for slot in slots {
            let world = pos + fwd.perp_right() * slot.x + fwd * slot.y;
            let jitter = Vec2::new(self.rng.range(-0.15, 0.15), self.rng.range(-0.15, 0.15));
            soldier_ids.push(self.soldiers.len());
            self.soldiers.push(Soldier {
                unit: id,
                pos: world + jitter,
                vel: Vec2::ZERO,
                facing,
                hp: stats.hp,
                alive: true,
                fled: false,
                slot,
                attack_timer: self.rng.range(0.0, stats.attack_interval),
                charge_timer: 0.0,
                stun_timer: 0.0,
                since_attack: 99.0,
                since_hit: 99.0,
                death_time: 0.0,
                in_contact: false,
            });
        }
        self.units.push(Unit {
            id,
            team,
            kind,
            name,
            initial_men: stats.men,
            morale: stats.morale,
            stats,
            soldiers: soldier_ids,
            anchor: pos,
            anchor_facing: facing,
            files,
            order: Order::Hold,
            running: false,
            state: UnitState::Ready,
            shock: 0.0,
            stamina: 100.0,
            centre: pos,
            alive_count: n as u32,
            kills: 0,
            rout_count: 0,
            retreat_dir,
            flank_timer: 0.0,
            rear_timer: 0.0,
            charged_timer: 0.0,
            calm_timer: 0.0,
            contacts: 0,
            melee_mode: false,
            slot_error: 0.0,
            wait_timer: 0.0,
        });
        id
    }

    // ------------------------------------------------------------------
    // Public API
    // ------------------------------------------------------------------

    /// Give a unit an order. Ignored for broken units.
    pub fn order(&mut self, unit: UnitId, order: Order) {
        let Some(u) = self.units.get_mut(unit) else { return };
        if u.state.is_broken() {
            return;
        }
        if let Order::Attack { target } = order {
            if target >= self.units.len() || self.units[target].team == self.units[unit].team {
                return;
            }
        }
        let u = &mut self.units[unit];
        u.order = order;
        u.melee_mode = false;
        u.wait_timer = 0.0;
        if let Order::Move { to, facing, files } = order {
            if let Some(f) = files {
                u.files = f.max(1);
            }
            // Face the direction of travel while marching.
            let dir = to - u.anchor;
            if dir.length() > 1.0 {
                u.anchor_facing = dir.angle();
            } else if let Some(f) = facing {
                u.anchor_facing = f;
            }
            self.reassign_slots(unit);
        }
    }

    pub fn set_running(&mut self, unit: UnitId, running: bool) {
        if let Some(u) = self.units.get_mut(unit) {
            u.running = running;
        }
    }

    /// The unit with an active soldier nearest `pos`, within `radius` metres.
    pub fn unit_at(&self, pos: Vec2, radius: f32) -> Option<UnitId> {
        let mut best: Option<(f32, UnitId)> = None;
        for s in self.soldiers.iter().filter(|s| s.active()) {
            let d = s.pos.distance_sq(pos);
            if d <= radius * radius && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, s.unit));
            }
        }
        best.map(|(_, u)| u)
    }

    /// Take the events raised since the last call.
    pub fn drain_events(&mut self) -> Vec<BattleEvent> {
        std::mem::take(&mut self.events)
    }

    /// Advance the battle by one fixed tick.
    pub fn tick(&mut self) {
        let dt = TICK;
        self.time += dt;
        self.grid.rebuild(
            self.soldiers
                .iter()
                .enumerate()
                .filter(|(_, s)| s.active())
                .map(|(i, s)| (i, s.pos)),
        );
        self.update_unit_stats();
        self.ai_timer -= dt;
        if self.ai_timer <= 0.0 {
            self.ai_timer = 1.0;
            for team in self.ai_teams.clone() {
                crate::ai::think(self, team);
            }
        }
        self.update_anchors(dt);
        self.plan_soldiers(dt);
        self.resolve_attacks(dt);
        self.integrate(dt);
        self.collide();
        self.update_unit_stats();
        self.update_morale(dt);
        self.check_victory();
    }

    /// Run `seconds` of battle.
    pub fn run_for(&mut self, seconds: f32) {
        let ticks = (seconds / TICK).round() as u32;
        for _ in 0..ticks {
            self.tick();
        }
    }

    // ------------------------------------------------------------------
    // Formation helpers
    // ------------------------------------------------------------------

    /// Give each soldier a slot so that men nearest the new front become the
    /// front rank, and left-to-right order is kept. Stops men crossing through
    /// each other when the formation turns or reshapes.
    pub fn reassign_slots(&mut self, unit: UnitId) {
        let u = &self.units[unit];
        let mut ids: Vec<SoldierId> =
            u.soldiers.iter().copied().filter(|&i| self.soldiers[i].active()).collect();
        if ids.is_empty() {
            return;
        }
        let fwd = u.forward();
        let right = fwd.perp_right();
        let files = (u.files.max(1) as usize).min(ids.len());
        let slots = formation_slots(ids.len(), files as u32, u.stats.spacing);
        let soldiers = &self.soldiers;
        ids.sort_by(|&a, &b| {
            soldiers[b].pos.dot(fwd).total_cmp(&soldiers[a].pos.dot(fwd))
        });
        for chunk in ids.chunks_mut(files) {
            chunk.sort_by(|&a, &b| soldiers[a].pos.dot(right).total_cmp(&soldiers[b].pos.dot(right)));
        }
        for (k, &sid) in ids.iter().enumerate() {
            self.soldiers[sid].slot = slots[k];
        }
    }

    fn update_unit_stats(&mut self) {
        for u in &mut self.units {
            let mut sum = Vec2::ZERO;
            let mut n = 0u32;
            for &sid in &u.soldiers {
                let s = &self.soldiers[sid];
                if s.active() {
                    sum += s.pos;
                    n += 1;
                }
            }
            u.alive_count = n;
            if n > 0 {
                u.centre = sum * (1.0 / n as f32);
            } else if u.state != UnitState::Destroyed {
                u.state = UnitState::Destroyed;
                self.events.push(BattleEvent::UnitDestroyed(u.id));
            }
        }
    }

    fn update_anchors(&mut self, dt: f32) {
        for uid in 0..self.units.len() {
            let state = self.units[uid].state;
            if state.is_broken() {
                let c = self.units[uid].centre;
                self.units[uid].anchor = c;
                continue;
            }
            // Bump into the enemy while marching? Stop and fight them.
            if matches!(self.units[uid].order, Order::Move { .. }) && self.units[uid].contacts >= 4 {
                if let Some(enemy) = self.nearest_enemy_unit(uid, 15.0, false) {
                    self.units[uid].order = Order::Attack { target: enemy };
                }
            }
            let order = self.units[uid].order;
            match order {
                Order::Hold => {
                    let u = &mut self.units[uid];
                    u.melee_mode = false;
                    u.state = if u.contacts > 0 { UnitState::Fighting } else { UnitState::Ready };
                }
                Order::Move { to, facing, .. } => self.step_move(uid, to, facing, dt),
                Order::Attack { target } => self.step_attack(uid, target, dt),
            }
        }
    }

    fn march_speed(&self, uid: UnitId, run: bool) -> f32 {
        let u = &self.units[uid];
        let base = if run { u.stats.run_speed } else { u.stats.walk_speed };
        // The anchor goes a little slower than the men so they can keep up.
        base * 0.9 * u.fatigue_factor()
    }

    fn step_move(&mut self, uid: UnitId, to: Vec2, facing: Option<f32>, dt: f32) {
        let run = self.units[uid].running;
        let speed = self.march_speed(uid, run);
        let u = &mut self.units[uid];
        let dir = to - u.anchor;
        let dist = dir.length();
        if dist > 0.3 {
            u.state = UnitState::Moving;
            // Wait for stragglers, so the formation holds together.
            if u.slot_error > 3.0 && u.wait_timer < 3.0 {
                u.wait_timer += dt;
                return;
            }
            u.wait_timer = 0.0;
            let turn_rate = if u.stats.mounted { 2.0 } else { 1.2 };
            u.anchor_facing = turn_towards(u.anchor_facing, dir.angle(), turn_rate * dt);
            u.anchor += dir.normalize_or_zero() * (speed * dt).min(dist);
        } else {
            u.anchor = to;
            let final_facing = facing.unwrap_or(u.anchor_facing);
            let changed = angle_diff(u.anchor_facing, final_facing).abs() > 0.05;
            u.anchor_facing = final_facing;
            u.order = Order::Hold;
            u.state = UnitState::Ready;
            if changed {
                self.reassign_slots(uid);
            }
        }
    }

    fn step_attack(&mut self, uid: UnitId, target: UnitId, dt: f32) {
        let target_ok = self.units.get(target).is_some_and(|t| {
            !matches!(t.state, UnitState::Destroyed | UnitState::Shattered) && t.alive_count > 0
        });
        if !target_ok {
            let u = &mut self.units[uid];
            u.order = Order::Hold;
            u.anchor = u.centre;
            u.melee_mode = false;
            self.reassign_slots(uid);
            return;
        }
        let tc = self.units[target].centre;
        let reach_dist = self.units[uid].depth() * 0.5 + self.units[target].depth() * 0.5 + 4.0;
        let dist = tc.distance(self.units[uid].anchor);
        // Run once close, like a proper charge.
        let run = self.units[uid].running || dist < 60.0;
        let speed = self.march_speed(uid, run);
        let u = &mut self.units[uid];
        if u.melee_mode || dist < reach_dist {
            u.melee_mode = true;
            u.anchor = u.centre;
            u.state = if u.contacts > 0 { UnitState::Fighting } else { UnitState::Charging };
            // If the fight drifts apart (enemy fled far), form up and chase again.
            if u.contacts == 0 && u.centre.distance(tc) > reach_dist + 25.0 {
                u.melee_mode = false;
            }
            return;
        }
        u.state = if run && dist < 40.0 { UnitState::Charging } else { UnitState::Moving };
        let dir = tc - u.anchor;
        let turn_rate = if u.stats.mounted { 2.0 } else { 1.2 };
        u.anchor_facing = turn_towards(u.anchor_facing, dir.angle(), turn_rate * dt);
        // Hold the line together on the approach, but never stall a charge.
        if u.slot_error > 3.5 && dist > 30.0 && u.wait_timer < 3.0 {
            u.wait_timer += dt;
            return;
        }
        u.wait_timer = 0.0;
        u.anchor += dir.normalize_or_zero() * speed * dt;
    }

    /// Which way the unit's front points: at its target once in melee,
    /// otherwise its formation facing.
    pub fn front_dir(&self, uid: UnitId) -> Vec2 {
        let u = &self.units[uid];
        if let (true, Order::Attack { target }) = (u.melee_mode, u.order) {
            let d = (self.units[target].centre - u.centre).normalize_or_zero();
            if d != Vec2::ZERO {
                return d;
            }
        }
        u.forward()
    }

    /// Nearest enemy unit to this unit's centre that is not destroyed.
    pub fn nearest_enemy_unit(&self, uid: UnitId, max_dist: f32, include_routing: bool) -> Option<UnitId> {
        let u = &self.units[uid];
        self.units
            .iter()
            .filter(|e| e.team != u.team && e.alive_count > 0)
            .filter(|e| include_routing || !e.state.is_broken())
            .map(|e| (e.centre.distance(u.centre), e.id))
            .filter(|(d, _)| *d <= max_dist)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    }

    // ------------------------------------------------------------------
    // Soldier phases
    // ------------------------------------------------------------------

    fn nearest_enemy_soldier(&self, sid: SoldierId, radius: f32) -> Option<(SoldierId, f32)> {
        let me = &self.soldiers[sid];
        let my_team = self.units[me.unit].team;
        let mut best: Option<(SoldierId, f32)> = None;
        self.grid.for_each_near(me.pos, radius, |j| {
            let o = &self.soldiers[j];
            if self.units[o.unit].team == my_team {
                return;
            }
            let d = o.pos.distance(me.pos);
            if d <= radius && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((j, d));
            }
        });
        best
    }

    #[allow(clippy::needless_range_loop)]
    fn plan_soldiers(&mut self, dt: f32) {
        let mut plans = std::mem::take(&mut self.plans);
        plans.clear();
        plans.resize(
            self.soldiers.len(),
            Plan { desired_vel: Vec2::ZERO, facing: 0.0, strike: None },
        );
        let mut slot_err_sum = vec![0.0f32; self.units.len()];
        for sid in 0..self.soldiers.len() {
            let s = &self.soldiers[sid];
            if !s.active() {
                continue;
            }
            let u = &self.units[s.unit];
            let st = &u.stats;
            let fatigue = u.fatigue_factor();
            let run_speed = st.run_speed * fatigue;
            let walk_speed = st.walk_speed * fatigue;

            if u.state.is_broken() {
                // Flee: back towards home, and away from any enemy close by.
                let mut dir = u.retreat_dir;
                if let Some((e, d)) = self.nearest_enemy_soldier(sid, 10.0) {
                    let away = (s.pos - self.soldiers[e].pos).normalize_or_zero();
                    dir += away * (2.0 * (1.0 - d / 10.0));
                }
                let dir = dir.normalize_or_zero();
                plans[sid] = Plan { desired_vel: dir * run_speed, facing: dir.angle(), strike: None };
                continue;
            }

            // How far this soldier will go looking for a fight.
            let engage_radius: f32 = if u.melee_mode {
                10.0
            } else if matches!(u.state, UnitState::Ready | UnitState::Fighting) {
                3.5
            } else {
                0.0
            };
            let reach = st.reach + st.radius;
            let search = engage_radius.max(reach + 1.2);
            let enemy = self.nearest_enemy_soldier(sid, search);

            let slot_pos = u.slot_world(s.slot);
            let slot_dist = slot_pos.distance(s.pos);
            slot_err_sum[s.unit] += slot_dist;

            let mut strike = None;
            let (desired_vel, facing) = match enemy {
                Some((e, d)) if d <= engage_radius || d <= reach + self.soldiers[e].radius_hint(&self.units) => {
                    let other = &self.soldiers[e];
                    let edge = d - st.radius - self.units[other.unit].stats.radius;
                    let to_enemy = (other.pos - s.pos).normalize_or_zero();
                    if edge <= st.reach {
                        strike = Some(e);
                    }
                    let gap = edge - st.reach * 0.7;
                    let charging = s.vel.length() > run_speed * CHARGE_SPEED_FRACTION;
                    let speed = if gap <= 0.0 {
                        0.0
                    } else if charging {
                        // Charging men don't slow down: they hit at full tilt.
                        run_speed
                    } else if u.melee_mode {
                        run_speed.min(gap * 3.0 + walk_speed * 0.5)
                    } else {
                        walk_speed.min(gap * 3.0 + 0.3)
                    };
                    (to_enemy * speed, to_enemy.angle())
                }
                _ => {
                    // Head for the slot. Run to catch up if far behind.
                    let to_slot = slot_pos - s.pos;
                    let unit_moving = matches!(u.state, UnitState::Moving | UnitState::Charging);
                    let max = if (unit_moving && (u.running || u.state == UnitState::Charging))
                        || slot_dist > 4.0
                    {
                        run_speed
                    } else {
                        walk_speed * 1.15
                    };
                    let speed = (slot_dist * 1.5).min(max);
                    let facing = if slot_dist > 1.5 { to_slot.angle() } else { u.anchor_facing };
                    (to_slot.normalize_or_zero() * speed, facing)
                }
            };
            plans[sid] = Plan { desired_vel, facing, strike };
        }
        for (uid, u) in self.units.iter_mut().enumerate() {
            u.slot_error = if u.alive_count > 0 { slot_err_sum[uid] / u.alive_count as f32 } else { 0.0 };
        }
        let _ = dt;
        self.plans = plans;
    }

    fn resolve_attacks(&mut self, dt: f32) {
        let n_units = self.units.len();
        let mut contacts = vec![0u32; n_units];
        for sid in 0..self.soldiers.len() {
            if !self.soldiers[sid].active() {
                continue;
            }
            {
                let s = &mut self.soldiers[sid];
                s.attack_timer -= dt;
                s.charge_timer = (s.charge_timer - dt).max(0.0);
                s.stun_timer = (s.stun_timer - dt).max(0.0);
                s.since_attack += dt;
                s.since_hit += dt;
            }
            let Some(target) = self.plans[sid].strike else {
                self.soldiers[sid].in_contact = false;
                continue;
            };
            if !self.soldiers[target].active() {
                continue;
            }
            let uid = self.soldiers[sid].unit;
            contacts[uid] += 1;

            // First touch at speed: a charge impact.
            let was_in_contact = self.soldiers[sid].in_contact;
            self.soldiers[sid].in_contact = true;
            if !was_in_contact && !self.units[uid].state.is_broken() {
                self.try_charge_impact(sid, target);
            }

            if !self.soldiers[sid].active() {
                continue; // died bouncing off braced spears
            }
            if self.soldiers[sid].attack_timer > 0.0 || self.soldiers[sid].stun_timer > 0.0 {
                continue;
            }
            self.strike(sid, target);
        }
        for (uid, u) in self.units.iter_mut().enumerate() {
            u.contacts = contacts[uid];
        }
    }

    fn try_charge_impact(&mut self, sid: SoldierId, target: SoldierId) {
        let a = &self.soldiers[sid];
        let au = &self.units[a.unit];
        let speed = a.vel.length();
        if au.stats.charge_bonus <= 0.0 || speed < au.stats.run_speed * CHARGE_SPEED_FRACTION {
            return;
        }
        let d = &self.soldiers[target];
        let du = &self.units[d.unit];
        let attacker_unit = a.unit;
        let defender_unit = d.unit;
        let dir = (d.pos - a.pos).normalize_or_zero();

        // Braced spears: horses charging the front of a steady spear block get hurt.
        let braced = au.stats.mounted
            && du.stats.bonus_vs_mounted > 0.0
            && d.vel.length() < 1.0
            && !du.state.is_broken()
            && Vec2::from_angle(d.facing).dot(-dir) > 0.3;
        if braced {
            let dmg = du.stats.damage * 1.5 * (1.0 + du.stats.bonus_vs_mounted / 100.0);
            let a = &mut self.soldiers[sid];
            a.vel = a.vel * -0.2;
            a.stun_timer = 1.0;
            a.charge_timer = 0.0;
            self.apply_damage(target, sid, dmg);
            return;
        }

        let ratio = au.stats.mass / (au.stats.mass + du.stats.mass);
        let impulse = dir * speed * ratio * 1.2;
        let knockdown = au.stats.mass >= du.stats.mass * 2.0;
        {
            let a = &mut self.soldiers[sid];
            a.charge_timer = CHARGE_WINDOW;
            a.vel = a.vel * (1.0 - ratio * 0.5);
        }
        {
            let d = &mut self.soldiers[target];
            d.vel += impulse;
            if knockdown {
                d.stun_timer = d.stun_timer.max(1.5);
            }
        }
        if knockdown {
            // Trampled: a free hit with the full charge behind it.
            let dmg = self.units[attacker_unit].stats.damage * 0.6;
            self.apply_damage(sid, target, dmg);
        }
        let du = &mut self.units[defender_unit];
        if du.charged_timer <= 0.0 {
            du.shock += 6.0;
            let pos = self.soldiers[target].pos;
            self.events.push(BattleEvent::ChargeImpact { unit: attacker_unit, target: defender_unit, pos });
        }
        du.charged_timer = 4.0;
    }

    fn strike(&mut self, sid: SoldierId, target: SoldierId) {
        let a = &self.soldiers[sid];
        let au = &self.units[a.unit];
        let d = &self.soldiers[target];
        let du = &self.units[d.unit];

        let charge = au.stats.charge_bonus * (a.charge_timer / CHARGE_WINDOW);
        let mut attack = (au.stats.attack + charge) * au.fatigue_factor();
        let mut defence = du.stats.defence * du.fatigue_factor();

        // Individual: a blow from behind or the side is harder to parry.
        let from = (a.pos - d.pos).normalize_or_zero();
        let cos = Vec2::from_angle(d.facing).dot(from);
        if cos < -0.4 {
            defence *= 0.4;
        } else if cos < 0.3 {
            defence *= 0.75;
        }
        // Unit level: is the attacker actually round the side or back of the
        // whole formation? That's what hurts morale.
        let defender_unit = d.unit;
        let front = self.front_dir(defender_unit);
        let rel = a.pos - du.centre;
        let ahead = rel.dot(front);
        let side = rel.dot(front.perp_right()).abs();
        let rear = ahead < -(du.depth() * 0.5 + 1.0);
        let flank = !rear && ahead < du.depth() * 0.5 && side > du.width() * 0.5 + 1.0;
        if d.stun_timer > 0.0 {
            defence *= 0.3;
        }
        if du.state.is_broken() {
            defence *= 0.25;
            attack += 10.0;
        }
        let hit_chance = (0.25 + (attack - defence) / 100.0).clamp(0.05, 0.9);
        let mut damage = au.stats.damage + charge * 0.4;
        if du.stats.mounted {
            damage *= 1.0 + au.stats.bonus_vs_mounted / 100.0;
        }
        damage *= 100.0 / (100.0 + du.stats.armour * 1.5);
        let interval = au.stats.attack_interval;

        let roll = self.rng.f32();
        let jitter = self.rng.range(0.7, 1.3);
        {
            let a = &mut self.soldiers[sid];
            a.attack_timer = interval * jitter;
            a.since_attack = 0.0;
        }
        {
            let du = &mut self.units[defender_unit];
            if rear {
                du.rear_timer = FLANK_MEMORY;
            } else if flank {
                du.flank_timer = FLANK_MEMORY;
            }
        }
        if roll < hit_chance {
            self.apply_damage(sid, target, damage);
        }
    }

    fn apply_damage(&mut self, attacker: SoldierId, target: SoldierId, dmg: f32) {
        let time = self.time;
        let d = &mut self.soldiers[target];
        if !d.active() {
            return;
        }
        d.hp -= dmg;
        d.since_hit = 0.0;
        if d.hp <= 0.0 {
            d.alive = false;
            d.vel = Vec2::ZERO;
            d.death_time = time;
            let du = d.unit;
            let men = self.units[du].initial_men.max(1) as f32;
            self.units[du].shock += 50.0 / men;
            let au = self.soldiers[attacker].unit;
            self.units[au].kills += 1;
        }
    }

    fn integrate(&mut self, dt: f32) {
        for sid in 0..self.soldiers.len() {
            let s = &self.soldiers[sid];
            if !s.active() {
                continue;
            }
            let st = &self.units[s.unit].stats;
            let accel = if st.mounted { 5.0 } else { 7.0 };
            let turn_rate = if st.mounted { 3.0 } else { 6.0 };
            let max_speed = st.run_speed * 1.3;
            let plan = self.plans[sid];
            let (hw, hh) = (self.field.width / 2.0 + 30.0, self.field.height / 2.0 + 30.0);
            let routing = self.units[s.unit].state.is_broken();
            let s = &mut self.soldiers[sid];
            if s.stun_timer > 0.0 {
                // Reeling: just slide to a stop.
                s.vel = s.vel * (1.0 - 3.0 * dt).max(0.0);
            } else {
                let dv = (plan.desired_vel - s.vel).clamp_length(accel * dt);
                s.vel += dv;
            }
            s.vel = s.vel.clamp_length(max_speed);
            s.pos += s.vel * dt;
            s.facing = turn_towards(s.facing, plan.facing, turn_rate * dt);
            // Routers who leave the field are gone.
            if routing && (s.pos.x.abs() > hw || s.pos.y.abs() > hh) {
                s.fled = true;
            }
        }
    }

    /// Push overlapping bodies apart, heavier bodies moving less.
    fn collide(&mut self) {
        self.grid.rebuild(
            self.soldiers
                .iter()
                .enumerate()
                .filter(|(_, s)| s.active())
                .map(|(i, s)| (i, s.pos)),
        );
        let mut push = vec![Vec2::ZERO; self.soldiers.len()];
        for i in 0..self.soldiers.len() {
            let a = &self.soldiers[i];
            if !a.active() {
                continue;
            }
            let ua = &self.units[a.unit];
            let (ra, ma) = (ua.stats.radius, ua.stats.mass);
            self.grid.for_each_near(a.pos, ra + 1.2, |j| {
                if j <= i {
                    return;
                }
                let b = &self.soldiers[j];
                let ub = &self.units[b.unit];
                let (rb, mb) = (ub.stats.radius, ub.stats.mass);
                let delta = b.pos - a.pos;
                let dist = delta.length();
                let min = ra + rb;
                if dist >= min {
                    return;
                }
                let n = if dist > 1e-4 { delta * (1.0 / dist) } else { Vec2::new(1.0, 0.0) };
                // Friends squeeze a little; enemies are solid.
                let stiffness = if ua.team == ub.team { 0.4 } else { 0.8 };
                let overlap = (min - dist) * stiffness;
                let total = ma + mb;
                push[i] -= n * (overlap * mb / total);
                push[j] += n * (overlap * ma / total);
            });
        }
        for (s, p) in self.soldiers.iter_mut().zip(push) {
            if s.active() {
                s.pos += p;
            }
        }
    }

    // ------------------------------------------------------------------
    // Morale
    // ------------------------------------------------------------------

    /// What a unit's morale would be right now, from all its modifiers.
    fn compute_morale(&self, uid: UnitId) -> f32 {
        let u = &self.units[uid];
        let mut m = u.stats.morale;
        let mut support = false;
        let mut routing_near = 0;
        for o in &self.units {
            if o.id == uid || o.team != u.team || o.alive_count == 0 {
                continue;
            }
            let d = o.centre.distance(u.centre);
            if o.state.is_broken() {
                if d < 60.0 {
                    routing_near += 1;
                }
            } else if d < 50.0 {
                support = true;
            }
        }
        if support {
            m += 8.0;
        }
        m -= (8.0 * routing_near as f32).min(24.0);
        m -= 70.0 * u.casualty_ratio();
        m -= u.shock;
        if u.rear_timer > 0.0 {
            m -= 25.0;
        } else if u.flank_timer > 0.0 {
            m -= 12.0;
        }
        if u.stamina < 30.0 {
            m -= 8.0;
        }
        m
    }

    fn update_morale(&mut self, dt: f32) {
        for uid in 0..self.units.len() {
            if self.units[uid].state == UnitState::Destroyed {
                continue;
            }
            // Stamina.
            {
                let u = &mut self.units[uid];
                let drain = match u.state {
                    UnitState::Fighting => -0.5,
                    UnitState::Charging | UnitState::Routing | UnitState::Shattered => -0.8,
                    UnitState::Moving if u.running => -0.8,
                    UnitState::Moving => -0.05,
                    _ => 1.0,
                };
                u.stamina = (u.stamina + drain * dt).clamp(0.0, 100.0);
                u.flank_timer = (u.flank_timer - dt).max(0.0);
                u.rear_timer = (u.rear_timer - dt).max(0.0);
                u.charged_timer = (u.charged_timer - dt).max(0.0);
                let decay = if u.state.is_broken() { 3.0 } else { 1.0 };
                u.shock = (u.shock - decay * dt).max(0.0);
            }
            let morale = self.compute_morale(uid);
            let threatened = self.nearest_enemy_unit(uid, 30.0, false).is_some();
            let u = &mut self.units[uid];
            u.morale = morale;
            match u.state {
                UnitState::Shattered => {}
                UnitState::Routing => {
                    u.calm_timer = if threatened { 0.0 } else { u.calm_timer + dt };
                    if u.calm_timer > 5.0 && morale > 20.0 {
                        u.state = UnitState::Ready;
                        u.order = Order::Hold;
                        u.anchor = u.centre;
                        u.anchor_facing = (-u.retreat_dir).angle();
                        u.melee_mode = false;
                        self.events.push(BattleEvent::UnitRallied(uid));
                        self.reassign_slots(uid);
                    }
                }
                _ if morale <= 0.0 => {
                    u.rout_count += 1;
                    u.melee_mode = false;
                    u.calm_timer = 0.0;
                    if u.casualty_ratio() >= 0.7 || u.rout_count >= 3 {
                        u.state = UnitState::Shattered;
                        self.events.push(BattleEvent::UnitShattered(uid));
                    } else {
                        u.state = UnitState::Routing;
                        self.events.push(BattleEvent::UnitRouted(uid));
                    }
                }
                _ => {}
            }
        }
    }

    fn check_victory(&mut self) {
        if self.winner.is_some() {
            return;
        }
        for team in [Team::Blue, Team::Red] {
            let beaten = self
                .units
                .iter()
                .filter(|u| u.team == team)
                .all(|u| u.state.is_broken());
            if beaten {
                let w = team.enemy();
                self.winner = Some(w);
                self.events.push(BattleEvent::Victory(w));
                return;
            }
        }
    }
}

impl Soldier {
    /// Body radius of this soldier (stored on the unit type).
    fn radius_hint(&self, units: &[Unit]) -> f32 {
        units[self.unit].stats.radius
    }
}

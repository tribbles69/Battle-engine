//! Fylking battle simulation.
//!
//! Engine-agnostic and deterministic: no graphics, no engine types. The game
//! crate renders it with Bevy, but the same core can run headless for tests,
//! balance runs, replays or a server.

pub mod ai;
pub mod battle;
pub mod data;
pub mod grid;
pub mod math;
pub mod rng;

pub use battle::{Battle, BattleEvent, MoraleLevel, Order, Soldier, Unit, UnitId, UnitState, TICK};
pub use data::{Scenario, Team, UnitType, UnitTypes};
pub use math::Vec2;

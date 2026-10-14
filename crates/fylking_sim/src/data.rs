//! Data definitions loaded from TOML: unit types and scenarios.
//!
//! Everything a modder should want to change lives in data files, not code.
//! See `assets/data/units.toml` and `assets/data/scenarios/`.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Stats for one kind of unit. Numbers are on a rough Total-War-like scale:
/// attack/defence/armour around 10-60, morale around 30-70.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitType {
    /// Display name, e.g. "Yari Ashigaru".
    pub name: String,
    /// Number of soldiers in a full-strength unit.
    pub men: u32,
    /// Hit points per soldier.
    pub hp: f32,
    /// Melee attack skill: improves chance to hit.
    pub attack: f32,
    /// Melee defence skill: reduces the enemy's chance to hit.
    pub defence: f32,
    /// Armour: reduces damage taken.
    pub armour: f32,
    /// Base damage per successful hit.
    pub damage: f32,
    /// Extra attack and damage for a short time after charging into contact.
    pub charge_bonus: f32,
    /// Extra damage (percent) against mounted soldiers.
    #[serde(default)]
    pub bonus_vs_mounted: f32,
    /// Walking speed, m/s.
    pub walk_speed: f32,
    /// Running speed, m/s.
    pub run_speed: f32,
    /// Base morale. A unit routs when its current morale drops to zero.
    pub morale: f32,
    /// Weight for charge impacts and shoving. Infantry is 1.
    #[serde(default = "default_mass")]
    pub mass: f32,
    /// Body radius in metres (collision).
    #[serde(default = "default_radius")]
    pub radius: f32,
    /// Weapon reach in metres, measured between body edges.
    #[serde(default = "default_reach")]
    pub reach: f32,
    /// Seconds between attacks (randomised +/- 30%).
    #[serde(default = "default_attack_interval")]
    pub attack_interval: f32,
    /// Horses: faster, heavier, vulnerable to spears.
    #[serde(default)]
    pub mounted: bool,
    /// Spacing between soldiers in formation, metres.
    #[serde(default = "default_spacing")]
    pub spacing: f32,
    /// Default number of files (soldiers per rank).
    #[serde(default = "default_files")]
    pub files: u32,
}

fn default_mass() -> f32 {
    1.0
}
fn default_radius() -> f32 {
    0.35
}
fn default_reach() -> f32 {
    1.0
}
fn default_attack_interval() -> f32 {
    1.6
}
fn default_spacing() -> f32 {
    1.1
}
fn default_files() -> u32 {
    20
}

/// All unit types, keyed by id (the TOML table name).
#[derive(Debug, Clone, Deserialize)]
pub struct UnitTypes {
    #[serde(flatten)]
    pub types: BTreeMap<String, UnitType>,
}

impl UnitTypes {
    pub fn from_toml(src: &str) -> Result<Self, String> {
        toml::from_str(src).map_err(|e| format!("units file: {e}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Team {
    Blue,
    Red,
}

impl Team {
    pub fn enemy(self) -> Team {
        match self {
            Team::Blue => Team::Red,
            Team::Red => Team::Blue,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Battlefield {
    /// Width (east-west) in metres. The field is centred on the origin.
    pub width: f32,
    /// Depth (north-south) in metres.
    pub height: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioUnit {
    /// Key into the unit types file.
    #[serde(rename = "type")]
    pub kind: String,
    pub team: Team,
    /// Centre of the formation, [x, y] in metres.
    pub pos: [f32; 2],
    /// Facing in degrees: 0 = east, 90 = north.
    pub facing: f32,
    /// Override the type's default number of files.
    pub files: Option<u32>,
    /// Override the display name.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Which team the player controls; the other is run by the AI.
    pub player_team: Team,
    #[serde(default)]
    pub seed: u64,
    pub battlefield: Battlefield,
    pub units: Vec<ScenarioUnit>,
}

impl Scenario {
    pub fn from_toml(src: &str) -> Result<Self, String> {
        toml::from_str(src).map_err(|e| format!("scenario file: {e}"))
    }
}

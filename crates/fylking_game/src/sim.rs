//! Owns the simulation: loading data, stepping at a fixed rate, game speed,
//! restarting and switching scenarios.

use bevy::prelude::*;
use fylking_sim::{Battle, BattleEvent, Scenario, Team, UnitTypes, Vec2 as SimVec2, TICK};
use std::path::{Path, PathBuf};

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        let data = DataDir(find_data_dir());
        let list = ScenarioList::scan(&data.0);
        let sim = Sim::load(&data.0, &list.paths[list.current]).unwrap_or_else(|e| {
            panic!("Couldn't start: {e}\nData folder: {}", data.0.display())
        });
        app.insert_resource(data)
            .insert_resource(list)
            .insert_resource(sim)
            .init_resource::<EventFeed>()
            .add_message::<BattleRestarted>()
            .add_systems(Update, (scenario_keys, step_sim).chain());
    }
}

/// Fired when a new battle starts (restart or new scenario) so views rebuild.
#[derive(Message)]
pub struct BattleRestarted;

#[derive(Resource)]
pub struct DataDir(pub PathBuf);

#[derive(Resource)]
pub struct ScenarioList {
    pub paths: Vec<PathBuf>,
    pub current: usize,
}

impl ScenarioList {
    fn scan(data: &Path) -> Self {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(data.join("scenarios"))
            .map(|rd| {
                rd.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.extension().is_some_and(|x| x == "toml"))
                    .collect()
            })
            .unwrap_or_default();
        paths.sort();
        assert!(!paths.is_empty(), "no scenarios found in {}", data.join("scenarios").display());
        // Start on the bigger battle if it's there.
        let current = paths
            .iter()
            .position(|p| p.file_stem().is_some_and(|s| s == "first_clash"))
            .unwrap_or(0);
        ScenarioList { paths, current }
    }
}

#[derive(Resource)]
pub struct Sim {
    pub battle: Battle,
    pub scenario: Scenario,
    pub player: Team,
    /// Positions at the previous tick, for smooth interpolation.
    pub prev_pos: Vec<SimVec2>,
    pub prev_facing: Vec<f32>,
    /// Fraction of a tick elapsed since the last one, 0..1.
    pub alpha: f32,
    accumulator: f32,
    pub paused: bool,
    pub speed: f32,
}

impl Sim {
    pub fn load(data: &Path, scenario_path: &Path) -> Result<Self, String> {
        let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
        let types = UnitTypes::from_toml(&read(&data.join("units.toml"))?)?;
        let scenario = Scenario::from_toml(&read(scenario_path)?)?;
        let battle = Battle::new(&scenario, &types)?;
        Ok(Sim {
            prev_pos: battle.soldiers.iter().map(|s| s.pos).collect(),
            prev_facing: battle.soldiers.iter().map(|s| s.facing).collect(),
            player: scenario.player_team,
            battle,
            scenario,
            alpha: 0.0,
            accumulator: 0.0,
            paused: true, // start paused so you can look around and plan
            speed: 1.0,
        })
    }
}

/// Recent battle events as readable lines, newest last.
#[derive(Resource, Default)]
pub struct EventFeed {
    pub lines: Vec<(f32, String, Team)>,
}

const SPEEDS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];

fn step_sim(time: Res<Time>, keys: Res<ButtonInput<KeyCode>>, mut sim: ResMut<Sim>, mut feed: ResMut<EventFeed>) {
    if keys.just_pressed(KeyCode::Space) {
        sim.paused = !sim.paused;
    }
    let idx = SPEEDS.iter().position(|&s| s == sim.speed).unwrap_or(2);
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        sim.speed = SPEEDS[(idx + 1).min(SPEEDS.len() - 1)];
    }
    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        sim.speed = SPEEDS[idx.saturating_sub(1)];
    }
    if sim.paused {
        return;
    }
    sim.accumulator += time.delta_secs().min(0.1) * sim.speed;
    let mut steps = 0;
    while sim.accumulator >= TICK && steps < 16 {
        let Sim { battle, prev_pos, prev_facing, .. } = &mut *sim;
        for (i, s) in battle.soldiers.iter().enumerate() {
            prev_pos[i] = s.pos;
            prev_facing[i] = s.facing;
        }
        battle.tick();
        for e in battle.drain_events() {
            if let Some(line) = describe(battle, &e) {
                feed.lines.push((battle.time, line.0, line.1));
            }
        }
        sim.accumulator -= TICK;
        steps += 1;
    }
    if feed.lines.len() > 6 {
        let n = feed.lines.len() - 6;
        feed.lines.drain(..n);
    }
    sim.alpha = (sim.accumulator / TICK).clamp(0.0, 1.0);
}

fn describe(b: &Battle, e: &BattleEvent) -> Option<(String, Team)> {
    let name = |u: usize| b.units[u].name.clone();
    let team = |u: usize| b.units[u].team;
    Some(match *e {
        BattleEvent::ChargeImpact { unit, target, .. } => {
            (format!("{} charge into {}", name(unit), name(target)), team(unit))
        }
        BattleEvent::UnitRouted(u) => (format!("{} are routing!", name(u)), team(u)),
        BattleEvent::UnitRallied(u) => (format!("{} have rallied", name(u)), team(u)),
        BattleEvent::UnitShattered(u) => (format!("{} have shattered", name(u)), team(u)),
        BattleEvent::UnitDestroyed(u) => (format!("{} destroyed", name(u)), team(u)),
        BattleEvent::Victory(_) => return None, // shown as a banner instead
    })
}

/// F5 restarts, Tab moves to the next scenario.
fn scenario_keys(
    keys: Res<ButtonInput<KeyCode>>,
    data: Res<DataDir>,
    mut list: ResMut<ScenarioList>,
    mut sim: ResMut<Sim>,
    mut feed: ResMut<EventFeed>,
    mut restarted: MessageWriter<BattleRestarted>,
) {
    let restart = keys.just_pressed(KeyCode::F5);
    let next = keys.just_pressed(KeyCode::Tab);
    if !restart && !next {
        return;
    }
    if next {
        list.current = (list.current + 1) % list.paths.len();
    }
    // Reloads the TOML each time, so you can tweak stats and press F5.
    match Sim::load(&data.0, &list.paths[list.current]) {
        Ok(s) => {
            *sim = s;
            feed.lines.clear();
            restarted.write(BattleRestarted);
        }
        Err(e) => {
            feed.lines.push((sim.battle.time, format!("Load error: {e}"), Team::Red));
        }
    }
}

/// Look for `assets/data` next to the executable, in the working directory,
/// or in the source tree. `FYLKING_DATA` overrides all of them.
fn find_data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("FYLKING_DATA") {
        return PathBuf::from(p);
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("assets/data"));
        }
    }
    candidates.push(PathBuf::from("assets/data"));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/data"));
    candidates
        .iter()
        .find(|p| p.join("units.toml").exists())
        .cloned()
        .unwrap_or_else(|| candidates.last().unwrap().clone())
}

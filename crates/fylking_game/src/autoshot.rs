//! Scripted smoke test: set `FYLKING_AUTOSHOT=<folder>` and the game plays a
//! short scripted battle, saves screenshots along the way, then quits.
//! Useful for checking rendering on CI or after big changes.

use crate::camera::RtsCamera;
use crate::input::Selection;
use crate::sim::Sim;
use crate::ui::HelpPanel;
use crate::view::to_world;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use fylking_sim::Order;
use std::path::PathBuf;

pub struct AutoshotPlugin;

impl Plugin for AutoshotPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(dir) = std::env::var("FYLKING_AUTOSHOT") {
            app.insert_resource(Script { dir: PathBuf::from(dir), frame: 0, step: 0 })
                .add_systems(Update, run_script);
        }
    }
}

#[derive(Resource)]
struct Script {
    dir: PathBuf,
    frame: u32,
    step: usize,
}

fn run_script(
    mut commands: Commands,
    mut script: ResMut<Script>,
    mut sim: ResMut<Sim>,
    mut sel: ResMut<Selection>,
    mut cam: Single<&mut RtsCamera>,
    mut exit: MessageWriter<AppExit>,
    mut help: Query<&mut Visibility, With<HelpPanel>>,
) {
    for mut v in &mut help {
        *v = Visibility::Hidden;
    }
    script.frame += 1;
    info!("autoshot frame {}", script.frame);
    // (frame, battle seconds to fast-forward to, screenshot name)
    const SHOTS: [(u32, f32, &str); 4] =
        [(40, 0.0, "1_deploy"), (46, 28.0, "2_advance"), (52, 50.0, "3_clash"), (58, 75.0, "4_aftermath")];
    if script.step >= SHOTS.len() {
        if script.frame > SHOTS[SHOTS.len() - 1].0 + 6 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let (frame, at, name) = SHOTS[script.step];
    if script.frame == 2 {
        // Select our army and send it forward, horse round the left flank.
        sel.0 = sim.battle.units.iter().filter(|u| u.team == sim.player).map(|u| u.id).collect();
        let ids = sel.0.clone();
        let enemy = sim.battle.units.iter().find(|u| u.team != sim.player).map(|u| u.id).unwrap();
        for id in ids {
            if sim.battle.units[id].stats.mounted {
                sim.battle.set_running(id, true);
                sim.battle.order(id, Order::Attack { target: enemy });
            } else {
                let to = sim.battle.units[id].centre + fylking_sim::Vec2::new(0.0, 25.0);
                sim.battle.order(id, Order::Move { to, facing: Some(std::f32::consts::FRAC_PI_2), files: None });
            }
        }
    }
    if script.frame == frame - 3 {
        sim.paused = true;
        while sim.battle.time < at {
            sim.battle.tick();
        }
        sim.prev_pos = sim.battle.soldiers.iter().map(|s| s.pos).collect();
        sim.prev_facing = sim.battle.soldiers.iter().map(|s| s.facing).collect();
        // Frame the action: wherever men are swinging, else the whole field.
        let fighting: Vec<_> = sim.battle.soldiers.iter().filter(|s| s.active() && s.since_attack < 2.0).collect();
        let alive: Vec<_> = if fighting.len() > 20 {
            fighting
        } else {
            sim.battle.soldiers.iter().filter(|s| s.active()).collect()
        };
        let mut c = fylking_sim::Vec2::ZERO;
        for s in &alive {
            c += s.pos;
        }
        c = c * (1.0 / alive.len().max(1) as f32);
        cam.focus = to_world(c, 0.0);
        cam.distance = if script.step == 2 { 45.0 } else { 120.0 };
        cam.pitch = if script.step == 2 { 0.4 } else { 0.75 };
        cam.yaw = if script.step == 2 { 0.6 } else { 0.0 };
    }
    if script.frame == frame {
        let path = script.dir.join(format!("{name}.png"));
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        script.step += 1;
    }
}

//! Total-War-style camera: pan with WASD, rotate with Q/E, zoom with the
//! wheel, tilt with Z/X or by dragging with the middle mouse button.

use crate::sim::{BattleRestarted, Sim};
use crate::view::to_world;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, (recentre_on_restart, control_camera).chain());
    }
}

#[derive(Component)]
pub struct RtsCamera {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl RtsCamera {
    /// Horizontal forward and right vectors (for panning).
    fn ground_axes(&self) -> (Vec3, Vec3) {
        let fwd = Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos());
        let right = Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin());
        (fwd, right)
    }

    fn transform(&self) -> Transform {
        let offset = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        ) * self.distance;
        Transform::from_translation(self.focus + offset).looking_at(self.focus, Vec3::Y)
    }
}

fn start_view(sim: &Sim) -> RtsCamera {
    let player: Vec<_> = sim.battle.units.iter().filter(|u| u.team == sim.player).collect();
    let mut c = fylking_sim::Vec2::ZERO;
    for u in &player {
        c += u.centre;
    }
    let c = c * (1.0 / player.len().max(1) as f32);
    RtsCamera {
        // Look a little past our own lines, towards the enemy.
        focus: to_world(c, 0.0) + Vec3::new(0.0, 0.0, -25.0),
        yaw: 0.0,
        pitch: 0.75,
        distance: 110.0,
    }
}

fn spawn_camera(mut commands: Commands, sim: Res<Sim>) {
    let rig = start_view(&sim);
    commands.spawn((Camera3d::default(), rig.transform(), rig));
}

fn recentre_on_restart(
    mut msgs: MessageReader<BattleRestarted>,
    sim: Res<Sim>,
    mut cam: Single<(&mut RtsCamera, &mut Transform)>,
) {
    if msgs.read().count() == 0 {
        return;
    }
    let rig = start_view(&sim);
    *cam.1 = rig.transform();
    *cam.0 = rig;
}

fn control_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    motion: Res<AccumulatedMouseMotion>,
    sim: Res<Sim>,
    mut cam: Single<(&mut RtsCamera, &mut Transform)>,
) {
    let (rig, transform) = &mut *cam;
    let dt = time.delta_secs();
    let (fwd, right) = rig.ground_axes();

    let mut pan = Vec3::ZERO;
    if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        pan += fwd;
    }
    if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        pan -= fwd;
    }
    if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
        pan += right;
    }
    if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
        pan -= right;
    }
    let fast = if keys.pressed(KeyCode::ShiftLeft) { 2.5 } else { 1.0 };
    let pan_speed = (20.0 + rig.distance * 0.8) * fast;
    rig.focus += pan.normalize_or_zero() * pan_speed * dt;

    if keys.pressed(KeyCode::KeyQ) {
        rig.yaw += 1.6 * dt;
    }
    if keys.pressed(KeyCode::KeyE) {
        rig.yaw -= 1.6 * dt;
    }
    if keys.pressed(KeyCode::KeyZ) {
        rig.pitch += 0.8 * dt;
    }
    if keys.pressed(KeyCode::KeyX) {
        rig.pitch -= 0.8 * dt;
    }
    if mouse.pressed(MouseButton::Middle) {
        rig.yaw -= motion.delta.x * 0.005;
        rig.pitch += motion.delta.y * 0.004;
    }

    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    rig.distance = (rig.distance * (1.0 - lines * 0.12)).clamp(8.0, 450.0);
    rig.pitch = rig.pitch.clamp(0.12, 1.45);

    // Keep the camera over the battlefield.
    let f = &sim.battle.field;
    rig.focus.x = rig.focus.x.clamp(-f.width / 2.0, f.width / 2.0);
    rig.focus.z = rig.focus.z.clamp(-f.height / 2.0, f.height / 2.0);

    **transform = rig.transform();
}

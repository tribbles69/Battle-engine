//! Turns the simulation into 3D: soldiers, banners, the field and lighting.
//!
//! Everything is built from primitive shapes so the prototype needs no art.
//! Each soldier is a root entity at their feet with mesh children; meshes and
//! materials are shared, so Bevy batches thousands of them cheaply.

use crate::input::Selection;
use crate::sim::{BattleRestarted, Sim};
use bevy::prelude::*;
use fylking_sim::{MoraleLevel, Team, UnitState, Vec2 as SimVec2};
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

pub struct ViewPlugin;

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.55, 0.68, 0.82)))
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb(0.9, 0.92, 1.0),
                brightness: 350.0,
                ..default()
            })
            .add_systems(Startup, (setup_assets, spawn_lights, spawn_battle).chain())
            .add_systems(Update, (respawn_on_restart, sync_soldiers, sync_banners).chain());
    }
}

/// Sim (x east, y north) to Bevy world (x east, y up, -z north).
pub fn to_world(p: SimVec2, height: f32) -> Vec3 {
    Vec3::new(p.x, height, -p.y)
}

pub fn to_sim(p: Vec3) -> SimVec2 {
    SimVec2::new(p.x, -p.z)
}

/// Rotation that points a model's forward (-Z) along a sim facing angle.
fn facing_rot(angle: f32) -> Quat {
    Quat::from_rotation_y(angle - FRAC_PI_2)
}

/// Everything that belongs to one battle, cleared on restart.
#[derive(Component)]
struct BattleScoped;

#[derive(Component)]
struct SoldierView {
    id: usize,
    /// The team-coloured child whose material changes with state.
    body: Entity,
    mounted: bool,
    look: Look,
}

#[derive(Component)]
struct BannerView {
    unit: usize,
    flag: Entity,
    look: Option<MoraleLevel>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Look {
    Normal,
    Selected,
    Routing,
    Dead,
}

#[derive(Resource)]
struct Palette {
    body: HashMap<(Team, Look), Handle<StandardMaterial>>,
    flag: HashMap<(Team, MoraleLevel), Handle<StandardMaterial>>,
    capsule: Handle<Mesh>,
    helmet: Handle<Mesh>,
    spear: Handle<Mesh>,
    sword: Handle<Mesh>,
    horse: Handle<Mesh>,
    horse_head: Handle<Mesh>,
    pole: Handle<Mesh>,
    flag_mesh: Handle<Mesh>,
    trunk: Handle<Mesh>,
    canopy: Handle<Mesh>,
    iron: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    horse_mat: Handle<StandardMaterial>,
    dead_horse: Handle<StandardMaterial>,
    leaves: Handle<StandardMaterial>,
    grass: Handle<StandardMaterial>,
}

fn team_colour(team: Team) -> Color {
    match team {
        Team::Blue => Color::srgb(0.16, 0.32, 0.78),
        Team::Red => Color::srgb(0.78, 0.14, 0.12),
    }
}

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let mut body = HashMap::new();
    let mut flag = HashMap::new();
    for team in [Team::Blue, Team::Red] {
        let base = team_colour(team).to_linear();
        let mk = |c: LinearRgba, emissive: LinearRgba| StandardMaterial {
            base_color: c.into(),
            emissive,
            perceptual_roughness: 0.85,
            ..default()
        };
        let pale = LinearRgba::rgb(
            base.red * 0.5 + 0.35,
            base.green * 0.5 + 0.35,
            base.blue * 0.5 + 0.35,
        );
        let dark = LinearRgba::rgb(base.red * 0.25, base.green * 0.25, base.blue * 0.25);
        body.insert((team, Look::Normal), mats.add(mk(base, LinearRgba::BLACK)));
        body.insert((team, Look::Selected), mats.add(mk(base, base * 0.6)));
        body.insert((team, Look::Routing), mats.add(mk(pale, LinearRgba::BLACK)));
        body.insert((team, Look::Dead), mats.add(mk(dark, LinearRgba::BLACK)));
        for level in [
            MoraleLevel::Eager,
            MoraleLevel::Steady,
            MoraleLevel::Wavering,
            MoraleLevel::Routing,
            MoraleLevel::Shattered,
        ] {
            let c = match level {
                MoraleLevel::Eager | MoraleLevel::Steady => base,
                MoraleLevel::Wavering => LinearRgba::rgb(0.95, 0.75, 0.1),
                MoraleLevel::Routing => LinearRgba::rgb(0.95, 0.95, 0.95),
                MoraleLevel::Shattered => LinearRgba::rgb(0.3, 0.3, 0.3),
            };
            let glow = if level == MoraleLevel::Eager { c * 0.4 } else { c * 0.15 };
            flag.insert(
                (team, level),
                mats.add(StandardMaterial {
                    base_color: c.into(),
                    emissive: glow,
                    cull_mode: None,
                    ..default()
                }),
            );
        }
    }
    let plain = |mats: &mut Assets<StandardMaterial>, c: Color| {
        mats.add(StandardMaterial { base_color: c, perceptual_roughness: 0.9, ..default() })
    };
    commands.insert_resource(Palette {
        body,
        flag,
        capsule: meshes.add(Capsule3d::new(0.27, 0.85)),
        helmet: meshes.add(Cone { radius: 0.34, height: 0.2 }),
        spear: meshes.add(Cuboid::new(0.05, 0.05, 2.6)),
        sword: meshes.add(Cuboid::new(0.05, 0.05, 0.9)),
        horse: meshes.add(Cuboid::new(0.6, 0.8, 1.9)),
        horse_head: meshes.add(Cuboid::new(0.35, 0.7, 0.55)),
        pole: meshes.add(Cylinder::new(0.06, 4.5)),
        flag_mesh: meshes.add(Cuboid::new(1.7, 1.1, 0.05)),
        trunk: meshes.add(Cylinder::new(0.3, 3.0)),
        canopy: meshes.add(Cone { radius: 2.2, height: 5.0 }),
        iron: plain(&mut mats, Color::srgb(0.35, 0.35, 0.38)),
        wood: plain(&mut mats, Color::srgb(0.42, 0.3, 0.18)),
        horse_mat: plain(&mut mats, Color::srgb(0.36, 0.24, 0.14)),
        dead_horse: plain(&mut mats, Color::srgb(0.16, 0.11, 0.07)),
        leaves: plain(&mut mats, Color::srgb(0.16, 0.34, 0.14)),
        grass: plain(&mut mats, Color::srgb(0.33, 0.47, 0.21)),
    });
}

fn spawn_lights(mut commands: Commands) {
    commands.spawn((
        DirectionalLight { illuminance: 9000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(60.0, 120.0, 40.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spawn_battle(mut commands: Commands, sim: Res<Sim>, pal: Res<Palette>, mut meshes: ResMut<Assets<Mesh>>) {
    let field = &sim.battle.field;
    // Ground, generously bigger than the playable area.
    commands.spawn((
        BattleScoped,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(field.width + 400.0, field.height + 400.0))),
        MeshMaterial3d(pal.grass.clone()),
    ));

    // A ring of trees outside the field, for a sense of place and scale.
    let mut seed = 12345u32;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    for _ in 0..140 {
        let side = rnd();
        let along = rnd() - 0.5;
        let out = 15.0 + rnd() * 120.0;
        let (hw, hh) = (field.width / 2.0, field.height / 2.0);
        let p = if side < 0.25 {
            Vec3::new(along * (field.width + 200.0), 0.0, -(hh + out))
        } else if side < 0.5 {
            Vec3::new(along * (field.width + 200.0), 0.0, hh + out)
        } else if side < 0.75 {
            Vec3::new(-(hw + out), 0.0, along * (field.height + 200.0))
        } else {
            Vec3::new(hw + out, 0.0, along * (field.height + 200.0))
        };
        let s = 0.7 + rnd() * 0.8;
        commands.spawn((BattleScoped, Transform::from_translation(p).with_scale(Vec3::splat(s)), Visibility::default()))
            .with_children(|t| {
                t.spawn((Mesh3d(pal.trunk.clone()), MeshMaterial3d(pal.wood.clone()), Transform::from_xyz(0.0, 1.5, 0.0)));
                t.spawn((Mesh3d(pal.canopy.clone()), MeshMaterial3d(pal.leaves.clone()), Transform::from_xyz(0.0, 5.0, 0.0)));
            });
    }

    for (i, s) in sim.battle.soldiers.iter().enumerate() {
        let unit = &sim.battle.units[s.unit];
        let team_mat = pal.body[&(unit.team, Look::Normal)].clone();
        let root = Transform::from_translation(to_world(s.pos, 0.0)).with_rotation(facing_rot(s.facing));
        let mut body = Entity::PLACEHOLDER;
        let mounted = unit.stats.mounted;
        let long_weapon = unit.stats.reach >= 1.2;
        let mut e = commands.spawn((BattleScoped, root, Visibility::default()));
        e.with_children(|c| {
            if mounted {
                c.spawn((Mesh3d(pal.horse.clone()), MeshMaterial3d(pal.horse_mat.clone()), Transform::from_xyz(0.0, 1.0, 0.0)));
                c.spawn((Mesh3d(pal.horse_head.clone()), MeshMaterial3d(pal.horse_mat.clone()), Transform::from_xyz(0.0, 1.45, -1.0)));
                body = c
                    .spawn((Mesh3d(pal.capsule.clone()), MeshMaterial3d(team_mat.clone()), Transform::from_xyz(0.0, 2.0, 0.1)))
                    .id();
                c.spawn((Mesh3d(pal.helmet.clone()), MeshMaterial3d(pal.iron.clone()), Transform::from_xyz(0.0, 2.75, 0.1)));
                c.spawn((Mesh3d(pal.spear.clone()), MeshMaterial3d(pal.wood.clone()), Transform::from_xyz(0.35, 2.0, -0.9)));
            } else {
                body = c
                    .spawn((Mesh3d(pal.capsule.clone()), MeshMaterial3d(team_mat.clone()), Transform::from_xyz(0.0, 0.7, 0.0)))
                    .id();
                c.spawn((Mesh3d(pal.helmet.clone()), MeshMaterial3d(pal.iron.clone()), Transform::from_xyz(0.0, 1.45, 0.0)));
                if long_weapon {
                    c.spawn((Mesh3d(pal.spear.clone()), MeshMaterial3d(pal.wood.clone()), Transform::from_xyz(0.3, 0.9, -0.6)));
                } else {
                    c.spawn((Mesh3d(pal.sword.clone()), MeshMaterial3d(pal.iron.clone()), Transform::from_xyz(0.3, 0.85, -0.45)));
                }
            }
        });
        e.insert(SoldierView { id: i, body, mounted, look: Look::Normal });
    }

    for u in &sim.battle.units {
        let flag_mat = pal.flag[&(u.team, MoraleLevel::Steady)].clone();
        let mut flag = Entity::PLACEHOLDER;
        let mut e = commands.spawn((BattleScoped, Transform::from_translation(to_world(u.centre, 0.0)), Visibility::default()));
        e.with_children(|c| {
            c.spawn((Mesh3d(pal.pole.clone()), MeshMaterial3d(pal.wood.clone()), Transform::from_xyz(0.0, 2.25, 0.0)));
            flag = c
                .spawn((Mesh3d(pal.flag_mesh.clone()), MeshMaterial3d(flag_mat), Transform::from_xyz(0.9, 3.9, 0.0)))
                .id();
        });
        e.insert(BannerView { unit: u.id, flag, look: None });
    }
}

fn respawn_on_restart(
    mut msgs: MessageReader<BattleRestarted>,
    mut commands: Commands,
    scoped: Query<Entity, With<BattleScoped>>,
    sim: Res<Sim>,
    pal: Res<Palette>,
    meshes: ResMut<Assets<Mesh>>,
) {
    if msgs.read().count() == 0 {
        return;
    }
    for e in &scoped {
        commands.entity(e).despawn();
    }
    spawn_battle(commands, sim, pal, meshes);
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    a + fylking_sim::math::angle_diff(a, b) * t
}

fn sync_soldiers(
    time: Res<Time>,
    sim: Res<Sim>,
    sel: Res<Selection>,
    pal: Res<Palette>,
    mut roots: Query<(&mut SoldierView, &mut Transform, &mut Visibility, &Children)>,
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    let b = &sim.battle;
    let t = time.elapsed_secs();
    for (mut view, mut tf, mut vis, children) in &mut roots {
        let Some(s) = b.soldiers.get(view.id) else { continue };
        if s.fled {
            *vis = Visibility::Hidden;
            continue;
        }
        let unit = &b.units[s.unit];
        let pos = if s.alive {
            let p0 = sim.prev_pos[view.id];
            p0 + (s.pos - p0) * sim.alpha
        } else {
            s.pos
        };
        let facing = lerp_angle(sim.prev_facing[view.id], s.facing, sim.alpha);
        let mut rot = facing_rot(facing);
        let mut lift = 0.0;
        let mut lean = 0.0;

        if !s.alive {
            // Topple over, then lie still.
            let k = ((b.time - s.death_time) / 0.5).clamp(0.0, 1.0);
            if view.mounted {
                rot *= Quat::from_rotation_z(FRAC_PI_2 * k);
            } else {
                lean = FRAC_PI_2 * k;
            }
        } else {
            let speed = s.vel.length();
            let bob_rate = if view.mounted { 7.0 } else { 10.0 };
            lift = (t * bob_rate + view.id as f32).sin().abs() * 0.07 * (speed / 2.0).min(1.0);
            if s.stun_timer > 0.0 {
                lean = 1.1; // knocked flat on his back
            } else if s.since_attack < 0.35 {
                lean = -0.35 * (1.0 - s.since_attack / 0.35); // lunge
            } else if s.since_hit < 0.2 {
                lean = 0.25; // flinch
            }
        }
        rot *= Quat::from_rotation_x(lean);
        tf.translation = to_world(pos, lift);
        tf.rotation = rot;
        *vis = Visibility::Inherited;

        let look = if !s.alive {
            Look::Dead
        } else if unit.state.is_broken() {
            Look::Routing
        } else if sel.0.contains(&s.unit) {
            Look::Selected
        } else {
            Look::Normal
        };
        if look != view.look {
            view.look = look;
            if let Ok(mut m) = materials.get_mut(view.body) {
                m.0 = pal.body[&(unit.team, look)].clone();
            }
            if view.mounted && look == Look::Dead {
                for c in children.iter() {
                    if let Ok(mut m) = materials.get_mut(c) {
                        if m.0 == pal.horse_mat {
                            m.0 = pal.dead_horse.clone();
                        }
                    }
                }
            }
        }
    }
}

fn sync_banners(
    sim: Res<Sim>,
    sel: Res<Selection>,
    pal: Res<Palette>,
    time: Res<Time>,
    mut banners: Query<(&mut BannerView, &mut Transform, &mut Visibility)>,
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    for (mut view, mut tf, mut vis) in &mut banners {
        let u = &sim.battle.units[view.unit];
        if u.state == UnitState::Destroyed || u.alive_count == 0 {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let height = if u.stats.mounted { 3.4 } else { 2.0 };
        let target = to_world(u.centre, height);
        // Glide rather than jitter as men shuffle about.
        let k = (time.delta_secs() * 6.0).min(1.0);
        tf.translation = tf.translation.lerp(target, k);
        let selected = sel.0.contains(&u.id);
        tf.scale = Vec3::splat(if selected { 1.35 } else { 1.0 });
        // Banners face the same way as the unit.
        tf.rotation = facing_rot(u.anchor_facing);
        let level = u.morale_level();
        if view.look != Some(level) {
            view.look = Some(level);
            if let Ok(mut m) = materials.get_mut(view.flag) {
                m.0 = pal.flag[&(u.team, level)].clone();
            }
        }
    }
}

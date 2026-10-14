//! Selecting units and giving orders, Total War style.
//!
//! Left click / drag a box: select.   Shift: add to selection.
//! Right click ground: move.           Right click enemy: attack.
//! Right drag: draw the front line of a new formation (width and facing).
//! Double right-click: run.  R: toggle run.  H: halt.
//! Ctrl+A: select all.  1-9: select a unit.  Esc: clear.

use crate::camera::RtsCamera;
use crate::sim::Sim;
use crate::view::{to_sim, to_world};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fylking_sim::{Battle, Order, UnitId, Vec2 as SimVec2};
use std::f32::consts::FRAC_PI_2;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>()
            .init_resource::<Drag>()
            .add_systems(Startup, spawn_select_box)
            .add_systems(Update, (keyboard_orders, mouse_orders, draw_overlays, prune_selection).chain());
    }
}

#[derive(Resource, Default)]
pub struct Selection(pub Vec<UnitId>);

#[derive(Resource, Default)]
struct Drag {
    left_start: Option<Vec2>,
    right_start: Option<SimVec2>,
    last_right_click: f32,
}

/// The rubber-band rectangle drawn while box selecting.
#[derive(Component)]
struct SelectBox;

/// The cursor's screen position and the ground point under it.
pub struct Cursor {
    pub screen: Vec2,
    pub ground: SimVec2,
}

pub fn cursor_on_ground(window: &Window, camera: &Camera, cam_tf: &GlobalTransform) -> Option<Cursor> {
    let screen = window.cursor_position()?;
    let ray = camera.viewport_to_world(cam_tf, screen).ok()?;
    let dist = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    Some(Cursor { screen, ground: to_sim(ray.get_point(dist)) })
}

fn spawn_select_box(mut commands: Commands) {
    commands.spawn((
        SelectBox,
        Node {
            position_type: PositionType::Absolute,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.8)),
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
        Visibility::Hidden,
    ));
}

fn own_units(sim: &Sim) -> impl Iterator<Item = &fylking_sim::Unit> {
    sim.battle.units.iter().filter(|u| u.team == sim.player && u.alive_count > 0)
}

fn keyboard_orders(keys: Res<ButtonInput<KeyCode>>, mut sim: ResMut<Sim>, mut sel: ResMut<Selection>) {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if keys.just_pressed(KeyCode::Escape) {
        sel.0.clear();
    }
    if ctrl && keys.just_pressed(KeyCode::KeyA) {
        sel.0 = own_units(&sim).map(|u| u.id).collect();
    }
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    let mine: Vec<UnitId> = sim.battle.units.iter().filter(|u| u.team == sim.player).map(|u| u.id).collect();
    for (n, key) in digits.iter().enumerate() {
        if keys.just_pressed(*key) {
            if let Some(&id) = mine.get(n) {
                toggle_or_set(&mut sel, id, shift);
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyR) && !sel.0.is_empty() {
        let any_walking = sel.0.iter().any(|&u| !sim.battle.units[u].running);
        for &u in &sel.0 {
            sim.battle.set_running(u, any_walking);
        }
    }
    if keys.just_pressed(KeyCode::KeyH) {
        for &u in &sel.0 {
            let here = sim.battle.units[u].centre;
            sim.battle.order(u, Order::Move { to: here, facing: None, files: None });
        }
    }
}

fn toggle_or_set(sel: &mut Selection, id: UnitId, add: bool) {
    if add {
        if let Some(i) = sel.0.iter().position(|&u| u == id) {
            sel.0.remove(i);
        } else {
            sel.0.push(id);
        }
    } else {
        sel.0 = vec![id];
    }
}

/// Drop units that can no longer take orders.
fn prune_selection(sim: Res<Sim>, mut sel: ResMut<Selection>) {
    let b = &sim.battle;
    sel.0.retain(|&u| u < b.units.len() && b.units[u].alive_count > 0 && b.units[u].team == sim.player);
}

#[allow(clippy::too_many_arguments)]
fn mouse_orders(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
    ui_hover: Query<&Interaction>,
    mut sim: ResMut<Sim>,
    mut sel: ResMut<Selection>,
    mut drag: ResMut<Drag>,
    mut boxes: Query<(&mut Node, &mut Visibility), With<SelectBox>>,
) {
    let (cam, cam_tf) = *camera;
    let Some(cursor) = cursor_on_ground(&window, cam, cam_tf) else { return };
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let over_ui = ui_hover.iter().any(|i| *i != Interaction::None);

    // ---- Left button: selection ----
    if mouse.just_pressed(MouseButton::Left) && !over_ui {
        drag.left_start = Some(cursor.screen);
    }
    if let (Some(start), Ok((mut node, mut vis))) = (drag.left_start, boxes.single_mut()) {
        let min = start.min(cursor.screen);
        let size = (start - cursor.screen).abs();
        if size.length() > 6.0 && mouse.pressed(MouseButton::Left) {
            node.left = Val::Px(min.x);
            node.top = Val::Px(min.y);
            node.width = Val::Px(size.x);
            node.height = Val::Px(size.y);
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }
    if mouse.just_released(MouseButton::Left) {
        if let Some(start) = drag.left_start.take() {
            if start.distance(cursor.screen) > 6.0 {
                let (min, max) = (start.min(cursor.screen), start.max(cursor.screen));
                let picked: Vec<UnitId> = own_units(&sim)
                    .filter(|u| {
                        cam.world_to_viewport(cam_tf, to_world(u.centre, 1.0))
                            .is_ok_and(|p| p.cmpge(min).all() && p.cmple(max).all())
                    })
                    .map(|u| u.id)
                    .collect();
                if !shift {
                    sel.0.clear();
                }
                for id in picked {
                    if !sel.0.contains(&id) {
                        sel.0.push(id);
                    }
                }
            } else {
                match sim.battle.unit_at(cursor.ground, 2.5) {
                    Some(u) if sim.battle.units[u].team == sim.player => toggle_or_set(&mut sel, u, shift),
                    Some(_) => {} // clicking an enemy just looks at it
                    None if !shift => sel.0.clear(),
                    None => {}
                }
            }
            if let Ok((_, mut vis)) = boxes.single_mut() {
                *vis = Visibility::Hidden;
            }
        }
    }

    // ---- Right button: orders ----
    if sel.0.is_empty() {
        drag.right_start = None;
        return;
    }
    if mouse.just_pressed(MouseButton::Right) && !over_ui {
        drag.right_start = Some(cursor.ground);
    }
    if mouse.just_released(MouseButton::Right) {
        let Some(start) = drag.right_start.take() else { return };
        let now = time.elapsed_secs();
        let double = now - drag.last_right_click < 0.35;
        drag.last_right_click = now;
        let player = sim.player;
        let b = &mut sim.battle;
        if double {
            for &u in &sel.0 {
                b.set_running(u, true);
            }
        }
        if start.distance(cursor.ground) > 3.0 {
            let cam_fwd = to_sim(cam_tf.forward().as_vec3());
            for p in plan_line(b, &sel.0, start, cursor.ground, cam_fwd) {
                b.order(p.unit, Order::Move { to: p.anchor, facing: Some(p.facing), files: Some(p.files) });
            }
            return;
        }
        match b.unit_at(cursor.ground, 2.5) {
            Some(target) if b.units[target].team != player => {
                for &u in &sel.0 {
                    b.order(u, Order::Attack { target });
                }
            }
            _ => group_move(b, &sel.0, cursor.ground),
        }
    }
}

/// Move a group to `to`, keeping their positions relative to each other.
fn group_move(b: &mut Battle, units: &[UnitId], to: SimVec2) {
    let mut g = SimVec2::ZERO;
    for &u in units {
        g += b.units[u].centre;
    }
    let g = g * (1.0 / units.len() as f32);
    let travel = to - g;
    let facing = (travel.length() > 5.0).then(|| travel.angle());
    for &u in units {
        let offset = b.units[u].centre - g;
        b.order(u, Order::Move { to: to + offset, facing, files: None });
    }
}

pub struct LinePlan {
    pub unit: UnitId,
    pub anchor: SimVec2,
    pub facing: f32,
    pub files: u32,
    pub width: f32,
    pub depth: f32,
}

/// Split a dragged front line between the selected units, left to right.
pub fn plan_line(b: &Battle, units: &[UnitId], a: SimVec2, c: SimVec2, cam_fwd: SimVec2) -> Vec<LinePlan> {
    let along = (c - a).normalize_or_zero();
    let len = a.distance(c);
    // Face away from the camera: the side of the line you're looking towards.
    let mut normal = along.perp_right();
    if normal.dot(cam_fwd) < 0.0 {
        normal = -normal;
    }
    // Keep units in their left-to-right order along the new line.
    let mut order: Vec<UnitId> = units.to_vec();
    order.sort_by(|&x, &y| b.units[x].centre.dot(along).total_cmp(&b.units[y].centre.dot(along)));
    let share = |u: UnitId| b.units[u].alive_count.max(1) as f32 * b.units[u].stats.spacing;
    let total: f32 = order.iter().map(|&u| share(u)).sum();
    let gap = if order.len() > 1 { 2.0 } else { 0.0 };
    let usable = (len - gap * (order.len() - 1) as f32).max(order.len() as f32);
    let mut pos = 0.0;
    order
        .into_iter()
        .map(|u| {
            let unit = &b.units[u];
            let seg = usable * share(u) / total;
            let files = ((seg / unit.stats.spacing).round() as u32).clamp(1, unit.alive_count.max(1));
            let width = files as f32 * unit.stats.spacing;
            let depth = (unit.alive_count.max(1) as f32 / files as f32).ceil() * unit.stats.spacing;
            let front_mid = a + along * (pos + seg / 2.0);
            pos += seg + gap;
            LinePlan { unit: u, anchor: front_mid - normal * (depth / 2.0), facing: normal.angle(), files, width, depth }
        })
        .collect()
}

/// Outline of a formation on the ground, with the front edge highlighted.
fn draw_formation(g: &mut Gizmos, centre: SimVec2, facing: f32, width: f32, depth: f32, col: Color, front: Color) {
    let f = SimVec2::from_angle(facing);
    let r = f.perp_right();
    let (hw, hd) = (width / 2.0, depth / 2.0);
    let corner = |x: f32, y: f32| to_world(centre + r * x + f * y, 0.12);
    let (fl, fr, bl, br) = (corner(-hw, hd), corner(hw, hd), corner(-hw, -hd), corner(hw, -hd));
    g.line(fl, fr, front);
    g.line(fr, br, col);
    g.line(br, bl, col);
    g.line(bl, fl, col);
    // A little arrow pointing the way the formation faces.
    let tip = to_world(centre + f * (hd + 2.5), 0.12);
    g.line(corner(-1.2, hd + 0.5), tip, front);
    g.line(corner(1.2, hd + 0.5), tip, front);
}

#[allow(clippy::too_many_arguments)]
fn draw_overlays(
    mut g: Gizmos,
    sim: Res<Sim>,
    sel: Res<Selection>,
    drag: Res<Drag>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
) {
    let b = &sim.battle;
    let (cam, cam_tf) = *camera;
    let cursor = cursor_on_ground(&window, cam, cam_tf);
    let field = &b.field;
    g.rect(
        Isometry3d::new(Vec3::new(0.0, 0.05, 0.0), Quat::from_rotation_x(-FRAC_PI_2)),
        Vec2::new(field.width, field.height),
        Color::srgba(1.0, 1.0, 1.0, 0.25),
    );

    let soft = Color::srgba(0.7, 0.9, 1.0, 0.5);
    let bright = Color::srgb(0.6, 1.0, 0.6);
    for &uid in &sel.0 {
        let u = &b.units[uid];
        g.circle(
            Isometry3d::new(to_world(u.centre, 0.15), Quat::from_rotation_x(FRAC_PI_2)),
            2.0,
            bright,
        );
        match u.order {
            Order::Move { to, facing, files } => {
                g.line(to_world(u.centre, 0.15), to_world(to, 0.15), soft);
                let files = files.unwrap_or(u.files).min(u.alive_count.max(1));
                let width = files as f32 * u.stats.spacing;
                let depth = (u.alive_count.max(1) as f32 / files.max(1) as f32).ceil() * u.stats.spacing;
                let face = facing.unwrap_or((to - u.anchor).angle());
                draw_formation(&mut g, to, face, width, depth, soft, bright);
            }
            Order::Attack { target } => {
                let t = &b.units[target];
                g.line(to_world(u.centre, 0.2), to_world(t.centre, 0.2), Color::srgb(1.0, 0.3, 0.2));
            }
            Order::Hold => {
                if !u.state.is_broken() {
                    draw_formation(&mut g, u.anchor, u.anchor_facing, u.width(), u.depth(), soft, bright);
                }
            }
        }
    }

    let Some(cursor) = cursor else { return };
    // Hovering an enemy with units selected: show it as an attack target.
    if !sel.0.is_empty() {
        if let Some(e) = b.unit_at(cursor.ground, 2.5) {
            if b.units[e].team != sim.player {
                g.circle(
                    Isometry3d::new(to_world(b.units[e].centre, 0.2), Quat::from_rotation_x(FRAC_PI_2)),
                    b.units[e].width().max(4.0) / 2.0,
                    Color::srgb(1.0, 0.25, 0.2),
                );
            }
        }
    }
    // Live preview while dragging out a formation.
    if let (Some(start), true) = (drag.right_start, mouse.pressed(MouseButton::Right)) {
        if start.distance(cursor.ground) > 3.0 && !sel.0.is_empty() {
            let cam_fwd = to_sim(cam_tf.forward().as_vec3());
            g.line(to_world(start, 0.15), to_world(cursor.ground, 0.15), Color::WHITE);
            for p in plan_line(b, &sel.0, start, cursor.ground, cam_fwd) {
                draw_formation(&mut g, p.anchor, p.facing, p.width, p.depth, soft, bright);
            }
        }
    }
}

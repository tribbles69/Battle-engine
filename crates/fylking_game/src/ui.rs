//! Heads-up display: battle clock, unit cards with morale and stamina,
//! the event feed, a help panel and the victory banner.

use crate::input::Selection;
use crate::sim::{BattleRestarted, EventFeed, ScenarioList, Sim};
use bevy::prelude::*;
use fylking_sim::{MoraleLevel, Order, Team, UnitId, UnitState};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_hud, spawn_cards).chain()).add_systems(
            Update,
            (rebuild_cards, click_cards, update_cards, update_hud, toggle_help).chain(),
        );
    }
}

#[derive(Component)]
struct InfoText;
#[derive(Component)]
struct FeedText;
#[derive(Component)]
pub struct HelpPanel;
#[derive(Component)]
struct VictoryText;
#[derive(Component)]
struct CardsRoot;
#[derive(Component)]
struct Card(UnitId);
#[derive(Component)]
struct CardLine(UnitId);
#[derive(Component)]
struct MoraleBar(UnitId);
#[derive(Component)]
struct StaminaBar(UnitId);

const PANEL: Color = Color::srgba(0.05, 0.05, 0.07, 0.72);

fn text(s: impl Into<String>, size: f32, colour: Color) -> impl Bundle {
    (Text::new(s), TextFont::from_font_size(size), TextColor(colour))
}

const HELP: &str = "\
CAMERA   WASD / arrows pan   Q E rotate   Z X tilt   wheel zoom   middle-drag look
SELECT   left click   drag a box   Shift adds   Ctrl+A all   1-9 by number   Esc clear
ORDERS   right click ground: move   right click enemy: attack
         right DRAG: draw the front line (sets width + facing)
         double right-click or R: run   H: halt
BATTLE   Space pause   + / - speed   F5 restart   Tab next scenario   F1 hide this";

fn spawn_hud(mut commands: Commands) {
    // Top left: scenario + clock.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(12.0),
            top: Val::Px(12.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(PANEL),
        children![(InfoText, text("", 16.0, Color::WHITE))],
    ));
    // Top right: what just happened.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            top: Val::Px(12.0),
            max_width: Val::Px(380.0),
            ..default()
        },
        children![(FeedText, text("", 15.0, Color::WHITE), TextLayout::justify(Justify::Right))],
    ));
    // Centre: controls, shown at the start.
    commands.spawn((
        HelpPanel,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Px(70.0),
            margin: UiRect::left(Val::Px(-390.0)),
            width: Val::Px(780.0),
            padding: UiRect::all(Val::Px(14.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(PANEL),
        children![text(HELP, 13.0, Color::srgb(0.9, 0.9, 0.85))],
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            top: Val::Percent(30.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(VictoryText, text("", 52.0, Color::WHITE), Visibility::Hidden)],
    ));
}

fn spawn_cards(mut commands: Commands, sim: Res<Sim>) {
    let root = commands
        .spawn((
            CardsRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(8.0),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
        ))
        .id();
    for u in sim.battle.units.iter().filter(|u| u.team == sim.player) {
        let id = u.id;
        let bar = |marker: Box<dyn FnOnce(&mut EntityCommands)>, colour: Color, commands: &mut Commands| {
            let inner = commands
                .spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(colour)))
                .id();
            marker(&mut commands.entity(inner));
            let outer = commands
                .spawn((
                    Node { width: Val::Percent(100.0), height: Val::Px(6.0), margin: UiRect::top(Val::Px(4.0)), ..default() },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.12)),
                ))
                .id();
            commands.entity(outer).add_child(inner);
            outer
        };
        let morale = bar(Box::new(move |e| { e.insert(MoraleBar(id)); }), Color::WHITE, &mut commands);
        let stamina = bar(Box::new(move |e| { e.insert(StaminaBar(id)); }), Color::srgb(0.85, 0.75, 0.3), &mut commands);
        let card = commands
            .spawn((
                Card(id),
                Button,
                Node {
                    width: Val::Px(172.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(Color::NONE),
            ))
            .id();
        let name = commands.spawn(text(format!("{}. {}", position_label(&sim, id), u.name), 14.0, Color::WHITE)).id();
        let line = commands.spawn((CardLine(id), text("", 12.0, Color::srgb(0.8, 0.8, 0.8)))).id();
        commands.entity(card).add_children(&[name, line, morale, stamina]);
        commands.entity(root).add_child(card);
    }
}

/// The number key that selects this unit.
fn position_label(sim: &Sim, id: UnitId) -> usize {
    sim.battle.units.iter().filter(|u| u.team == sim.player).position(|u| u.id == id).unwrap_or(0) + 1
}

fn rebuild_cards(
    mut msgs: MessageReader<BattleRestarted>,
    mut commands: Commands,
    roots: Query<Entity, With<CardsRoot>>,
    sim: Res<Sim>,
    mut victory: Query<&mut Visibility, With<VictoryText>>,
) {
    if msgs.read().count() == 0 {
        return;
    }
    for r in &roots {
        commands.entity(r).despawn();
    }
    for mut v in &mut victory {
        *v = Visibility::Hidden;
    }
    spawn_cards(commands, sim);
}

fn click_cards(
    cards: Query<(&Interaction, &Card), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut sel: ResMut<Selection>,
) {
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    for (i, card) in &cards {
        if *i == Interaction::Pressed {
            if shift {
                if let Some(p) = sel.0.iter().position(|&u| u == card.0) {
                    sel.0.remove(p);
                } else {
                    sel.0.push(card.0);
                }
            } else {
                sel.0 = vec![card.0];
            }
        }
    }
}

fn morale_colour(level: MoraleLevel) -> Color {
    match level {
        MoraleLevel::Eager => Color::srgb(0.35, 0.85, 0.4),
        MoraleLevel::Steady => Color::srgb(0.55, 0.8, 0.95),
        MoraleLevel::Wavering => Color::srgb(0.95, 0.75, 0.15),
        MoraleLevel::Routing => Color::srgb(0.95, 0.3, 0.25),
        MoraleLevel::Shattered => Color::srgb(0.4, 0.4, 0.4),
    }
}

#[allow(clippy::type_complexity)]
fn update_cards(
    sim: Res<Sim>,
    sel: Res<Selection>,
    mut cards: Query<(&Card, &mut BorderColor, &mut BackgroundColor)>,
    mut lines: Query<(&CardLine, &mut Text)>,
    mut morale: Query<(&MoraleBar, &mut Node, &mut BackgroundColor), (Without<Card>, Without<StaminaBar>)>,
    mut stamina: Query<(&StaminaBar, &mut Node), Without<MoraleBar>>,
) {
    let b = &sim.battle;
    for (card, mut border, mut bg) in &mut cards {
        let u = &b.units[card.0];
        *border = BorderColor::all(if sel.0.contains(&card.0) { Color::srgb(0.6, 1.0, 0.6) } else { Color::NONE });
        bg.0 = if u.alive_count == 0 { Color::srgba(0.2, 0.05, 0.05, 0.7) } else { PANEL };
    }
    for (line, mut t) in &mut lines {
        let u = &b.units[line.0];
        let level = u.morale_level();
        let state = match (u.state, u.order) {
            (UnitState::Destroyed, _) => "Destroyed",
            (UnitState::Shattered, _) => "Shattered",
            (UnitState::Routing, _) => "ROUTING",
            (UnitState::Fighting, _) => "Fighting",
            (UnitState::Charging, _) => "Charging!",
            (UnitState::Moving, _) if u.running => "Running",
            (UnitState::Moving, _) => "Marching",
            (_, Order::Attack { .. }) => "Attacking",
            _ => "Holding",
        };
        let mood = match level {
            MoraleLevel::Eager => "Eager",
            MoraleLevel::Steady => "Steady",
            MoraleLevel::Wavering => "Wavering",
            MoraleLevel::Routing => "Broken",
            MoraleLevel::Shattered => "Shattered",
        };
        let pace = if u.running && state != "Running" { " (run)" } else { "" };
        t.0 = format!("{}/{} men, {} kills\n{}{}, {}", u.alive_count, u.initial_men, u.kills, state, pace, mood);
    }
    for (bar, mut node, mut bg) in &mut morale {
        let u = &b.units[bar.0];
        let frac = (u.morale / u.stats.morale.max(1.0)).clamp(0.0, 1.0);
        node.width = Val::Percent(frac * 100.0);
        bg.0 = morale_colour(u.morale_level());
    }
    for (bar, mut node) in &mut stamina {
        node.width = Val::Percent(b.units[bar.0].stamina);
    }
}

fn update_hud(
    sim: Res<Sim>,
    feed: Res<EventFeed>,
    list: Res<ScenarioList>,
    mut info: Query<&mut Text, (With<InfoText>, Without<FeedText>, Without<VictoryText>)>,
    mut feed_text: Query<&mut Text, (With<FeedText>, Without<InfoText>, Without<VictoryText>)>,
    mut victory: Query<(&mut Text, &mut TextColor, &mut Visibility), (With<VictoryText>, Without<InfoText>, Without<FeedText>)>,
) {
    let b = &sim.battle;
    let t = b.time as u32;
    let status = if sim.paused { "PAUSED  |  Space to play".to_string() } else { format!("x{}", sim.speed) };
    let men = |team: Team| b.units.iter().filter(|u| u.team == team).map(|u| u.alive_count).sum::<u32>();
    let (mine, theirs) = (men(sim.player), men(sim.player.enemy()));
    if let Ok(mut text) = info.single_mut() {
        text.0 = format!(
            "{}  ({}/{})\n{}:{:02}   {}\nYour men {}   Enemy {}",
            sim.scenario.name,
            list.current + 1,
            list.paths.len(),
            t / 60,
            t % 60,
            status,
            mine,
            theirs
        );
    }
    if let Ok(mut text) = feed_text.single_mut() {
        text.0 = feed
            .lines
            .iter()
            .map(|(time, line, _)| format!("{}:{:02}  {}", *time as u32 / 60, *time as u32 % 60, line))
            .collect::<Vec<_>>()
            .join("\n");
    }
    if let Ok((mut text, mut colour, mut vis)) = victory.single_mut() {
        if let Some(w) = b.winner {
            let won = w == sim.player;
            text.0 = if won { "VICTORY".into() } else { "DEFEAT".into() };
            text.0.push_str("\nF5 to fight again  |  Tab for the next battle");
            colour.0 = if won { Color::srgb(1.0, 0.85, 0.3) } else { Color::srgb(0.9, 0.25, 0.2) };
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

fn toggle_help(keys: Res<ButtonInput<KeyCode>>, mut panel: Query<&mut Visibility, With<HelpPanel>>) {
    if keys.just_pressed(KeyCode::F1) {
        for mut v in &mut panel {
            *v = match *v {
                Visibility::Hidden => Visibility::Inherited,
                _ => Visibility::Hidden,
            };
        }
    }
}

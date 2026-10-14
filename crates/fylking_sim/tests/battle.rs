//! Behaviour tests for the battle sim, run headless.

use fylking_sim::battle::formation_slots;
use fylking_sim::{Battle, BattleEvent, Order, Scenario, Team, UnitState, UnitTypes, Vec2};
use std::path::PathBuf;

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/data")
}

fn types() -> UnitTypes {
    UnitTypes::from_toml(&std::fs::read_to_string(data_dir().join("units.toml")).unwrap()).unwrap()
}

fn scenario(name: &str) -> Scenario {
    let p = data_dir().join("scenarios").join(format!("{name}.toml"));
    Scenario::from_toml(&std::fs::read_to_string(p).unwrap()).unwrap()
}

/// A custom scenario from inline TOML units.
fn custom(units: &str) -> Battle {
    let src = format!(
        "name = \"t\"\nplayer_team = \"blue\"\nseed = 3\n[battlefield]\nwidth = 300\nheight = 300\n{units}"
    );
    let mut b = Battle::new(&Scenario::from_toml(&src).unwrap(), &types()).unwrap();
    b.ai_teams.clear(); // tests drive both sides by hand
    b
}

#[test]
fn all_data_files_load() {
    let t = types();
    assert!(t.types.len() >= 3);
    for entry in std::fs::read_dir(data_dir().join("scenarios")).unwrap() {
        let path = entry.unwrap().path();
        let s = Scenario::from_toml(&std::fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        Battle::new(&s, &t).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

#[test]
fn unknown_unit_type_is_an_error() {
    let src = "name = \"t\"\nplayer_team = \"blue\"\n[battlefield]\nwidth = 100\nheight = 100\n\
               [[units]]\ntype = \"dragon\"\nteam = \"red\"\npos = [0, 0]\nfacing = 0\n";
    let err = Battle::new(&Scenario::from_toml(src).unwrap(), &types()).err().unwrap();
    assert!(err.contains("dragon"));
}

#[test]
fn formation_is_centred_and_complete() {
    let slots = formation_slots(25, 10, 1.0);
    assert_eq!(slots.len(), 25);
    let c = slots.iter().fold(Vec2::ZERO, |a, &s| a + s) * (1.0 / 25.0);
    assert!(c.x.abs() < 0.01, "centred left-right: {c:?}");
    // 3 ranks: front rank is furthest forward.
    assert!(slots[0].y > slots[24].y);
}

#[test]
fn battles_are_deterministic() {
    let run = || {
        let mut b = Battle::new(&scenario("first_clash"), &types()).unwrap();
        b.ai_teams = vec![Team::Blue, Team::Red];
        b.run_for(60.0);
        b.units.iter().map(|u| (u.alive_count, u.state)).collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

#[test]
fn move_order_arrives_in_formation_and_facing() {
    let mut b = custom(
        "[[units]]\ntype = \"yari_ashigaru\"\nteam = \"blue\"\npos = [0, 0]\nfacing = 90\n\
         [[units]]\ntype = \"yari_ashigaru\"\nteam = \"red\"\npos = [0, 140]\nfacing = 270\n",
    );
    let facing = 0.0; // face east on arrival
    b.order(0, Order::Move { to: Vec2::new(-60.0, 20.0), facing: Some(facing), files: Some(12) });
    b.run_for(70.0);
    let u = &b.units[0];
    assert_eq!(u.order, Order::Hold, "should have arrived");
    assert!(u.centre.distance(Vec2::new(-60.0, 20.0)) < 2.0, "centre {:?}", u.centre);
    assert!(u.slot_error < 1.0, "formation should be tidy, error {}", u.slot_error);
    assert!(u.anchor_facing.abs() < 0.01);
    assert_eq!(u.files, 12);
}

#[test]
fn running_is_faster_than_walking() {
    let time_to_arrive = |run: bool| {
        let mut b = custom(
            "[[units]]\ntype = \"yari_ashigaru\"\nteam = \"blue\"\npos = [0, -100]\nfacing = 90\n\
             [[units]]\ntype = \"yari_ashigaru\"\nteam = \"red\"\npos = [130, 130]\nfacing = 270\n",
        );
        b.set_running(0, run);
        b.order(0, Order::Move { to: Vec2::new(0.0, -20.0), facing: None, files: None });
        while b.units[0].order != Order::Hold && b.time < 200.0 {
            b.tick();
        }
        b.time
    };
    let (walk, run) = (time_to_arrive(false), time_to_arrive(true));
    assert!(run < walk * 0.6, "run {run}s vs walk {walk}s");
}

#[test]
fn a_fight_ends_with_someone_routing() {
    let mut b = Battle::new(&scenario("spear_duel"), &types()).unwrap();
    b.ai_teams = vec![Team::Blue, Team::Red];
    let mut routed = false;
    while b.time < 240.0 && b.winner.is_none() {
        b.tick();
        routed |= b.drain_events().iter().any(|e| matches!(e, BattleEvent::UnitRouted(_)));
    }
    assert!(routed, "one side should break");
    assert!(b.winner.is_some());
    // Spear fights shouldn't be over in a blink, or last forever.
    assert!(b.time > 25.0 && b.time < 180.0, "battle took {}s", b.time);
}

/// Cavalry charging spears from the front should come off far worse than
/// cavalry charging the same spears in the rear.
#[test]
fn braced_spears_punish_frontal_cavalry_charges() {
    let cav_losses = |from_behind: bool| {
        let cav_y = if from_behind { -60.0 } else { 60.0 };
        let cav_facing = if from_behind { 90 } else { 270 };
        let mut b = custom(&format!(
            "[[units]]\ntype = \"yari_ashigaru\"\nteam = \"blue\"\npos = [0, 0]\nfacing = 90\n\
             [[units]]\ntype = \"yari_cavalry\"\nteam = \"red\"\npos = [0, {cav_y}]\nfacing = {cav_facing}\n"
        ));
        b.set_running(1, true);
        b.order(1, Order::Attack { target: 0 });
        b.run_for(14.0);
        (40 - b.units[1].alive_count, 120 - b.units[0].alive_count)
    };
    let (front_cav_lost, front_spears_lost) = cav_losses(false);
    let (rear_cav_lost, rear_spears_lost) = cav_losses(true);
    assert!(
        front_cav_lost > rear_cav_lost,
        "frontal charge cav losses {front_cav_lost} vs rear {rear_cav_lost}"
    );
    assert!(
        rear_spears_lost > front_spears_lost,
        "spear losses: rear-charged {rear_spears_lost} vs front {front_spears_lost}"
    );
}

#[test]
fn rear_charge_breaks_an_engaged_unit_faster() {
    let rout_time = |with_cav: bool| {
        let cav = if with_cav {
            "[[units]]\ntype = \"yari_cavalry\"\nteam = \"blue\"\npos = [0, -150]\nfacing = 270\n"
        } else {
            ""
        };
        // Red spears fight blue spears to the north; blue horse waits south of red.
        let mut b = custom(&format!(
            "[[units]]\ntype = \"yari_ashigaru\"\nteam = \"blue\"\npos = [0, 20]\nfacing = 270\n\
             [[units]]\ntype = \"yari_ashigaru\"\nteam = \"red\"\npos = [0, -20]\nfacing = 90\n{cav}"
        ));
        b.order(0, Order::Attack { target: 1 });
        b.order(1, Order::Attack { target: 0 });
        b.run_for(15.0);
        if with_cav {
            b.set_running(2, true);
            b.order(2, Order::Attack { target: 1 });
        }
        while !b.units[1].state.is_broken() && b.time < 300.0 {
            b.tick();
        }
        b.time
    };
    let (alone, flanked) = (rout_time(false), rout_time(true));
    assert!(flanked < alone, "flanked {flanked}s vs straight fight {alone}s");
}

#[test]
fn routing_units_flee_and_can_rally() {
    let mut b = custom(
        "[[units]]\ntype = \"yari_ashigaru\"\nteam = \"blue\"\npos = [0, 0]\nfacing = 90\n\
         [[units]]\ntype = \"yari_ashigaru\"\nteam = \"red\"\npos = [0, 120]\nfacing = 270\n",
    );
    // Scare blue badly without killing many.
    b.units[0].shock = 80.0;
    b.tick();
    assert_eq!(b.units[0].state, UnitState::Routing);
    let start = b.units[0].centre;
    b.run_for(4.0);
    assert!(b.units[0].centre.y < start.y - 8.0, "routers run away from the enemy");
    b.run_for(30.0);
    assert_ne!(b.units[0].state, UnitState::Routing, "should rally once safe and calm");
    assert!(b.drain_events().iter().any(|e| matches!(e, BattleEvent::UnitRallied(0))));
}

#[test]
fn moving_unit_that_bumps_the_enemy_stops_to_fight() {
    let mut b = custom(
        "[[units]]\ntype = \"yari_ashigaru\"\nteam = \"blue\"\npos = [0, 0]\nfacing = 90\n\
         [[units]]\ntype = \"yari_ashigaru\"\nteam = \"red\"\npos = [0, 30]\nfacing = 270\n",
    );
    b.order(0, Order::Move { to: Vec2::new(0.0, 80.0), facing: None, files: None });
    b.run_for(30.0);
    assert_eq!(b.units[0].order, Order::Attack { target: 1 });
}

#[test]
fn unit_at_picks_the_nearest_unit() {
    let b = Battle::new(&scenario("spear_duel"), &types()).unwrap();
    assert_eq!(b.unit_at(Vec2::new(0.0, -40.0), 3.0), Some(0));
    assert_eq!(b.unit_at(Vec2::new(0.0, 40.0), 3.0), Some(1));
    assert_eq!(b.unit_at(Vec2::new(100.0, 0.0), 3.0), None);
}

#[test]
fn performance_budget() {
    // ~720 soldiers should simulate far faster than real time.
    let mut b = Battle::new(&scenario("first_clash"), &types()).unwrap();
    b.ai_teams = vec![Team::Blue, Team::Red];
    let t = std::time::Instant::now();
    b.run_for(30.0);
    let el = t.elapsed().as_secs_f32();
    // Generous so debug builds pass too.
    assert!(el < 30.0, "30s of battle took {el}s");
}

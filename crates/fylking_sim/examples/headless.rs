//! Run a scenario with the AI on both sides and print what happens.
//!
//!     cargo run -p fylking_sim --example headless -- first_clash
//!
//! Handy for balancing unit stats without opening the game.

use fylking_sim::{Battle, BattleEvent, Scenario, Team, UnitTypes};
use std::path::PathBuf;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "first_clash".into());
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/data");
    let types = UnitTypes::from_toml(&std::fs::read_to_string(data.join("units.toml")).unwrap()).unwrap();
    let path = data.join("scenarios").join(format!("{name}.toml"));
    let scenario = Scenario::from_toml(&std::fs::read_to_string(&path).unwrap()).unwrap();

    let mut b = Battle::new(&scenario, &types).unwrap();
    b.ai_teams = vec![Team::Blue, Team::Red];
    println!("== {} ==", scenario.name);

    let start = std::time::Instant::now();
    let mut next_report = 10.0;
    while b.time < 600.0 {
        b.tick();
        for e in b.drain_events() {
            let t = b.time;
            match e {
                BattleEvent::ChargeImpact { unit, target, .. } => {
                    println!("{t:6.1}s  {} charges into {}", label(&b, unit), label(&b, target))
                }
                BattleEvent::UnitRouted(u) => println!("{t:6.1}s  {} ROUTS", label(&b, u)),
                BattleEvent::UnitRallied(u) => println!("{t:6.1}s  {} rallies", label(&b, u)),
                BattleEvent::UnitShattered(u) => println!("{t:6.1}s  {} SHATTERED", label(&b, u)),
                BattleEvent::UnitDestroyed(u) => println!("{t:6.1}s  {} destroyed", label(&b, u)),
                BattleEvent::Victory(team) => println!("{t:6.1}s  VICTORY: {team:?}"),
            }
        }
        if b.time >= next_report {
            next_report += 10.0;
            let line: Vec<String> = b
                .units
                .iter()
                .map(|u| format!("{}:{}/{} m{:.0}", short(&b, u.id), u.alive_count, u.initial_men, u.morale))
                .collect();
            println!("{:6.1}s  {}", b.time, line.join("  "));
        }
        if b.winner.is_some() && b.time > 20.0 {
            // Let the pursuit play out a little, then stop.
            b.run_for(5.0);
            break;
        }
    }
    let ticks = (b.time / fylking_sim::TICK) as f64;
    let el = start.elapsed().as_secs_f64();
    println!("\nFinal:");
    for u in &b.units {
        println!(
            "  {:<22} {:<5?} {:>3}/{:<3} men  kills {:>3}  {:?}",
            u.name, u.team, u.alive_count, u.initial_men, u.kills, u.state
        );
    }
    println!(
        "\n{} soldiers, {:.0} ticks in {:.2}s ({:.0}x real time)",
        b.soldiers.len(),
        ticks,
        el,
        b.time as f64 / el
    );
}

fn label(b: &Battle, u: usize) -> String {
    format!("{} ({:?})", b.units[u].name, b.units[u].team)
}

fn short(b: &Battle, u: usize) -> String {
    let unit = &b.units[u];
    let t = if unit.team == Team::Blue { "B" } else { "R" };
    let initials: String = unit.name.split_whitespace().filter_map(|w| w.chars().next()).collect();
    format!("{t}{u}{initials}")
}

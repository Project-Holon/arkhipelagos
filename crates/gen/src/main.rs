//! arkh-gen: make worlds and check the generator.
//!
//!   arkh-gen world <config.json> [out.json]   make a world; print its hash
//!   arkh-gen hash <config.json>               print the world's hash only
//!   arkh-gen sweep <count>                    many random configurations: every
//!                                             world must generate and be finishable

use std::process::ExitCode;

use arkh_gen::gen::{generate, summary, Config, PlayerConfig};
use arkh_gen::rng::{fnv1a, Rng};
use serde_json::{Map, Value};

fn main() -> ExitCode {
    let a: Vec<String> = std::env::args().collect();
    let r = match a.get(1).map(|s| s.as_str()) {
        Some("world") if a.len() >= 3 => world(&a[2], a.get(3).map(|s| s.as_str()), false),
        Some("hash") if a.len() == 3 => world(&a[2], None, true),
        Some("sweep") if a.len() == 3 => sweep(a[2].parse().unwrap_or(100)),
        Some("spheres") if a.len() == 3 => spheres(&a[2]),
        _ => Err("usage: arkh-gen world <config> [out] | hash <config> | sweep <count>".into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn world(path: &str, out: Option<&str>, hash_only: bool) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let cfg: Config = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let m = arkh_gen::model()?;
    let w = generate(&m, &cfg)?;
    let bytes = serde_json::to_vec(&w).map_err(|e| e.to_string())?;
    let h = fnv1a(&[&bytes]);
    if hash_only {
        println!("{h:016x}");
        return Ok(());
    }
    if let Some(o) = out {
        std::fs::write(o, serde_json::to_vec_pretty(&w).unwrap()).map_err(|e| format!("{o}: {e}"))?;
    }
    println!("world {h:016x}: {:?}", summary(&w));
    Ok(())
}

fn spheres(path: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let cfg: Config = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let m = arkh_gen::model()?;
    let w = generate(&m, &cfg)?;
    let s = arkh_gen::gen::spheres(&m, &cfg, &w)?;
    let total: usize = w.players.iter().map(|p| p.locations.len()).sum();
    println!("locations reachable after each sphere (of {total}): {s:?}");
    Ok(())
}

fn sweep(count: u64) -> Result<(), String> {
    let m = arkh_gen::model()?;
    let mut rng = Rng::new(0x5EED);
    let mut ok = 0;
    let mut fails = Vec::new();
    for k in 0..count {
        let players = 1 + rng.below(6) as usize;
        let goal = if rng.below(2) == 0 { "ghetsis" } else { "champion" };
        let seasons = ["vanilla", "changeable"][rng.below(2) as usize];
        let mut opts = Map::new();
        opts.insert("goal".into(), Value::String(goal.into()));
        opts.insert("season_control".into(), Value::String(seasons.into()));
        let ps = (0..players)
            .map(|i| {
                let mut po = Map::new();
                if rng.below(3) == 0 {
                    po.insert("modify_logic".into(), Value::Array(vec![Value::String("require_flash".into())]));
                }
                PlayerConfig {
                    name: format!("P{}", i + 1),
                    version: ["black", "white"][rng.below(2) as usize].into(),
                    options: po,
                }
            })
            .collect();
        let cfg = Config { schema: 1, seed: rng.next_u64(), options: opts, players: ps };
        match generate(&m, &cfg) {
            Ok(_) => ok += 1,
            Err(e) => fails.push(format!("#{k} ({players} players, {goal}, {seasons}): {e}")),
        }
    }
    println!("sweep: {ok} of {count} worlds made and finishable");
    for f in fails.iter().take(10) {
        println!("   {f}");
    }
    if fails.is_empty() {
        Ok(())
    } else {
        Err(format!("{} failures", fails.len()))
    }
}

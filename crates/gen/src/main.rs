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
        Some("ap-static") if a.len() == 3 => ap_static(&a[2]),
        Some("ap-pack") if a.len() == 4 => ap_pack(&a[2], &a[3]),
        Some("ap-reach") if a.len() == 5 => ap_reach(&a[2], &a[3], &a[4]),
        _ => Err("usage: arkh-gen world <config> [out] | hash <config> | sweep <count> | ap-static <out> | ap-pack <config or -> <player> | ap-reach <config> <player> <items.json>".into()),
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

/// Read a configuration from a file, or from standard input for "-".
fn read_config(path: &str) -> Result<Config, String> {
    let text = if path == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).map_err(|e| e.to_string())?;
        s
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?
    };
    serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
}

/// The Archipelago world's static file: every item and location number.
fn ap_static(out: &str) -> Result<(), String> {
    let m = arkh_gen::model()?;
    let v = arkh_gen::gen::ap::static_json(&m);
    std::fs::write(out, serde_json::to_vec(&v).unwrap()).map_err(|e| format!("{out}: {e}"))
}

/// One player's pack for the Archipelago world, on standard output.
fn ap_pack(path: &str, player: &str) -> Result<(), String> {
    let cfg = read_config(path)?;
    let m = arkh_gen::model()?;
    let i: usize = player.parse().map_err(|_| "player is a number".to_string())?;
    let v = arkh_gen::gen::ap::pack_json(&m, &cfg, i)?;
    println!("{}", serde_json::to_string(&v).unwrap());
    Ok(())
}

/// The locations one player reaches with the items in a JSON list of ids.
fn ap_reach(path: &str, player: &str, items: &str) -> Result<(), String> {
    let cfg = read_config(path)?;
    let m = arkh_gen::model()?;
    let i: usize = player.parse().map_err(|_| "player is a number".to_string())?;
    let text = std::fs::read_to_string(items).map_err(|e| format!("{items}: {e}"))?;
    let list: Vec<u32> = serde_json::from_str(&text).map_err(|e| format!("{items}: {e}"))?;
    let v = arkh_gen::gen::ap::reach_json(&m, &cfg, i, &list)?;
    println!("{}", serde_json::to_string(&v).unwrap());
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

/// Every randomizer option with its modifiers (the first one turns it on).
const RANDOMIZERS: &[(&str, &[&str])] = &[
    ("randomize_wild_pokemon", &["randomize", "ensure_all", "similar_stats", "type_themed_areas"]),
    ("randomize_static_pokemon", &["randomize", "similar_stats", "no_legendaries"]),
    ("randomize_gift_pokemon", &["randomize", "similar_stats", "no_legendaries", "split_monkeys"]),
    ("randomize_legendary_pokemon", &["randomize", "keep_legendary"]),
    ("randomize_trade_pokemon", &["randomize_offer", "randomize_request"]),
    ("randomize_starter_pokemon", &["randomize", "any_base", "similar_stats", "type_variety"]),
    ("randomize_trainer_pokemon", &["randomize", "similar_stats", "type_themed", "shuffle_gym_types", "rivals_keep_starter"]),
    ("randomize_evolutions", &["randomize", "follow_type"]),
    ("replace_evo_methods", &["trade"]),
    ("randomize_tm_hm_compatibility", &["randomize", "all_hms", "all_tms", "follow_evolutions"]),
    ("randomize_types", &["shuffle", "follow_evolutions"]),
    ("randomize_base_stats", &["randomize", "follow_evolutions", "random_total"]),
    ("randomize_catch_rates", &["shuffle"]),
    ("randomize_gender_ratio", &["randomize"]),
    ("randomize_abilities", &["randomize"]),
    ("randomize_held_items", &["randomize"]),
    ("randomize_levelup_movesets", &["randomize", "match_types"]),
    ("randomize_move_data", &["power", "type", "accuracy", "pp"]),
    ("randomize_type_chart", &["shuffle"]),
];

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
                // half the games random: each randomizer on in about half of
                // those, with random modifiers
                if k % 2 == 1 {
                    for (opt, mods) in RANDOMIZERS {
                        if rng.below(2) == 0 {
                            let mut v = vec![Value::String(mods[0].into())];
                            for md in &mods[1..] {
                                if rng.below(2) == 0 {
                                    v.push(Value::String((*md).into()));
                                }
                            }
                            po.insert((*opt).into(), Value::Array(v));
                        }
                    }
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

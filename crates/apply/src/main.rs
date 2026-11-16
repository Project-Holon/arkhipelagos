//! arkh-apply: write one player's changed archives, built on a cartridge's
//! own files (a test of the apply step without eidos).
//!
//!   arkh-apply <world.json> <slot> <cartridge data folder> <out folder> [eu|ja]
//!   arkh-apply scripts <eu|ja> <cartridge data folder> <out folder>
//!
//! `scripts` writes <out>/a/0/5/7 with the check edits (scripts.rs), each
//! edited member checked: original bytes kept outside the edits, and the
//! member reads back the same.
//!
//! Writes <out>/a/0/1/6 and the other touched archives as whole NARCs, and
//! <out>/type_chart.bin when the chart changed, and <out>/a/0/5/7 with the
//! check edits and the world's species (family eu unless given).

use std::process::ExitCode;

use arkh_formats::narc::Narc;
use arkh_gen::gen::World;

fn main() -> ExitCode {
    let a: Vec<String> = std::env::args().collect();
    if a.len() == 5 && a[1] == "scripts" {
        return match scripts(&a[2], &a[3], &a[4]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    if a.len() != 5 && a.len() != 6 {
        eprintln!("usage: arkh-apply <world.json> <slot> <cartridge data folder> <out folder>");
        return ExitCode::FAILURE;
    }
    match run(&a[1], &a[2], &a[3], &a[4], a.get(5).map(|s| s.as_str()).unwrap_or("eu")) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run(world: &str, slot: &str, data: &str, out: &str, fam: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(world).map_err(|e| format!("{world}: {e}"))?;
    let w: World = serde_json::from_str(&text).map_err(|e| format!("{world}: {e}"))?;
    let slot: u32 = slot.parse().map_err(|_| "slot: a number".to_string())?;
    let p = w.players.iter().find(|p| p.slot == slot).ok_or("no such slot")?;
    let mut touched = 0;
    for arc in arkh_apply::ARCHIVES {
        let path = format!("{data}/{arc}");
        let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
        let mut narc = Narc::parse(&bytes).map_err(|e| format!("{path}: {}", e.0))?;
        let files = narc.files.clone();
        let changed = arkh_apply::members(&p.tables, arc, &|i| files.get(i as usize).cloned())?;
        if changed.is_empty() {
            continue;
        }
        for (i, d) in &changed {
            narc.files[*i as usize] = d.clone();
        }
        let dst = format!("{out}/{arc}");
        if let Some(dir) = std::path::Path::new(&dst).parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{dst}: {e}"))?;
        }
        std::fs::write(&dst, narc.pack()).map_err(|e| format!("{dst}: {e}"))?;
        println!("{arc}: {} members", changed.len());
        touched += 1;
    }
    {
        let family = arkh_script::Family::parse(fam).ok_or("family: eu or ja")?;
        let arc = arkh_apply::scripts::SCRIPTS;
        let path = format!("{data}/{arc}");
        let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
        let mut narc = Narc::parse(&bytes).map_err(|e| format!("{path}: {}", e.0))?;
        let files = narc.files.clone();
        let ws = arkh_apply::scripts::WorldSpecies { version: &p.version, changes: &p.tables, starters: &p.starters, picks: &p.picks };
        let changed = arkh_apply::scripts::members_for(family, Some(&ws), &|i| files.get(i as usize).cloned())?;
        for (i, d) in &changed {
            narc.files[*i as usize] = d.clone();
        }
        let dst = format!("{out}/{arc}");
        if let Some(dir) = std::path::Path::new(&dst).parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{dst}: {e}"))?;
        }
        std::fs::write(&dst, narc.pack()).map_err(|e| format!("{dst}: {e}"))?;
        println!("{arc}: {} members", changed.len());
        touched += 1;
    }
    if let Some(chart) = arkh_apply::type_chart(&p.tables) {
        std::fs::create_dir_all(out).map_err(|e| format!("{out}: {e}"))?;
        std::fs::write(format!("{out}/type_chart.bin"), chart).map_err(|e| e.to_string())?;
        println!("type chart");
    }
    println!("{touched} archives written");
    Ok(())
}

fn scripts(fam: &str, data: &str, out: &str) -> Result<(), String> {
    let family = arkh_script::Family::parse(fam).ok_or("family: eu or ja")?;
    let arc = arkh_apply::scripts::SCRIPTS;
    let path = format!("{data}/{arc}");
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let mut narc = Narc::parse(&bytes).map_err(|e| format!("{path}: {}", e.0))?;
    let files = narc.files.clone();
    let changed = arkh_apply::scripts::members(family, &|i| files.get(i as usize).cloned())?;
    let mut grown = 0;
    for (i, d) in &changed {
        grown += d.len() - files[*i as usize].len();
        narc.files[*i as usize] = d.clone();
    }
    let dst = format!("{out}/{arc}");
    if let Some(dir) = std::path::Path::new(&dst).parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{dst}: {e}"))?;
    }
    std::fs::write(&dst, narc.pack()).map_err(|e| format!("{dst}: {e}"))?;
    println!("{arc}: {} members edited, {grown} bytes added", changed.len());
    Ok(())
}

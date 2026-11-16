//! arkh-script: field-script tool.
//!
//!   arkh-script check <eu|ja> <a/0/5/7 archive>      every member, every check
//!   arkh-script dis <eu|ja> <archive> <index>         listing of one member
//!   arkh-script dis-file <eu|ja> <member file>        listing of a loose member
//!   arkh-script asm <eu|ja> <listing> <out>           assemble a listing
//!
//! Checks, per member:
//! - script: bytes -> script -> bytes is identical; bytes -> text -> script
//!   -> bytes is identical; adding an entry and a new script at the end
//!   leaves every existing item as it was (the entry table grows, all else
//!   shifts by 4);
//! - zone table: bytes -> zone -> bytes and through text, identical;
//! - empty and other members are kept as they are;
//! - the archive repacks byte for byte.

use std::process::ExitCode;

use arkh_formats::narc::Narc;
use arkh_script::script::{self, shape, Item, Kind, Script};
use arkh_script::zone::Zone;
use arkh_script::{Family, Table};

fn main() -> ExitCode {
    let a: Vec<String> = std::env::args().collect();
    let res = match a.get(1).map(|s| s.as_str()) {
        Some("check") if a.len() == 4 => check(&a[2], &a[3]),
        Some("dis") if a.len() == 5 => dis(&a[2], &a[3], &a[4]),
        Some("dis-file") if a.len() == 4 => dis_file(&a[2], &a[3]),
        Some("asm") if a.len() == 5 => asm(&a[2], &a[3], &a[4]),
        Some("port") if a.len() == 6 => port(&a[2], &a[3], &a[4], &a[5]),
        _ => Err("usage: arkh-script check|dis|asm ...".to_string()),
    };
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn family(s: &str) -> Result<Family, String> {
    Family::parse(s).ok_or_else(|| format!("unknown family {s}"))
}

fn load(path: &str) -> Result<(Vec<u8>, Narc), String> {
    let raw = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let n = Narc::parse(&raw).map_err(|e| format!("{path}: {e}"))?;
    Ok((raw, n))
}

fn dis(fam: &str, path: &str, idx: &str) -> Result<(), String> {
    let f = family(fam)?;
    let (_, n) = load(path)?;
    let i: usize = idx.parse().map_err(|_| "bad index".to_string())?;
    let d = n.files.get(i).ok_or("no such member")?;
    dis_bytes(f, d)
}

fn dis_file(fam: &str, path: &str) -> Result<(), String> {
    let d = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    dis_bytes(family(fam)?, &d)
}

fn dis_bytes(f: Family, d: &[u8]) -> Result<(), String> {
    match script::classify(d) {
        Kind::Script => print!("{}", script::to_text(&script::disassemble(d, f).map_err(|e| e.0)?)),
        Kind::Zone => print!("{}", Zone::parse(d).unwrap().to_text()),
        k => println!("; {k:?} member, {} bytes", d.len()),
    }
    Ok(())
}

fn asm(fam: &str, src: &str, out: &str) -> Result<(), String> {
    let f = family(fam)?;
    let text = std::fs::read_to_string(src).map_err(|e| format!("{src}: {e}"))?;
    let bytes = if text.trim_start().starts_with(".zone") {
        Zone::from_text(&text).and_then(|z| z.assemble())
    } else {
        script::from_text(&text).and_then(|s| script::assemble(&s, f))
    }
    .map_err(|e| e.0)?;
    std::fs::write(out, bytes).map_err(|e| format!("{out}: {e}"))
}

/// Take each script of one cartridge, assemble it for another family, and
/// compare with that cartridge's own member. Equal bytes: the commands map
/// across languages and nothing else differs. Same commands, other
/// numbers: the languages differ only in operands (message numbers).
fn port(from_fam: &str, from: &str, to_fam: &str, to: &str) -> Result<(), String> {
    let (ff, tf) = (family(from_fam)?, family(to_fam)?);
    let (_, a) = load(from)?;
    let (_, b) = load(to)?;
    let mut same = 0;
    let mut same_cmds = 0;
    let mut other = 0;
    let mut missing = std::collections::BTreeMap::<String, usize>::new();
    for (i, d) in a.files.iter().enumerate() {
        if script::classify(d) != Kind::Script {
            continue;
        }
        let s = script::disassemble(d, ff).map_err(|e| e.0)?;
        match script::assemble(&s, tf) {
            Ok(bytes) if bytes == b.files[i] => same += 1,
            Ok(_) => {
                let t = script::disassemble(&b.files[i], tf).map_err(|e| e.0)?;
                let names = |s: &Script| {
                    s.items
                        .iter()
                        .filter_map(|it| if let Item::Cmd { name, .. } = it { Some(name.clone()) } else { None })
                        .collect::<Vec<_>>()
                };
                if names(&s) == names(&t) {
                    same_cmds += 1
                } else {
                    other += 1
                }
            }
            Err(e) => *missing.entry(e.0).or_default() += 1,
        }
    }
    println!("identical bytes {same}; same commands, other operands {same_cmds}; other differences {other}");
    for (e, n) in missing {
        println!("   cannot assemble ({n} members): {e}");
    }
    Ok(())
}

fn grow(s: &Script, f: Family) -> Result<(), String> {
    let end = "SCRIPT_WORK_CmdEnd";
    if Table::get(f).by_name(end).is_none() {
        return Err(format!("{end} missing from the table"));
    }
    let mut s2 = s.clone();
    s2.entries.push("ARKH_NEW".into());
    s2.items.push(Item::Label("ARKH_NEW".into()));
    s2.items.push(Item::Cmd { name: end.into(), args: vec![] });
    let bytes = script::assemble(&s2, f).map_err(|e| e.0)?;
    let back = script::disassemble(&bytes, f).map_err(|e| format!("grown script does not decode: {}", e.0))?;
    if shape(&back) != shape(&s2) {
        let a = shape(&back);
        let b = shape(&s2);
        let first = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
        return Err(format!(
            "grown script differs at item {first}: {:?} vs {:?}",
            a.get(first),
            b.get(first)
        ));
    }
    Ok(())
}

fn check(fam: &str, path: &str) -> Result<(), String> {
    let f = family(fam)?;
    let (raw, n) = load(path)?;
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    let mut failures = Vec::new();
    for (i, d) in n.files.iter().enumerate() {
        let r: Result<&str, String> = (|| match script::classify(d) {
            Kind::Script => {
                let s = script::disassemble(d, f).map_err(|e| format!("disassemble: {}", e.0))?;
                if script::assemble(&s, f).map_err(|e| e.0)? != *d {
                    return Err("binary round trip differs".into());
                }
                let t = script::from_text(&script::to_text(&s)).map_err(|e| e.0)?;
                if script::assemble(&t, f).map_err(|e| e.0)? != *d {
                    return Err("text round trip differs".into());
                }
                grow(&s, f)?;
                Ok("script")
            }
            Kind::Zone => {
                let z = Zone::parse(d).ok_or("zone parse")?;
                if z.assemble().map_err(|e| e.0)? != *d {
                    return Err("zone round trip differs".into());
                }
                let z2 = Zone::from_text(&z.to_text()).map_err(|e| e.0)?;
                if z2.assemble().map_err(|e| e.0)? != *d {
                    return Err("zone text round trip differs".into());
                }
                Ok("zone")
            }
            Kind::Empty => Ok("empty"),
            Kind::Other => Ok("other (kept as is)"),
        })();
        match r {
            Ok(k) => *counts.entry(k).or_default() += 1,
            Err(e) => failures.push(format!("member {i}: {e}")),
        }
    }
    let repack = n.pack() == raw;
    println!(
        "{path}: {} members {:?}; failures {}; archive repacks identically: {repack}",
        n.files.len(),
        counts,
        failures.len()
    );
    for fl in failures.iter().take(10) {
        println!("   {fl}");
    }
    if failures.is_empty() && repack {
        Ok(())
    } else {
        Err("check failed".into())
    }
}

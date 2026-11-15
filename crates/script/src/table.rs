//! The field-script command table, per language family.
//!
//! The table is `data/script/commands.tsv`, derived from aletheia's source
//! (each handler's reads from the script, with the language switches
//! applied) and checked against every script of the 12 cartridges. It is
//! compiled in, so the tool and the mod carry it.
//!
//! Families: `Eu` is the ten European cartridges (English, French, German,
//! Italian, Spanish; Black and White), `Ja` the two Japanese ones. Japanese
//! has 605 commands, the others 609: four English word-buffer commands have
//! no Japanese twin, so opcodes from 0x4D on are numbered differently.

use std::collections::HashMap;
use std::sync::OnceLock;

const TSV: &str = include_str!("../../../data/script/commands.tsv");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Eu,
    Ja,
}

impl Family {
    pub fn name(self) -> &'static str {
        match self {
            Family::Eu => "eu",
            Family::Ja => "ja",
        }
    }

    pub fn parse(s: &str) -> Option<Family> {
        match s {
            "eu" => Some(Family::Eu),
            "ja" => Some(Family::Ja),
            _ => None,
        }
    }

    /// The family of a cartridge game code (IRBO, IRAJ, ...).
    pub fn of_game_code(code: &str) -> Family {
        if code.ends_with('J') {
            Family::Ja
        } else {
            Family::Eu
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    U8,
    U16,
    U32,
    /// u16: a work variable or a constant
    Wv,
    /// u16: a work variable
    Wp,
    /// u32 relative offset to code
    Code,
    /// u32 relative offset to a movement list
    Movement,
    /// u32 relative offset to a warp list
    Warp,
}

impl ArgKind {
    pub fn size(self) -> usize {
        match self {
            ArgKind::U8 => 1,
            ArgKind::U16 | ArgKind::Wv | ArgKind::Wp => 2,
            ArgKind::U32 | ArgKind::Code | ArgKind::Movement | ArgKind::Warp => 4,
        }
    }

    pub fn is_target(self) -> bool {
        matches!(self, ArgKind::Code | ArgKind::Movement | ArgKind::Warp)
    }

    fn parse(s: &str) -> Option<ArgKind> {
        Some(match s {
            "u8" => ArgKind::U8,
            "u16" => ArgKind::U16,
            "u32" => ArgKind::U32,
            "wv" => ArgKind::Wv,
            "wp" => ArgKind::Wp,
            "code" => ArgKind::Code,
            "movement" => ArgKind::Movement,
            "warp" => ArgKind::Warp,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Cmd {
    pub op: u16,
    pub name: String,
    pub args: Vec<ArgKind>,
    /// The command ends the flow: nothing after it runs (End, Return, Jump).
    pub ends: bool,
}

impl Cmd {
    pub fn size(&self) -> usize {
        2 + self.args.iter().map(|a| a.size()).sum::<usize>()
    }
}

pub struct Table {
    pub family: Family,
    by_op: HashMap<u16, Cmd>,
    by_name: HashMap<String, u16>,
}

impl Table {
    pub fn get(family: Family) -> &'static Table {
        static EU: OnceLock<Table> = OnceLock::new();
        static JA: OnceLock<Table> = OnceLock::new();
        match family {
            Family::Eu => EU.get_or_init(|| Table::load(Family::Eu)),
            Family::Ja => JA.get_or_init(|| Table::load(Family::Ja)),
        }
    }

    fn load(family: Family) -> Table {
        let mut by_op = HashMap::new();
        let mut by_name = HashMap::new();
        let mut rows = Vec::new();
        let mut seen: HashMap<String, usize> = HashMap::new();
        for line in TSV.lines().skip(1) {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 5 || f[0] != family.name() {
                continue;
            }
            let op = u16::from_str_radix(f[1].trim_start_matches("0x"), 16).expect("table: opcode");
            let args = if f[3].is_empty() {
                Vec::new()
            } else {
                f[3].split(',').map(|a| ArgKind::parse(a).expect("table: argument kind")).collect()
            };
            *seen.entry(f[2].to_string()).or_default() += 1;
            rows.push(Cmd { op, name: f[2].to_string(), args, ends: f[4] == "1" });
        }
        for mut cmd in rows {
            // one handler behind two opcodes: the name alone would not say
            // which opcode to write back, so these names carry the opcode
            if seen[&cmd.name] > 1 {
                cmd.name = format!("{}#{:#05x}", cmd.name, cmd.op);
            }
            by_name.insert(cmd.name.clone(), cmd.op);
            by_op.insert(cmd.op, cmd);
        }
        Table { family, by_op, by_name }
    }

    pub fn by_op(&self, op: u16) -> Option<&Cmd> {
        self.by_op.get(&op)
    }

    pub fn by_name(&self, name: &str) -> Option<&Cmd> {
        self.by_name.get(name).and_then(|op| self.by_op.get(op))
    }

    pub fn len(&self) -> usize {
        self.by_op.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_op.is_empty()
    }
}

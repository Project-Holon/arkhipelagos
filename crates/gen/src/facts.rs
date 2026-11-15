//! The original game's tables (data/facts, read from the cartridges by
//! tools/facts.py), compiled in. Numbers only; keys are game numbers.

use std::collections::BTreeMap;

use serde::Deserialize;

pub const SPECIES: &str = include_str!("../../../data/facts/species.json");
pub const LEARNSETS: &str = include_str!("../../../data/facts/learnsets.json");
pub const EVOLUTIONS: &str = include_str!("../../../data/facts/evolutions.json");
pub const MOVES: &str = include_str!("../../../data/facts/moves.json");
pub const MACHINES: &str = include_str!("../../../data/facts/machines.json");
pub const TYPE_CHART: &str = include_str!("../../../data/facts/type_chart.json");
pub const ENCOUNTERS: &str = include_str!("../../../data/facts/encounters.json");
pub const TRAINERS: &str = include_str!("../../../data/facts/trainers.json");

pub const ALL: [&str; 8] = [SPECIES, LEARNSETS, EVOLUTIONS, MOVES, MACHINES, TYPE_CHART, ENCOUNTERS, TRAINERS];

/// The last species number (Genesect).
pub const LAST_DEX: u32 = 649;

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Personal {
    /// HP, Attack, Defense, Speed, Sp. Atk, Sp. Def
    pub stats: [u8; 6],
    pub types: [u8; 2],
    pub catch_rate: u8,
    pub items: [u16; 3],
    pub gender: u8,
    pub hatch: u8,
    pub friendship: u8,
    pub growth: u8,
    pub egg_groups: [u8; 2],
    pub abilities: [u8; 3],
    pub form_base: u16,
    pub forms: u8,
    /// machine compatibility bits that are set (0..=100)
    pub machines: Vec<u8>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Move {
    #[serde(rename = "type")]
    pub kind: u8,
    /// the game's "category": the effect group
    pub group: u8,
    /// 0 status, 1 physical, 2 special
    pub damage_type: u8,
    pub power: u8,
    pub accuracy: u8,
    pub pp: u8,
    pub priority: i8,
    /// the ailment the move can cause (sickId)
    pub ailment: u16,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Machine {
    pub name: String,
    pub item: u32,
    #[serde(rename = "move")]
    pub mv: u32,
    pub compat_bit: u8,
}

#[derive(Deserialize, Clone, Debug)]
pub struct EncounterBlock {
    pub rates: BTreeMap<String, u8>,
    /// per method: [species, form, min level, max level] per slot
    pub slots: BTreeMap<String, Vec<[u16; 4]>>,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TrainerMon {
    pub species: u16,
    pub form: u16,
    pub level: u16,
    pub dv: u8,
    pub ability: u8,
    #[serde(default)]
    pub item: Option<u16>,
    #[serde(default)]
    pub moves: Option<[u16; 4]>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Trainer {
    pub class: u8,
    pub party: Vec<TrainerMon>,
}

#[derive(Deserialize)]
struct All<T> {
    all: T,
}
#[derive(Deserialize)]
struct PerVersion<T> {
    black: T,
    white: T,
}

pub struct Facts {
    /// by personal record number (index 0 unused)
    pub species: Vec<Personal>,
    pub learnsets: Vec<Vec<[u16; 2]>>,
    /// [method, parameter, target species] per record
    pub evolutions: Vec<Vec<[u16; 3]>>,
    /// by move number (index 0 unused)
    pub moves: Vec<Option<Move>>,
    pub machines: Vec<Machine>,
    pub type_chart: Vec<Vec<u8>>,
    /// [black, white]: per encounter file, the season blocks (1 or 4)
    pub encounters: [Vec<Vec<EncounterBlock>>; 2],
    /// by trainer number (index 0 unused)
    pub trainers: Vec<Option<Trainer>>,
    /// (species, form) pairs: the mon number is the index
    pub mons: Vec<(u16, u8)>,
    /// first mon number of each species (index by species number)
    pub mon_base: Vec<u32>,
}

fn by_key<T: Clone>(m: BTreeMap<String, T>, what: &str) -> Result<Vec<Option<T>>, String> {
    let mut v: Vec<Option<T>> = Vec::new();
    for (k, x) in m {
        let i: usize = k.parse().map_err(|_| format!("{what}: bad key {k}"))?;
        if v.len() <= i {
            v.resize(i + 1, None);
        }
        v[i] = Some(x);
    }
    Ok(v)
}

impl Facts {
    pub fn load() -> Result<Facts, String> {
        let p = |what: &str, e: serde_json::Error| format!("facts {what}: {e}");
        let sp: All<BTreeMap<String, Personal>> = serde_json::from_str(SPECIES).map_err(|e| p("species", e))?;
        let ls: All<BTreeMap<String, Vec<[u16; 2]>>> = serde_json::from_str(LEARNSETS).map_err(|e| p("learnsets", e))?;
        let ev: All<BTreeMap<String, Vec<[u16; 3]>>> = serde_json::from_str(EVOLUTIONS).map_err(|e| p("evolutions", e))?;
        let mv: All<BTreeMap<String, Move>> = serde_json::from_str(MOVES).map_err(|e| p("moves", e))?;
        let ma: All<Vec<Machine>> = serde_json::from_str(MACHINES).map_err(|e| p("machines", e))?;
        let tc: All<Vec<Vec<u8>>> = serde_json::from_str(TYPE_CHART).map_err(|e| p("type chart", e))?;
        let en: PerVersion<BTreeMap<String, Vec<EncounterBlock>>> =
            serde_json::from_str(ENCOUNTERS).map_err(|e| p("encounters", e))?;
        let tr: All<BTreeMap<String, Trainer>> = serde_json::from_str(TRAINERS).map_err(|e| p("trainers", e))?;

        let mut species: Vec<Option<Personal>> = by_key(sp.all, "species")?;
        // record 0 is not in the file (the game's empty record): a stand-in
        species[0] = species.get(1).cloned().flatten();
        let species: Vec<Personal> =
            species.into_iter().map(|x| x.ok_or("facts species: a gap")).collect::<Result<Vec<_>, _>>()?;
        let n = species.len();
        let learnsets = by_key(ls.all, "learnsets")?.into_iter().map(|x| x.unwrap_or_default()).collect();
        let mut evolutions: Vec<Vec<[u16; 3]>> =
            by_key(ev.all, "evolutions")?.into_iter().map(|x| x.unwrap_or_default()).collect();
        evolutions.resize(n, Vec::new());
        let moves = by_key(mv.all, "moves")?;
        let trainers = by_key(tr.all, "trainers")?;
        let enc = |m: BTreeMap<String, Vec<EncounterBlock>>| -> Result<Vec<Vec<EncounterBlock>>, String> {
            Ok(by_key(m, "encounters")?.into_iter().map(|x| x.unwrap_or_default()).collect())
        };
        let encounters = [enc(en.black)?, enc(en.white)?];

        let mut mons = Vec::new();
        let mut mon_base = vec![0u32; LAST_DEX as usize + 1];
        for dex in 1..=LAST_DEX {
            mon_base[dex as usize] = mons.len() as u32;
            let forms = species[dex as usize].forms.max(1);
            for f in 0..forms {
                mons.push((dex as u16, f));
            }
        }
        Ok(Facts {
            species,
            learnsets,
            evolutions,
            moves,
            machines: ma.all,
            type_chart: tc.all,
            encounters,
            trainers,
            mons,
            mon_base,
        })
    }

    /// The mon number of (species, form); None when the species has no such form.
    pub fn mon(&self, dex: u32, form: u32) -> Option<u32> {
        if dex == 0 || dex > LAST_DEX {
            return None;
        }
        let forms = self.species[dex as usize].forms.max(1) as u32;
        if form >= forms {
            return None;
        }
        Some(self.mon_base[dex as usize] + form)
    }

    /// The personal record of a mon: a form with no record of its own uses
    /// its species' record.
    pub fn record(&self, mon: u32) -> usize {
        let (dex, form) = self.mons[mon as usize];
        let base = self.species[dex as usize].form_base as usize;
        if form == 0 || base == 0 {
            dex as usize
        } else {
            base + form as usize - 1
        }
    }

    pub fn machine(&self, name: &str) -> Option<&Machine> {
        self.machines.iter().find(|m| m.name == name)
    }
}

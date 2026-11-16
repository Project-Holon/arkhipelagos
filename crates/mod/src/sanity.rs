//! The extra checks in the game (docs/generator.md, `crates/gen/src/sanity.rs`):
//!
//! - dexsanity: `dex.register` with `caught` 1 for a species with a check;
//! - seensanity: `dex.register` for a species with a check (caught counts
//!   as seen);
//! - dexcountsanity: after a species is caught, every count check up to the
//!   number of species caught (read with `dex_caught`, so species caught
//!   before the mod ran count too);
//! - trainersanity: `battle.end` won against a trainer with a check.
//!
//! A location is a check only when it is one of this player's locations.

use crate::{Game, Host};

pub const DEX_BASE: u32 = 0x20000;
pub const SEEN_BASE: u32 = 0x21000;
pub const DEXCOUNT_BASE: u32 = 0x22000;
pub const TRAINER_BASE: u32 = 0x23000;
pub const LAST_DEX: u32 = 649;
/// `battle.end` outcome: won
pub const OUTCOME_WIN: u8 = 0;

/// The location ids a `dex.register` can check; `caught_count` is the
/// number of species caught (only read for a catch).
pub fn dex_locations(species: u16, caught: bool, caught_count: u32) -> Vec<u32> {
    let s = u32::from(species);
    if s == 0 || s > LAST_DEX {
        return Vec::new();
    }
    let mut v = vec![SEEN_BASE + s];
    if caught {
        v.push(DEX_BASE + s);
        v.extend((1..=caught_count.min(LAST_DEX)).map(|c| DEXCOUNT_BASE + c));
    }
    v
}

/// The location ids a `battle.end` can check.
pub fn battle_locations(outcome: u8, trainers: [u16; 2]) -> Vec<u32> {
    if outcome != OUTCOME_WIN {
        return Vec::new();
    }
    trainers.iter().filter(|t| **t != 0).map(|t| TRAINER_BASE + u32::from(*t)).collect()
}

/// Species caught, by the game's own dex.
pub fn caught_count(host: &mut dyn Host) -> u32 {
    (1..=LAST_DEX as u16).filter(|s| host.dex_caught(*s) == Some(true)).count() as u32
}

impl Game {
    /// The `dex.register` handler.
    pub fn on_dex_register(&mut self, host: &mut dyn Host, species: u16, caught: bool) {
        let count = if caught { caught_count(host) } else { 0 };
        let locs = dex_locations(species, caught, count);
        self.extra_checks(host, locs);
    }

    /// The `battle.end` handler.
    pub fn on_battle_end(&mut self, host: &mut dyn Host, outcome: u8, trainers: [u16; 2]) {
        let locs = battle_locations(outcome, trainers);
        self.extra_checks(host, locs);
    }

    fn extra_checks(&mut self, host: &mut dyn Host, locs: Vec<u32>) {
        let mine: Vec<u32> = locs
            .into_iter()
            .filter(|l| self.placed.contains_key(l) && !self.state.checked.contains(l))
            .collect();
        if !mine.is_empty() {
            host.log(&format!("arkhipelagos: extra checks {mine:?}"));
            self.award(host, mine);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dex_and_battle_locations() {
        assert_eq!(dex_locations(25, false, 0), vec![SEEN_BASE + 25]);
        assert_eq!(
            dex_locations(25, true, 3),
            vec![SEEN_BASE + 25, DEX_BASE + 25, DEXCOUNT_BASE + 1, DEXCOUNT_BASE + 2, DEXCOUNT_BASE + 3]
        );
        assert!(dex_locations(0, true, 1).is_empty());
        assert!(dex_locations(650, true, 1).is_empty());
        assert_eq!(battle_locations(OUTCOME_WIN, [12, 0]), vec![TRAINER_BASE + 12]);
        assert_eq!(battle_locations(OUTCOME_WIN, [12, 13]), vec![TRAINER_BASE + 12, TRAINER_BASE + 13]);
        assert!(battle_locations(1, [12, 0]).is_empty());
    }
}

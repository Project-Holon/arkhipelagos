//! Which items badge and TM rewards may hold (PokemonBWAP's options):
//!
//! - `shuffle_badge_rewards`: `vanilla` (each gym leader gives their own
//!   badge), `shuffle` (the gym leaders give the player's eight badges in
//!   any order), `anything` (the default here: any item anywhere).
//! - `shuffle_tm_rewards`: `shuffle` (the 35 TM rewards give a TM or HM of
//!   the same player), `hm_with_badge` (the same, and the HMs and TM70
//!   Flash are among the nine TM rewards of gym leaders and Clay on Route
//!   6), `anything` (the default here).
//!
//! With the defaults nothing is restricted and the fill is as before.

use crate::model::Model;

use super::PlayerWorld;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Badge {
    Anything,
    Shuffle,
    Vanilla,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tm {
    Anything,
    Shuffle,
    HmWithBadge,
}

/// The rules of every player of a world.
pub(super) struct Rewards {
    badge: Vec<Badge>,
    tm: Vec<Tm>,
}

fn badge_of(m: &Model, l: usize) -> Option<u32> {
    let loc = &m.locs[l];
    // a badge reward's event flag is 370 + the badge's bit
    (loc.kind == "badge").then(|| 0x1000 + ((loc.id - 0x10000) >> 4) - 370)
}

fn is_badge(m: &Model, i: u32) -> bool {
    m.items[i as usize].group == "badge"
}

fn is_tm_hm(m: &Model, i: u32) -> bool {
    matches!(m.items[i as usize].group.as_str(), "tm" | "hm")
}

fn is_hm_or_flash(m: &Model, i: u32) -> bool {
    let it = &m.items[i as usize];
    it.group == "hm" || it.name.starts_with("TM70")
}

/// A gym leader's TM reward, or Clay's on Route 6 (they share a badge-like
/// flag: the location number ends in 8).
fn is_badge_tm(m: &Model, l: usize) -> bool {
    m.locs[l].kind == "tm" && m.locs[l].id & 0xF == 8
}

impl Rewards {
    pub(super) fn new(ws: &[PlayerWorld]) -> Rewards {
        Rewards {
            badge: ws
                .iter()
                .map(|w| match w.opts.str("shuffle_badge_rewards") {
                    "vanilla" => Badge::Vanilla,
                    "shuffle" => Badge::Shuffle,
                    _ => Badge::Anything,
                })
                .collect(),
            tm: ws
                .iter()
                .map(|w| match w.opts.str("shuffle_tm_rewards") {
                    "shuffle" => Tm::Shuffle,
                    "hm_with_badge" => Tm::HmWithBadge,
                    _ => Tm::Anything,
                })
                .collect(),
        }
    }

    /// True when nothing is restricted (the fill is then as before).
    pub(super) fn none(&self) -> bool {
        self.badge.iter().all(|b| *b == Badge::Anything) && self.tm.iter().all(|t| *t == Tm::Anything)
    }

    /// True when location `l` of player `q` has a rule of its own.
    pub(super) fn restricted(&self, m: &Model, q: usize, l: usize) -> bool {
        (self.badge[q] != Badge::Anything && m.locs[l].kind == "badge")
            || (self.tm[q] != Tm::Anything && m.locs[l].kind == "tm")
    }

    /// May location `l` of player `q` hold item `i` of player `p`?
    pub(super) fn allowed(&self, m: &Model, q: usize, l: usize, p: usize, i: u32) -> bool {
        // the location's side
        match self.badge[q] {
            Badge::Vanilla if m.locs[l].kind == "badge" => {
                return p == q && m.items[i as usize].id == badge_of(m, l);
            }
            Badge::Shuffle if m.locs[l].kind == "badge" => return p == q && is_badge(m, i),
            _ => {}
        }
        if self.tm[q] != Tm::Anything && m.locs[l].kind == "tm" && !(p == q && is_tm_hm(m, i)) {
            return false;
        }
        // the item's side
        if is_badge(m, i) && self.badge[p] != Badge::Anything && !(q == p && m.locs[l].kind == "badge") {
            return false;
        }
        if self.tm[p] == Tm::HmWithBadge && is_hm_or_flash(m, i) && !(q == p && is_badge_tm(m, l)) {
            return false;
        }
        true
    }
}

//! eidos's field Tracker (`tracker_set`, eidos docs/mod-services.md, "The
//! Tracker"): the player's locations by area, each found, open (reachable
//! now with the items the player holds) or locked, the items that open
//! locations with the count held, and the totals.
//!
//! - Areas: `data/world/areas.json` (tools/areas.py): a place the player
//!   knows (town, route, cave, gym) with its zones. A location belongs to
//!   the area its name starts with; extra checks (Pokedex, trainers) go by
//!   their region's name, the Pokedex ones under "Pokedex".
//! - Reach: the generator's own rules (`arkh_gen::gen::reach`), with the
//!   items the player found for themself and the ones others sent.
//! - The text is made again when a check is found or an item comes, at
//!   most once a second, and only sent when it changed.

use std::collections::BTreeMap;

use arkh_gen::gen::reach::Logic;
use arkh_gen::gen::PlayerOut;
use arkh_gen::model::Model;
use serde_json::Value;

use crate::{Game, Host};

const AREAS: &str = include_str!("../../../data/world/areas.json");
/// Frames between two sends (a second).
pub const SEND_FRAMES: u32 = 60;
/// eidos keeps this many checks (engine/ui/tracker.c, TR_CHECKS)
pub const MAX_CHECKS: usize = 2000;
/// and this many bytes of text
pub const MAX_TEXT: usize = 128 * 1024;
/// and this many characters of an area's name
const NAME_MAX: usize = 39;

/// One area: its name, zones, and the player's locations in it (id, the
/// name shown).
pub struct Area {
    pub name: String,
    pub zones: Vec<u32>,
    pub checks: Vec<(u32, String)>,
}

pub struct Tracker {
    /// the world file this tracker is for
    pub world: String,
    model: Model,
    logic: Logic,
    pub areas: Vec<Area>,
    /// (group, model item index) of the items that open locations
    items: Vec<(&'static str, u32)>,
    /// what was sent last, and the inputs it was made from
    pub sent: Option<String>,
    seen: (usize, usize),
    pub dirty: bool,
    pub wait: u32,
}

/// The group a progression item is listed under.
fn group_of(g: &str) -> Option<&'static str> {
    match g {
        "badge" => Some("Badges"),
        "hm" => Some("HMs"),
        "season" => Some("Seasons"),
        "other" => None,
        _ => Some("Key items"),
    }
}

/// Cut a name to what eidos keeps.
fn cut(s: &str, max: usize) -> String {
    s.chars().filter(|c| *c != '\t' && *c != '\n').take(max).collect()
}

impl Tracker {
    pub fn new(po: &PlayerOut, world: &str) -> Result<Tracker, String> {
        let model = arkh_gen::model()?;
        let logic = Logic::new(&model, po)?;
        let file: Value = serde_json::from_str(AREAS).map_err(|e| format!("areas.json: {e}"))?;
        let merge: Vec<(String, String)> = file["merge"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| Some((m[0].as_str()?.to_string(), m[1].as_str()?.to_string()))).collect())
            .unwrap_or_default();
        let mut areas: Vec<Area> = file["areas"]
            .as_array()
            .ok_or("areas.json: areas")?
            .iter()
            .map(|a| Area {
                name: a["name"].as_str().unwrap_or("").to_string(),
                zones: a["zones"].as_array().map(|z| z.iter().filter_map(|x| x.as_u64().map(|x| x as u32)).collect()).unwrap_or_default(),
                checks: Vec::new(),
            })
            .collect();
        let index: BTreeMap<String, usize> = areas.iter().enumerate().map(|(i, a)| (a.name.clone(), i)).collect();
        // the known area a name starts with (the longest), after the merges
        let known = |head: &str| -> Option<usize> {
            let head = merge.iter().find(|(start, _)| head.starts_with(start.as_str())).map(|(_, n)| n.as_str()).unwrap_or(head);
            index.get(head).copied().or_else(|| {
                index.iter().filter(|(n, _)| head.starts_with(n.as_str())).max_by_key(|(n, _)| n.len()).map(|(_, &i)| i)
            })
        };
        for (&id, &l) in &logic.locs {
            let loc = &model.locs[l];
            let (head, rest) = loc.name.split_once(" - ").unwrap_or((loc.name.as_str(), loc.name.as_str()));
            let a = if head.starts_with("Pok") && head.ends_with("dex") {
                None
            } else {
                known(head).or_else(|| known(&model.regions[loc.region as usize]))
            };
            let name = match a {
                Some(i) => i,
                None => {
                    // a place of its own (the Pokedex, a trainer's place
                    // no area names)
                    let n = if head.starts_with("Pok") && head.ends_with("dex") { "Pokedex".to_string() } else { head.to_string() };
                    match areas.iter().position(|x| x.name == n) {
                        Some(i) => i,
                        None => {
                            areas.push(Area { name: n, zones: Vec::new(), checks: Vec::new() });
                            areas.len() - 1
                        }
                    }
                }
            };
            areas[name].checks.push((id, rest.to_string()));
        }
        areas.retain(|a| !a.checks.is_empty());
        let mut items: Vec<(&'static str, u32)> = logic
            .progression
            .iter()
            .filter_map(|&i| group_of(&model.items[i as usize].group).map(|g| (g, i)))
            .collect();
        let order = |g: &str| ["Badges", "HMs", "Key items", "Seasons"].iter().position(|x| *x == g).unwrap_or(9);
        items.sort_by_key(|(g, i)| (order(g), *i));
        Ok(Tracker { world: world.to_string(), model, logic, areas, items, sent: None, seen: (usize::MAX, 0), dirty: true, wait: 0 })
    }

    /// The ids of the items that open locations in this player's game.
    pub fn progression_ids(&self) -> Vec<u32> {
        self.logic.progression.iter().filter_map(|&i| self.model.items[i as usize].id).collect()
    }

    /// The items the player holds that the logic counts: their own found,
    /// and what others sent.
    pub fn held(g: &Game) -> Vec<u32> {
        let mut v: Vec<u32> = g
            .state
            .checked
            .iter()
            .filter_map(|l| g.placed.get(l))
            .filter(|(_, to)| *to == g.slot)
            .map(|(i, _)| *i)
            .collect();
        v.extend(&g.state.got);
        v
    }

    /// The tracker's text for this game now.
    pub fn text(&self, g: &Game) -> String {
        let held = Tracker::held(g);
        let open: std::collections::BTreeSet<u32> = self.logic.reach(&self.model, &held).into_iter().collect();
        let mut out = String::new();
        let (mut found, mut total, mut shown) = (0, 0, 0);
        for a in &self.areas {
            let zones: Vec<String> = a.zones.iter().map(|z| z.to_string()).collect();
            let head = format!("area\t{}\t{}\n", cut(&a.name, NAME_MAX), zones.join(","));
            let mut body = String::new();
            for (id, name) in &a.checks {
                total += 1;
                let state = if g.state.checked.contains(id) {
                    found += 1;
                    "found"
                } else if open.contains(id) {
                    "open"
                } else {
                    "locked"
                };
                if shown < MAX_CHECKS {
                    shown += 1;
                    body.push_str(&format!("check\t{state}\t{}\n", cut(name, 60)));
                }
            }
            if !body.is_empty() {
                out.push_str(&head);
                out.push_str(&body);
            }
        }
        for (group, i) in &self.items {
            let it = &self.model.items[*i as usize];
            let n = it.id.map(|id| held.iter().filter(|&&h| h == id).count()).unwrap_or(0);
            out.push_str(&format!("item\t{group}\t{n}\t{}\n", cut(&it.name, 40)));
        }
        out.push_str(&format!("count\t{found}\t{total}\n"));
        if out.len() > MAX_TEXT {
            // eidos keeps 128 KB: the checks past it are left out
            let mut cut_at = MAX_TEXT;
            while !out.is_char_boundary(cut_at) {
                cut_at -= 1;
            }
            let end = out[..cut_at].rfind('\n').map(|e| e + 1).unwrap_or(0);
            let tail = format!("count\t{found}\t{total}\n");
            out.truncate(end.saturating_sub(tail.len()));
            if let Some(e) = out.rfind('\n') {
                out.truncate(e + 1);
            }
            out.push_str(&tail);
        }
        out
    }

    /// Each frame: when the checks or the items changed, make the text
    /// again (at most once a second) and send it if it changed.
    pub fn tick(&mut self, host: &mut dyn Host, g: &Game) {
        let now = (g.state.checked.len(), g.state.got.len());
        if now != self.seen {
            self.seen = now;
            self.dirty = true;
        }
        if self.wait > 0 {
            self.wait -= 1;
            return;
        }
        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.wait = SEND_FRAMES;
        let text = self.text(g);
        if self.sent.as_deref() != Some(text.as_str()) {
            // an eidos without the Tracker refuses it; the next change tries again
            host.tracker_set(&text);
            self.sent = Some(text);
        }
    }
}

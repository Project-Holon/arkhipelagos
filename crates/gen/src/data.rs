//! The game's logic data (data/world/*.json), compiled in, and the rule
//! language.
//!
//! Rule language (JSON):
//!   true / false
//!   {"has": NAME}                 the player has the item or event (count 1)
//!   {"has": NAME, "count": N}
//!   {"all": [R...]}  {"any": [R...]}  {"not": R}
//!   {"reach": REGION}             the player can reach the region
//!   {"rule": NAME}                a named rule (rules.json "rules")
//!   {"include": NAME}             a named inclusion rule (rules.json "include")
//!   {"if": R, "then": R}          when R holds, then R; else true
//!   {"option": NAME, "eq": V}     a player option equals V
//!   {"option": NAME, "has": V}    a list option holds V
//!   {"option": NAME, "set": true} an option is on
//!   {"value": NAME}               a value the generator computes
//!   {"has_any_set": NAME}         has caught a species of a set (species_rules.json "sets")
//!   {"has_all_group": NAME}, {"has_any_group": NAME}
//!   {"has_value": NAME}           the per-world pick: "other_locations_species"
//!                                 (caught) or "driftveil_random_tm" (held)
//!   {"seen_count": N}             N species seen or caught
//!   {"species": [DEX, FORM]}      has caught that species and form
//!   {"level": N}                  can raise a Pokemon to level N (by the
//!                                 levels of the places reached, in steps of 5)
//!   {"has_id": N}                 has the item with the game's number N

use serde::Deserialize;
use serde_json::Value;

pub const ITEMS: &str = include_str!("../../../data/world/items.json");
pub const REGIONS: &str = include_str!("../../../data/world/regions.json");
pub const CONNECTIONS: &str = include_str!("../../../data/world/connections.json");
pub const EVENTS: &str = include_str!("../../../data/world/events.json");
pub const LOCATIONS: &str = include_str!("../../../data/world/locations.json");
pub const RULES: &str = include_str!("../../../data/world/rules.json");
pub const SPECIES_LOGIC: &str = include_str!("../../../data/world/species.json");
pub const SPECIES_RULES: &str = include_str!("../../../data/world/species_rules.json");

/// The hash of the compiled-in data (logic and game facts). Carriage
/// returns are skipped, so a checkout with CRLF line ends (Windows) hashes
/// the same as one with LF.
pub fn data_hash() -> u64 {
    let mut all = vec![ITEMS, REGIONS, CONNECTIONS, EVENTS, LOCATIONS, RULES, SPECIES_LOGIC, SPECIES_RULES];
    all.extend(crate::facts::ALL);
    let parts: Vec<Vec<u8>> = all
        .iter()
        .map(|s| s.bytes().filter(|b| *b != 0x0D).collect())
        .collect();
    let refs: Vec<&[u8]> = parts.iter().map(|v| v.as_slice()).collect();
    crate::rng::fnv1a(&refs)
}

#[derive(Deserialize, Clone)]
pub struct Item {
    pub id: u32,
    pub name: String,
    pub group: String,
    pub table: String,
    pub class: String,
}

#[derive(Deserialize, Clone)]
pub struct Region {
    pub name: String,
    pub include: Value,
}

#[derive(Deserialize, Clone)]
pub struct Connection {
    pub from: String,
    pub to: String,
    pub rule: Value,
    pub rule_back: Value,
    pub one_way: bool,
    pub include: Value,
}

#[derive(Deserialize, Clone)]
pub struct Event {
    pub name: String,
    pub region: String,
    pub include: Value,
    pub rule: Value,
}

#[derive(Deserialize, Clone)]
pub struct Location {
    pub id: u32,
    pub name: String,
    pub kind: String,
    pub region: String,
    pub include: Value,
    pub rule: Value,
}

#[derive(Deserialize)]
struct ItemsFile {
    items: Vec<Item>,
}
#[derive(Deserialize)]
struct RegionsFile {
    regions: Vec<Region>,
}
#[derive(Deserialize)]
struct ConnectionsFile {
    connections: Vec<Connection>,
}
#[derive(Deserialize)]
struct EventsFile {
    events: Vec<Event>,
}
#[derive(Deserialize)]
struct LocationsFile {
    locations: Vec<Location>,
}
#[derive(Deserialize)]
struct RulesFile {
    rules: std::collections::BTreeMap<String, Value>,
    include: std::collections::BTreeMap<String, Value>,
}

#[derive(Deserialize, Clone)]
pub struct WildSource {
    pub area: String,
    pub file: u32,
    pub season: Option<u32>,
    pub method: String,
    pub regions: Vec<String>,
    pub include: Value,
    pub rule: Value,
}

#[derive(Deserialize, Clone)]
pub struct StaticSource {
    pub kind: String,
    pub name: String,
    pub slot: u32,
    pub black: [u32; 2],
    pub white: [u32; 2],
    pub region: String,
    pub include: Value,
    pub rule: Value,
    pub fixed: bool,
}

#[derive(Deserialize, Clone)]
pub struct TradeSource {
    pub name: String,
    pub black: [u32; 2],
    pub white: [u32; 2],
    pub wanted_black: u32,
    pub wanted_white: u32,
    pub region: String,
}

#[derive(Deserialize, Clone)]
pub struct TrainerPlace {
    pub id: u32,
    pub region: String,
    pub include: Value,
    pub rule: Value,
    pub boss: bool,
    pub rival: u32,
    pub gym: Option<Value>,
}

#[derive(Deserialize, Clone)]
pub struct SpeciesLogic {
    pub wild: Vec<WildSource>,
    pub statics: Vec<StaticSource>,
    pub trades: Vec<TradeSource>,
    pub trainers: Vec<TrainerPlace>,
}

#[derive(Deserialize, Clone)]
pub struct FormChange {
    pub dex: u32,
    pub region: String,
    pub include: Value,
    pub forms: Vec<Value>,
}

#[derive(Deserialize, Clone)]
pub struct SpeciesRules {
    pub evolution_methods: std::collections::BTreeMap<String, Value>,
    pub evolution_items: std::collections::BTreeMap<String, String>,
    pub sets: std::collections::BTreeMap<String, Value>,
    pub form_changes: Vec<FormChange>,
    pub legendaries: Vec<u32>,
}

pub struct Data {
    pub items: Vec<Item>,
    pub regions: Vec<Region>,
    pub connections: Vec<Connection>,
    pub events: Vec<Event>,
    pub locations: Vec<Location>,
    pub rules: std::collections::BTreeMap<String, Value>,
    pub include: std::collections::BTreeMap<String, Value>,
    pub species: SpeciesLogic,
    pub species_rules: SpeciesRules,
}

impl Data {
    pub fn load() -> Result<Data, String> {
        let p = |what: &str, e: serde_json::Error| format!("{what}: {e}");
        let items: ItemsFile = serde_json::from_str(ITEMS).map_err(|e| p("items.json", e))?;
        let regions: RegionsFile = serde_json::from_str(REGIONS).map_err(|e| p("regions.json", e))?;
        let conns: ConnectionsFile = serde_json::from_str(CONNECTIONS).map_err(|e| p("connections.json", e))?;
        let events: EventsFile = serde_json::from_str(EVENTS).map_err(|e| p("events.json", e))?;
        let locs: LocationsFile = serde_json::from_str(LOCATIONS).map_err(|e| p("locations.json", e))?;
        let rules: RulesFile = serde_json::from_str(RULES).map_err(|e| p("rules.json", e))?;
        let species: SpeciesLogic = serde_json::from_str(SPECIES_LOGIC).map_err(|e| p("species.json", e))?;
        let species_rules: SpeciesRules =
            serde_json::from_str(SPECIES_RULES).map_err(|e| p("species_rules.json", e))?;
        Ok(Data {
            species,
            species_rules,
            items: items.items,
            regions: regions.regions,
            connections: conns.connections,
            events: events.events,
            locations: locs.locations,
            rules: rules.rules,
            include: rules.include,
        })
    }
}

/// A rule, compiled: names resolved to indices.
#[derive(Clone, Debug)]
pub enum Rule {
    Const(bool),
    Has(u32, u32),
    All(Vec<Rule>),
    Any(Vec<Rule>),
    Not(Box<Rule>),
    Reach(u32),
    Named(u32),
    If(Box<Rule>, Box<Rule>),
    OptEq(String, Value),
    OptHas(String, String),
    OptSet(String),
    Value(String),
    /// has caught a species of the set (index into the model's sets)
    SetAny(u32),
    /// that many species seen or caught
    Seen(u32),
    /// has caught the mon (species and form)
    Caught(u32),
    /// can raise a Pokemon to this level
    Level(u32),
    /// 0: has caught the "other locations" species; 1: holds the Driftveil TM
    Pick(u32),
}

pub struct Names<'a> {
    pub group: &'a dyn Fn(&str) -> Vec<u32>,
    pub item: &'a dyn Fn(&str) -> Option<u32>,
    pub item_id: &'a dyn Fn(u32) -> Option<u32>,
    pub region: &'a dyn Fn(&str) -> Option<u32>,
    pub rule: &'a dyn Fn(&str) -> Option<u32>,
    pub include: &'a dyn Fn(&str) -> Option<&'a Value>,
    pub set: &'a dyn Fn(&str) -> Option<u32>,
    pub mon: &'a dyn Fn(u32, u32) -> Option<u32>,
}

pub fn compile(v: &Value, n: &Names) -> Result<Rule, String> {
    Ok(match v {
        Value::Bool(b) => Rule::Const(*b),
        Value::Null => Rule::Const(true),
        Value::Object(m) => {
            let s = |k: &str| m.get(k).and_then(|x| x.as_str()).map(|x| x.to_string());
            let num = |k: &str| m.get(k).and_then(|x| x.as_u64()).map(|x| x as u32);
            // "option" first: {"option": O, "has": V} is an option test, not an item
            if let Some(o) = s("option") {
                if let Some(eq) = m.get("eq") {
                    Rule::OptEq(o, eq.clone())
                } else if let Some(h) = s("has") {
                    Rule::OptHas(o, h)
                } else {
                    Rule::OptSet(o)
                }
            } else if let Some(Value::Array(a)) = m.get("species") {
                let d = a.first().and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                let f = a.get(1).and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                Rule::Caught((n.mon)(d, f).ok_or_else(|| format!("no species {d} form {f}"))?)
            } else if let Some(l) = num("level") {
                Rule::Level(l)
            } else if let Some(i) = num("has_id") {
                match (n.item_id)(i) {
                    Some(x) => Rule::Has(x, 1),
                    None => Rule::Const(false),
                }
            } else if let Some(name) = s("has") {
                let count = m.get("count").and_then(|c| c.as_u64()).unwrap_or(1) as u32;
                // an item no pool holds (a species, "Out of logic") can never be had
                match (n.item)(&name) {
                    Some(i) => Rule::Has(i, count),
                    None => Rule::Const(false),
                }
            } else if let Some(Value::Array(a)) = m.get("all") {
                Rule::All(a.iter().map(|x| compile(x, n)).collect::<Result<_, _>>()?)
            } else if let Some(Value::Array(a)) = m.get("any") {
                Rule::Any(a.iter().map(|x| compile(x, n)).collect::<Result<_, _>>()?)
            } else if let Some(x) = m.get("not") {
                Rule::Not(Box::new(compile(x, n)?))
            } else if let Some(r) = s("reach") {
                Rule::Reach((n.region)(&r).ok_or_else(|| format!("unknown region {r}"))?)
            } else if let Some(r) = s("rule") {
                Rule::Named((n.rule)(&r).ok_or_else(|| format!("unknown rule {r}"))?)
            } else if let Some(r) = s("include") {
                let inc = (n.include)(&r).ok_or_else(|| format!("unknown include rule {r}"))?;
                compile(inc, n)?
            } else if let (Some(c), Some(t)) = (m.get("if"), m.get("then")) {
                Rule::If(Box::new(compile(c, n)?), Box::new(compile(t, n)?))
            } else if let Some(x) = s("value") {
                Rule::Value(x)
            } else if let Some(x) = s("has_any_set") {
                Rule::SetAny((n.set)(&x).ok_or_else(|| format!("unknown species set {x}"))?)
            } else if let Some(x) = s("has_value") {
                match x.as_str() {
                    "other_locations_species" => Rule::Pick(0),
                    "driftveil_random_tm" => Rule::Pick(1),
                    _ => return Err(format!("unknown value {x}")),
                }
            } else if let Some(c) = num("seen_count") {
                Rule::Seen(c)
            } else if let Some(g) = s("has_all_group") {
                Rule::All((n.group)(&g).into_iter().map(|i| Rule::Has(i, 1)).collect())
            } else if let Some(g) = s("has_any_group") {
                Rule::Any((n.group)(&g).into_iter().map(|i| Rule::Has(i, 1)).collect())
            } else {
                return Err(format!("unknown rule form {v}"));
            }
        }
        _ => return Err(format!("unknown rule form {v}")),
    })
}

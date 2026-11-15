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
//!   {"has_any_set": NAME}         has one of a species set computed at generation
//!   {"has_all_set": NAME}, {"has_all_group": NAME}, {"has_any_group": NAME}
//!   {"has_value": NAME}           has the item named by a generated value
//!   {"seen_count": N}             N species seen or caught

use serde::Deserialize;
use serde_json::Value;

pub const ITEMS: &str = include_str!("../../../data/world/items.json");
pub const REGIONS: &str = include_str!("../../../data/world/regions.json");
pub const CONNECTIONS: &str = include_str!("../../../data/world/connections.json");
pub const EVENTS: &str = include_str!("../../../data/world/events.json");
pub const LOCATIONS: &str = include_str!("../../../data/world/locations.json");
pub const RULES: &str = include_str!("../../../data/world/rules.json");

pub fn data_hash() -> u64 {
    crate::rng::fnv1a(&[
        ITEMS.as_bytes(),
        REGIONS.as_bytes(),
        CONNECTIONS.as_bytes(),
        EVENTS.as_bytes(),
        LOCATIONS.as_bytes(),
        RULES.as_bytes(),
    ])
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

pub struct Data {
    pub items: Vec<Item>,
    pub regions: Vec<Region>,
    pub connections: Vec<Connection>,
    pub events: Vec<Event>,
    pub locations: Vec<Location>,
    pub rules: std::collections::BTreeMap<String, Value>,
    pub include: std::collections::BTreeMap<String, Value>,
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
        Ok(Data {
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
    /// generation-time species sets and counts: see logic::v1_species
    Species(String),
}

pub struct Names<'a> {
    pub group: &'a dyn Fn(&str) -> Vec<u32>,
    pub item: &'a dyn Fn(&str) -> Option<u32>,
    pub region: &'a dyn Fn(&str) -> Option<u32>,
    pub rule: &'a dyn Fn(&str) -> Option<u32>,
    pub include: &'a dyn Fn(&str) -> Option<&'a Value>,
}

pub fn compile(v: &Value, n: &Names) -> Result<Rule, String> {
    Ok(match v {
        Value::Bool(b) => Rule::Const(*b),
        Value::Null => Rule::Const(true),
        Value::Object(m) => {
            let s = |k: &str| m.get(k).and_then(|x| x.as_str()).map(|x| x.to_string());
            if let Some(name) = s("has") {
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
            } else if let Some(o) = s("option") {
                if let Some(eq) = m.get("eq") {
                    Rule::OptEq(o, eq.clone())
                } else if let Some(h) = s("has") {
                    Rule::OptHas(o, h)
                } else {
                    Rule::OptSet(o)
                }
            } else if let Some(x) = s("value") {
                Rule::Value(x)
            } else if let Some(x) = s("has_any_set") {
                Rule::Species(format!("any:{x}"))
            } else if let Some(x) = s("has_all_set") {
                Rule::Species(format!("all:{x}"))
            } else if let Some(x) = s("has_value") {
                Rule::Species(format!("value:{x}"))
            } else if let Some(c) = m.get("seen_count") {
                Rule::Species(format!("seen:{c}"))
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

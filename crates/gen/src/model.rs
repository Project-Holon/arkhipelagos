//! The compiled logic: indices instead of names, rules compiled once.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::data::{compile, Data, Names, Rule};
use crate::facts::Facts;
use crate::species::SpeciesModel;

pub struct Conn {
    pub from: u32,
    pub to: u32,
    pub rule: Rule,
    pub back: Rule,
    pub one_way: bool,
    pub include: Rule,
}

pub struct Loc {
    pub id: u32,
    pub name: String,
    pub region: u32,
    pub rule: Rule,
    pub include: Rule,
    /// the game gives this location's item somewhere the mod can change
    pub site: bool,
}

pub struct Ev {
    pub item: u32,
    pub region: u32,
    pub rule: Rule,
    pub include: Rule,
}

pub struct ItemInfo {
    /// Archipelago id; events have none
    pub id: Option<u32>,
    pub name: String,
    pub group: String,
    pub table: String,
    pub class: String,
}

pub struct Model {
    pub items: Vec<ItemInfo>,
    pub regions: Vec<String>,
    pub region_include: Vec<Rule>,
    pub conns: Vec<Conn>,
    /// per region: connections leaving it (index into conns, forward flag)
    pub out: Vec<Vec<(u32, bool)>>,
    pub locs: Vec<Loc>,
    pub events: Vec<Ev>,
    pub rules: Vec<Rule>,
    pub menu: u32,
    pub item_by_name: BTreeMap<String, u32>,
    pub item_by_id: BTreeMap<u32, u32>,
    pub region_by_name: BTreeMap<String, u32>,
    pub rule_by_name: BTreeMap<String, u32>,
    pub include: BTreeMap<String, Value>,
    /// names of the species sets rules ask for (species_rules.json "sets")
    pub set_names: Vec<String>,
    pub facts: Facts,
    pub species: SpeciesModel,
}

impl Model {
    /// Compile a rule against this model's names.
    pub fn compile(&self, v: &Value) -> Result<Rule, String> {
        let group_items = |g: &str| -> Vec<u32> {
            self.items.iter().enumerate().filter(|(_, it)| it.group == g).map(|(i, _)| i as u32).collect()
        };
        let item_fn = |n: &str| self.item_by_name.get(n).copied();
        let item_id_fn = |n: u32| self.item_by_id.get(&n).copied();
        let region_fn = |n: &str| self.region_by_name.get(n).copied();
        let rule_fn = |n: &str| self.rule_by_name.get(n).copied();
        let include_fn = |n: &str| self.include.get(n);
        let set_fn = |n: &str| self.set_names.iter().position(|s| s == n).map(|i| i as u32);
        let mon_fn = |d: u32, f: u32| self.facts.mon(d, f);
        let names = Names {
            group: &group_items,
            item: &item_fn,
            item_id: &item_id_fn,
            region: &region_fn,
            rule: &rule_fn,
            include: &include_fn,
            set: &set_fn,
            mon: &mon_fn,
        };
        compile(v, &names)
    }

    pub fn region(&self, n: &str, what: &str) -> Result<u32, String> {
        self.region_by_name.get(n).copied().ok_or_else(|| format!("{what}: unknown region {n}"))
    }

    pub fn build(d: &Data) -> Result<Model, String> {
        let mut items: Vec<ItemInfo> = d
            .items
            .iter()
            .map(|i| ItemInfo {
                id: Some(i.id),
                name: i.name.clone(),
                group: i.group.clone(),
                table: i.table.clone(),
                class: i.class.clone(),
            })
            .collect();
        for e in &d.events {
            items.push(ItemInfo {
                id: None,
                name: e.name.clone(),
                group: "event".into(),
                table: "events".into(),
                class: "event".into(),
            });
        }
        let item_by_name: BTreeMap<String, u32> =
            items.iter().enumerate().map(|(i, it)| (it.name.clone(), i as u32)).collect();
        let item_by_id: BTreeMap<u32, u32> =
            items.iter().enumerate().filter_map(|(i, it)| it.id.map(|x| (x, i as u32))).collect();
        let regions: Vec<String> = d.regions.iter().map(|r| r.name.clone()).collect();
        let region_by_name: BTreeMap<String, u32> =
            regions.iter().enumerate().map(|(i, r)| (r.clone(), i as u32)).collect();
        let rule_by_name: BTreeMap<String, u32> =
            d.rules.keys().enumerate().map(|(i, r)| (r.clone(), i as u32)).collect();
        let set_names: Vec<String> = d.species_rules.sets.keys().cloned().collect();
        let facts = Facts::load()?;
        let menu = *region_by_name.get("Menu").ok_or("no Menu region")?;

        let mut m = Model {
            items,
            regions,
            region_include: Vec::new(),
            conns: Vec::new(),
            out: Vec::new(),
            locs: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            menu,
            item_by_name,
            item_by_id,
            region_by_name,
            rule_by_name,
            include: d.include.clone(),
            set_names,
            facts,
            species: SpeciesModel::default(),
        };
        let c = |m: &Model, v: &Value, what: &str| m.compile(v).map_err(|e| format!("{what}: {e}"));

        m.rules = d.rules.iter().map(|(k, v)| c(&m, v, k)).collect::<Result<Vec<_>, _>>()?;
        m.region_include = d.regions.iter().map(|r| c(&m, &r.include, &r.name)).collect::<Result<Vec<_>, _>>()?;
        let mut conns = Vec::new();
        for x in &d.connections {
            let what = format!("{} -> {}", x.from, x.to);
            conns.push(Conn {
                from: m.region(&x.from, &what)?,
                to: m.region(&x.to, &what)?,
                rule: c(&m, &x.rule, &what)?,
                back: c(&m, &x.rule_back, &what)?,
                one_way: x.one_way,
                include: c(&m, &x.include, &what)?,
            });
        }
        let mut out = vec![Vec::new(); m.regions.len()];
        for (i, cn) in conns.iter().enumerate() {
            out[cn.from as usize].push((i as u32, true));
            if !cn.one_way {
                out[cn.to as usize].push((i as u32, false));
            }
        }
        let mut locs = Vec::new();
        for l in &d.locations {
            locs.push(Loc {
                id: l.id,
                name: l.name.clone(),
                region: m.region(&l.region, &l.name)?,
                rule: c(&m, &l.rule, &l.name)?,
                include: c(&m, &l.include, &l.name)?,
                site: !d.no_site.contains(&l.id),
            });
        }
        let mut events = Vec::new();
        for e in &d.events {
            events.push(Ev {
                item: m.item_by_name[&e.name],
                region: m.region(&e.region, &e.name)?,
                rule: c(&m, &e.rule, &e.name)?,
                include: c(&m, &e.include, &e.name)?,
            });
        }
        m.conns = conns;
        m.out = out;
        m.locs = locs;
        m.events = events;
        m.species = SpeciesModel::build(&m, d)?;
        Ok(m)
    }
}

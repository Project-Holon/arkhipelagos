//! The compiled logic: indices instead of names, rules compiled once.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::data::{compile, Data, Names, Rule};

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
}

impl Model {
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
        let regions: Vec<String> = d.regions.iter().map(|r| r.name.clone()).collect();
        let region_by_name: BTreeMap<&str, u32> =
            regions.iter().enumerate().map(|(i, r)| (r.as_str(), i as u32)).collect();
        let rule_names: Vec<&String> = d.rules.keys().collect();
        let rule_by_name: BTreeMap<&str, u32> =
            rule_names.iter().enumerate().map(|(i, r)| (r.as_str(), i as u32)).collect();
        let group_items = |g: &str| -> Vec<u32> {
            items.iter().enumerate().filter(|(_, it)| it.group == g).map(|(i, _)| i as u32).collect()
        };
        let item_fn = |n: &str| item_by_name.get(n).copied();
        let region_fn = |n: &str| region_by_name.get(n).copied();
        let rule_fn = |n: &str| rule_by_name.get(n).copied();
        let include_fn = |n: &str| d.include.get(n);
        let names = Names { group: &group_items, item: &item_fn, region: &region_fn, rule: &rule_fn, include: &include_fn };
        let c = |v: &Value, what: &str| compile(v, &names).map_err(|e| format!("{what}: {e}"));

        let rules = d.rules.iter().map(|(k, v)| c(v, k)).collect::<Result<Vec<_>, _>>()?;
        let region_include = d.regions.iter().map(|r| c(&r.include, &r.name)).collect::<Result<Vec<_>, _>>()?;
        let reg = |n: &str, what: &str| region_fn(n).ok_or_else(|| format!("{what}: unknown region {n}"));
        let mut conns = Vec::new();
        for x in &d.connections {
            let what = format!("{} -> {}", x.from, x.to);
            conns.push(Conn {
                from: reg(&x.from, &what)?,
                to: reg(&x.to, &what)?,
                rule: c(&x.rule, &what)?,
                back: c(&x.rule_back, &what)?,
                one_way: x.one_way,
                include: c(&x.include, &what)?,
            });
        }
        let mut out = vec![Vec::new(); regions.len()];
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
                region: reg(&l.region, &l.name)?,
                rule: c(&l.rule, &l.name)?,
                include: c(&l.include, &l.name)?,
            });
        }
        let mut events = Vec::new();
        for e in &d.events {
            events.push(Ev {
                item: item_by_name[&e.name],
                region: reg(&e.region, &e.name)?,
                rule: c(&e.rule, &e.name)?,
                include: c(&e.include, &e.name)?,
            });
        }
        let menu = reg("Menu", "start")?;
        Ok(Model { items, regions, region_include, conns, out, locs, events, rules, menu, item_by_name })
    }
}

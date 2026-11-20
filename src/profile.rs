//! Persistent player profile: soul points earned by dying, upgrades bought
//! with them, unlocked classes and the Hall of the Dead history.
//! Stored as JSON in ~/.local/share/grazzark/profile.json.

use crate::data::{ClassDef, Skill, DIFFICULTIES, LEGACY_UPGRADES, LORE_MASTER, MONSTERS, UPGRADES};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RunRecord {
    pub class: String,
    pub level: i32,
    pub depth: u32,
    pub cause: String,
    pub points: u32,
    pub won: bool,
    pub turns: u64,
    pub kills: u32,
    #[serde(default)]
    pub difficulty: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Profile {
    pub points: u32,
    pub total_points: u32,
    pub runs: u32,
    pub wins: u32,
    pub best_depth: u32,
    pub upgrades: HashMap<String, u32>,
    pub history: Vec<RunRecord>,
    pub volume: Option<f32>,
    /// Lifetime kills of each monster, by name.
    pub bestiary: HashMap<String, u32>,
    /// Highest difficulty unlocked (0 = Normal only).
    pub unlocked_diff: u8,
}

/// Bonuses derived from purchased upgrades, applied at the start of each run.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Bonuses {
    pub hp: i32,
    pub dmg: i32,
    pub ac: i32,
    pub mp: i32,
    pub sight: i32,
    pub crit: f32,
    pub xp_mult: f32,
    pub regen: i32,
    pub potions: i32,
    pub luck: i32,
    pub siphon: i32,
    pub skills: Vec<Skill>,
    pub harvest: i32,
    pub bloodlust: i32,
    pub pockets: i32,
    pub carto: i32,
    pub anchor: bool,
    pub coup: i32,
    pub alchemy: i32,
    pub thorns: i32,
    pub bloodmagic: bool,
    pub hunter: i32,
    pub heirloom: i32,
    pub ironwill: i32,
    pub leech: i32,
    pub laststand: i32,
    pub fleet: i32,
    pub scavenger: i32,
    /// Monsters mastered in the bestiary (200+ lifetime kills).
    pub mastered: Vec<String>,
}

impl Profile {
    pub fn path() -> PathBuf {
        match std::env::var("HOME") {
            Ok(h) if !h.is_empty() => PathBuf::from(h).join(".local/share/grazzark/profile.json"),
            _ => PathBuf::from("grazzark_profile.json"),
        }
    }

    pub fn load() -> Profile {
        let mut p: Profile = fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if p.migrate() > 0 {
            p.save();
        }
        p
    }

    /// Refund upgrades that no longer exist (the old shared skills, now
    /// replaced by class skills). Returns the points refunded.
    pub fn migrate(&mut self) -> u32 {
        let mut refund = 0;
        for &(id, cost) in LEGACY_UPGRADES {
            if let Some(r) = self.upgrades.remove(id) {
                if r > 0 {
                    refund += cost;
                }
            }
        }
        self.points += refund;
        refund
    }

    pub fn kills_of(&self, name: &str) -> u32 {
        *self.bestiary.get(name).unwrap_or(&0)
    }

    /// Fold a run's kills into the bestiary. Returns the names newly mastered.
    pub fn record_kills(&mut self, slain: &HashMap<String, u32>) -> Vec<String> {
        let mut mastered = Vec::new();
        for (name, &n) in slain {
            let e = self.bestiary.entry(name.clone()).or_insert(0);
            let before = *e;
            *e += n;
            if before < LORE_MASTER && *e >= LORE_MASTER {
                mastered.push(name.clone());
            }
        }
        mastered
    }

    pub fn diff_unlocked(&self, d: usize) -> bool {
        d <= self.unlocked_diff as usize
    }

    /// Winning on difficulty `d` unlocks the next one. Returns its name if new.
    pub fn record_win(&mut self, d: usize) -> Option<&'static str> {
        let next = d + 1;
        if d <= self.unlocked_diff as usize && next < DIFFICULTIES.len() && next > self.unlocked_diff as usize {
            self.unlocked_diff = next as u8;
            Some(DIFFICULTIES[next].name)
        } else {
            None
        }
    }

    pub fn save(&self) {
        let p = Self::path();
        if let Some(d) = p.parent() {
            let _ = fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let tmp = p.with_extension("json.tmp");
            if fs::write(&tmp, s).is_ok() {
                let _ = fs::rename(&tmp, &p);
            }
        }
    }

    pub fn rank(&self, id: &str) -> u32 {
        *self.upgrades.get(id).unwrap_or(&0)
    }

    /// Cost of the next rank of upgrade `idx`, or None if maxed.
    pub fn next_cost(&self, idx: usize) -> Option<u32> {
        let u = &UPGRADES[idx];
        let r = self.rank(u.id);
        if r >= u.max {
            None
        } else {
            // Each rank costs the base price times the square of the rank.
            Some(u.cost * (r + 1) * (r + 1))
        }
    }

    pub fn buy(&mut self, idx: usize) -> Result<String, String> {
        let u = &UPGRADES[idx];
        let cost = match self.next_cost(idx) {
            None => return Err(format!("{} is already at max rank.", u.title())),
            Some(c) => c,
        };
        if self.points < cost {
            return Err(format!("You need {} soul points (you have {}).", cost, self.points));
        }
        self.points -= cost;
        *self.upgrades.entry(u.id.to_string()).or_insert(0) += 1;
        self.save();
        Ok(format!("Acquired {} (rank {}).", u.title(), self.rank(u.id)))
    }

    pub fn class_unlocked(&self, c: &ClassDef) -> bool {
        match c.unlock {
            None => true,
            Some(id) => self.rank(id) > 0,
        }
    }

    pub fn bonuses(&self) -> Bonuses {
        let r = |id: &str| self.rank(id) as i32;
        let mut skills = Vec::new();
        for u in UPGRADES.iter() {
            if let Some(s) = u.skill {
                if self.rank(u.id) > 0 {
                    skills.push(s);
                }
            }
        }
        Bonuses {
            hp: r("vitality") * 4,
            dmg: r("might"),
            ac: r("ironskin"),
            mp: r("arcana") * 3,
            sight: r("keen"),
            crit: r("keen") as f32 * 0.03,
            xp_mult: 1.0 + r("scholar") as f32 * 0.1,
            regen: r("regen"),
            potions: r("provision"),
            luck: r("luck"),
            siphon: r("siphon"),
            skills,
            harvest: r("harvest"),
            bloodlust: r("bloodlust"),
            pockets: r("pockets"),
            carto: r("carto"),
            anchor: r("anchor") > 0,
            coup: r("coup"),
            alchemy: r("alchemy"),
            thorns: r("thorns"),
            bloodmagic: r("bloodmagic") > 0,
            hunter: r("hunter"),
            heirloom: r("heirloom"),
            ironwill: r("ironwill"),
            leech: r("leech"),
            laststand: r("laststand"),
            fleet: r("fleet"),
            scavenger: r("scavenger"),
            mastered: MONSTERS
                .iter()
                .filter(|m| self.kills_of(m.name) >= LORE_MASTER)
                .map(|m| m.name.to_string())
                .collect(),
        }
    }
}

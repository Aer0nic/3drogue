//! Persistent player profile: soul points earned by dying, upgrades bought
//! with them, unlocked classes and the Hall of the Dead history.
//! Stored as JSON in ~/.local/share/grazzark/profile.json.

use crate::data::{ClassDef, Skill, UPGRADES};
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
}

/// Bonuses derived from purchased upgrades, applied at the start of each run.
#[derive(Clone, Debug, Default)]
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
}

impl Profile {
    pub fn path() -> PathBuf {
        match std::env::var("HOME") {
            Ok(h) if !h.is_empty() => PathBuf::from(h).join(".local/share/grazzark/profile.json"),
            _ => PathBuf::from("grazzark_profile.json"),
        }
    }

    pub fn load() -> Profile {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
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
            Some(u.cost * (r + 1))
        }
    }

    pub fn buy(&mut self, idx: usize) -> Result<String, String> {
        let u = &UPGRADES[idx];
        let cost = match self.next_cost(idx) {
            None => return Err(format!("{} is already at max rank.", u.name)),
            Some(c) => c,
        };
        if self.points < cost {
            return Err(format!("You need {} soul points (you have {}).", cost, self.points));
        }
        self.points -= cost;
        *self.upgrades.entry(u.id.to_string()).or_insert(0) += 1;
        self.save();
        Ok(format!("Acquired {} (rank {}).", u.name, self.rank(u.id)))
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
        }
    }
}

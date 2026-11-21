//! Persistent player profile: soul points earned by dying, upgrades bought
//! with them, unlocked classes and the Hall of the Dead history.
//! Several profiles can live side by side, each in its own folder under
//! <data dir>/profiles/<name>/ with its profile.json and its saved run.

use crate::data::{
    ClassDef, Skill, DIFFICULTIES, LEGACY_UPGRADES, LORE_COMMON, MONSTERS, UPGRADES,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// The folder of the profile in play.
static ACTIVE: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Longest profile name allowed.
pub const NAME_MAX: usize = 16;

/// Where all of GRAZZARK's data lives: %APPDATA%\grazzark on Windows,
/// ~/.local/share/grazzark elsewhere, or beside the game as a last resort.
pub fn base_dir() -> PathBuf {
    if cfg!(windows) {
        if let Ok(a) = std::env::var("APPDATA") {
            if !a.is_empty() {
                return PathBuf::from(a).join("grazzark");
            }
        }
    }
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return PathBuf::from(h).join(".local/share/grazzark");
        }
    }
    match std::env::current_exe() {
        Ok(e) => e.with_file_name("grazzark_data"),
        Err(_) => PathBuf::from("grazzark_data"),
    }
}

/// A profile found on disk, for the profile picker.
#[derive(Clone, Debug)]
pub struct ProfileEntry {
    pub name: String,
    pub dir: PathBuf,
    pub runs: u32,
    pub wins: u32,
    pub points: u32,
    pub has_save: bool,
}

/// Is this a usable character for a profile name?
pub fn name_char_ok(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '\'')
}

/// The folder name for a profile name: safe on every file system, and the
/// same for names that differ only in case.
pub fn folder_for(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Every profile under `base`, sorted by name.
pub fn list_in(base: &Path) -> Vec<ProfileEntry> {
    let mut v = Vec::new();
    let Ok(rd) = fs::read_dir(base.join("profiles")) else {
        return v;
    };
    for e in rd.flatten() {
        let dir = e.path();
        let file = dir.join("profile.json");
        if !file.is_file() {
            continue;
        }
        let p: Profile = fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let name = if p.name.trim().is_empty() {
            e.file_name().to_string_lossy().to_string()
        } else {
            p.name.clone()
        };
        v.push(ProfileEntry {
            name,
            runs: p.runs,
            wins: p.wins,
            points: p.points,
            has_save: dir.join("savegame.json").is_file(),
            dir,
        });
    }
    v.sort_by_key(|e| e.name.to_lowercase());
    v
}

/// Make a new, empty profile under `base`. Fails on a bad or taken name.
pub fn create_in(base: &Path, name: &str) -> Result<(Profile, PathBuf), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Your profile needs a name.".into());
    }
    if name.chars().count() > NAME_MAX || !name.chars().all(name_char_ok) {
        return Err(format!(
            "Use up to {} letters, numbers, spaces, - or _.",
            NAME_MAX
        ));
    }
    let folder = folder_for(name);
    let dir = base.join("profiles").join(&folder);
    if dir.join("profile.json").exists()
        || list_in(base).iter().any(|e| folder_for(&e.name) == folder)
    {
        return Err(format!("There is already a profile called {}.", name));
    }
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create the profile: {}", e))?;
    let p = Profile {
        name: name.to_string(),
        ..Default::default()
    };
    p.save_to(&dir.join("profile.json"));
    Ok((p, dir))
}

/// Older versions kept a single profile (and saved run) directly in the
/// data folder. Move it into the profiles folder, named "Default".
pub fn migrate_legacy_in(base: &Path) {
    let old = base.join("profile.json");
    if !old.is_file() {
        return;
    }
    let mut dir = base.join("profiles").join("default");
    let mut n = 2;
    while dir.exists() {
        dir = base.join("profiles").join(format!("default_{}", n));
        n += 1;
    }
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let mut p: Profile = fs::read_to_string(&old)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if p.name.trim().is_empty() {
        p.name = if n == 2 {
            "Default".to_string()
        } else {
            format!("Default {}", n - 1)
        };
    }
    p.save_to(&dir.join("profile.json"));
    if dir.join("profile.json").is_file() {
        let _ = fs::remove_file(&old);
        let save = base.join("savegame.json");
        if save.is_file() {
            let _ = fs::rename(&save, dir.join("savegame.json"));
        }
    }
}

/// The name of the profile played last, if remembered.
pub fn last_used() -> Option<String> {
    fs::read_to_string(base_dir().join("last_profile.txt"))
        .ok()
        .map(|s| s.trim().to_string())
}

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
    /// The profile's name, as the player typed it.
    pub name: String,
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
    /// Necromancer: a fallen thrall can be raised once more.
    pub rebind: bool,
    /// Monsters mastered in the bestiary (200+ lifetime kills).
    pub mastered: Vec<String>,
}

impl Profile {
    /// The profile file in play (its saved run sits beside it).
    pub fn path() -> PathBuf {
        match ACTIVE.read().ok().and_then(|a| a.clone()) {
            Some(dir) => dir.join("profile.json"),
            // Tests must never touch a player's real profiles or saved runs.
            None if cfg!(test) => std::env::temp_dir()
                .join(format!("grazzark-test-{}", std::process::id()))
                .join("profile.json"),
            None => base_dir()
                .join("profiles")
                .join("default")
                .join("profile.json"),
        }
    }

    /// Every profile on this computer.
    pub fn list() -> Vec<ProfileEntry> {
        list_in(&base_dir())
    }

    /// Create a new profile and make it the one in play.
    pub fn create(name: &str) -> Result<Profile, String> {
        let (p, dir) = create_in(&base_dir(), name)?;
        Self::activate(&dir);
        Ok(p)
    }

    /// Make the profile in `dir` the one in play and load it.
    pub fn open(dir: &Path) -> Profile {
        Self::activate(dir);
        let mut p = Self::load();
        if p.name.trim().is_empty() {
            p.name = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
        }
        let _ = fs::write(base_dir().join("last_profile.txt"), &p.name);
        p
    }

    fn activate(dir: &Path) {
        if let Ok(mut a) = ACTIVE.write() {
            *a = Some(dir.to_path_buf());
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
            let master = MONSTERS
                .iter()
                .find(|m| m.name == name.as_str())
                .map_or(LORE_COMMON.master, |m| m.lore().master);
            if before < master && *e >= master {
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
        if d <= self.unlocked_diff as usize
            && next < DIFFICULTIES.len()
            && next > self.unlocked_diff as usize
        {
            self.unlocked_diff = next as u8;
            Some(DIFFICULTIES[next].name)
        } else {
            None
        }
    }

    pub fn save(&self) {
        self.save_to(&Self::path());
    }

    fn save_to(&self, p: &Path) {
        let p = p.to_path_buf();
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
            match u.ranks.get(r as usize) {
                Some(&c) => Some(c),
                // Each rank costs the base price times the square of the rank.
                None => Some(u.cost * (r + 1) * (r + 1)),
            }
        }
    }

    pub fn buy(&mut self, idx: usize) -> Result<String, String> {
        let u = &UPGRADES[idx];
        let cost = match self.next_cost(idx) {
            None => return Err(format!("{} is already at max rank.", u.title())),
            Some(c) => c,
        };
        if self.points < cost {
            return Err(format!(
                "You need {} soul points (you have {}).",
                cost, self.points
            ));
        }
        self.points -= cost;
        *self.upgrades.entry(u.id.to_string()).or_insert(0) += 1;
        self.save();
        Ok(format!(
            "Acquired {} (rank {}).",
            u.title(),
            self.rank(u.id)
        ))
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
            rebind: r("n_rebind") > 0,
            mastered: MONSTERS
                .iter()
                .filter(|m| self.kills_of(m.name) >= m.lore().master)
                .map(|m| m.name.to_string())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("grazzark-prof-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn profiles_are_created_listed_and_kept_apart() {
        let base = temp_base("list");
        assert!(
            list_in(&base).is_empty(),
            "no profiles yet: the game asks for a name"
        );
        let (a, adir) = create_in(&base, "Grimwald").unwrap();
        assert_eq!(a.name, "Grimwald");
        create_in(&base, "  aria the bold ").unwrap();
        // Names are unique, ignoring case.
        assert!(create_in(&base, "grimwald").is_err());
        assert!(create_in(&base, "").is_err());
        assert!(create_in(&base, "bad/name").is_err());
        assert!(create_in(&base, "a name far too long for this").is_err());
        let list = list_in(&base);
        assert_eq!(
            list.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec!["aria the bold", "Grimwald"]
        );
        // Each keeps its own progress.
        let mut p: Profile =
            serde_json::from_str(&fs::read_to_string(adir.join("profile.json")).unwrap()).unwrap();
        p.points = 777;
        p.save_to(&adir.join("profile.json"));
        fs::write(adir.join("savegame.json"), "{}").unwrap();
        let list = list_in(&base);
        let g = list.iter().find(|e| e.name == "Grimwald").unwrap();
        assert_eq!(g.points, 777);
        assert!(g.has_save);
        assert!(
            !list
                .iter()
                .find(|e| e.name == "aria the bold")
                .unwrap()
                .has_save
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn an_old_single_profile_becomes_default() {
        let base = temp_base("legacy");
        let old = Profile {
            points: 1234,
            runs: 9,
            ..Default::default()
        };
        old.save_to(&base.join("profile.json"));
        fs::write(base.join("savegame.json"), "{\"saved\":1}").unwrap();
        migrate_legacy_in(&base);
        assert!(!base.join("profile.json").exists());
        let list = list_in(&base);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Default");
        assert_eq!(list[0].points, 1234);
        assert!(list[0].has_save, "the saved run moves with it");
        // Running it again changes nothing.
        migrate_legacy_in(&base);
        assert_eq!(list_in(&base).len(), 1);
        let _ = fs::remove_dir_all(&base);
    }
}

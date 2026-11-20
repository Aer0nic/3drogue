//! Monsters, factions and the hostility table that drives monster infighting.

use crate::data::{MonsterDef, MONSTERS};

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Faction {
    Player,
    Ally,
    Vermin,
    Beast,
    Greenskin,
    Undead,
    Dark,
    Demon,
}

/// Would a member of faction `a` attack a member of faction `b`?
pub fn hostile(a: Faction, b: Faction) -> bool {
    use Faction::*;
    if a == b {
        return false;
    }
    match (a, b) {
        (Player, Ally) | (Ally, Player) => false,
        (Player, _) | (_, Player) => true,
        (Ally, _) | (_, Ally) => true,
        (Demon, _) | (_, Demon) => true,
        (Undead, Vermin) | (Vermin, Undead) => false,
        (Undead, _) | (_, Undead) => true,
        (Greenskin, Beast) | (Beast, Greenskin) => true,
        (Greenskin, Dark) | (Dark, Greenskin) => true,
        (Beast, Vermin) | (Vermin, Beast) => true,
        _ => false,
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Player,
    Mon(u32),
}

/// Who caused some damage.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Src {
    Player,
    Mon(u32),
    Env,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Monster {
    pub id: u32,
    pub kind: usize,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub faction: Faction,
    pub next_act: f64,
    pub awake: bool,
    pub target: Option<Target>,
    pub last_known: Option<(i32, i32)>,
    pub stun: i32,
    pub fear: i32,
    pub alive: bool,
    pub summoned: bool,
    /// Won a fight with another monster and grew stronger.
    pub champion: bool,
    /// A champion that slew another champion: stronger still.
    pub overlord: bool,
    /// A berserker unique below half health.
    pub raged: bool,
    /// Mid-charge: the next blow lands harder.
    pub charging: bool,
    /// World time at which its special ability can be used again.
    pub abil_cd: f64,
    // presentation state
    pub rx: f32,
    pub ry: f32,
    pub flash: f32,
    pub lunge: (f32, f32, f32),
}

impl Monster {
    pub fn new(id: u32, kind: usize, x: i32, y: i32, time: f64) -> Monster {
        let d = &MONSTERS[kind];
        Monster {
            id,
            kind,
            x,
            y,
            hp: d.hp,
            max_hp: d.hp,
            faction: d.faction,
            next_act: time,
            awake: false,
            target: None,
            last_known: None,
            stun: 0,
            fear: 0,
            alive: true,
            summoned: false,
            champion: false,
            overlord: false,
            raged: false,
            charging: false,
            abil_cd: time + 300.0,
            rx: x as f32,
            ry: y as f32,
            flash: 0.0,
            lunge: (0.0, 0.0, 0.0),
        }
    }

    pub fn def(&self) -> &'static MonsterDef {
        &MONSTERS[self.kind]
    }

    /// Swell into a champion: more health (speed and damage come from the flag).
    pub fn promote_champion(&mut self) {
        self.champion = true;
        self.max_hp = (self.max_hp as f32 * 1.6).round() as i32 + 4;
        self.hp = self.max_hp;
    }

    /// Rise from champion to overlord.
    pub fn promote_overlord(&mut self) {
        if !self.champion {
            self.promote_champion();
        }
        self.overlord = true;
        self.max_hp = (self.max_hp as f32 * 1.5).round() as i32 + 10;
        self.hp = self.max_hp;
    }

    /// Scale health for the run's difficulty.
    pub fn scale_hp(&mut self, mult: f32) {
        if mult != 1.0 {
            self.max_hp = (self.max_hp as f32 * mult).round() as i32;
            self.hp = (self.hp as f32 * mult).round().max(1.0) as i32;
        }
    }

    pub fn is_unique(&self) -> bool {
        self.def().unique > 0
    }

    /// Ticks per action, after champion and rage bonuses.
    pub fn speed(&self) -> f64 {
        let mut s = self.def().speed;
        if self.overlord {
            s *= 0.65;
        } else if self.champion {
            s *= 0.75;
        }
        if self.raged {
            s *= 0.75;
        }
        s
    }

    /// Damage multiplier from champion status and rage.
    pub fn dmg_mult(&self) -> f32 {
        let mut k = 1.0;
        if self.overlord {
            k *= 1.9;
        } else if self.champion {
            k *= 1.4;
        }
        if self.raged {
            k *= 1.5;
        }
        if self.charging {
            k *= 1.5;
        }
        k
    }

    /// The monster's name for messages, e.g. "champion orc".
    pub fn label(&self) -> String {
        if self.overlord {
            format!("overlord {}", self.def().name)
        } else if self.champion {
            format!("champion {}", self.def().name)
        } else {
            self.def().name.to_string()
        }
    }

    /// "the champion orc", or the proper name of a boss or unique.
    pub fn the(&self) -> String {
        let d = self.def();
        if d.boss || d.unique > 0 {
            d.name.to_string()
        } else {
            format!("the {}", self.label())
        }
    }
}

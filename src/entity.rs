//! Monsters, factions and the hostility table that drives monster infighting.

use crate::data::{MonsterDef, MONSTERS};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

#[derive(Clone, Debug)]
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
            rx: x as f32,
            ry: y as f32,
            flash: 0.0,
            lunge: (0.0, 0.0, 0.0),
        }
    }

    pub fn def(&self) -> &'static MonsterDef {
        &MONSTERS[self.kind]
    }
}

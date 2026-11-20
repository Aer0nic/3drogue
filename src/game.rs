//! Core game state and rules: the turn / real-time clock, player actions,
//! combat, skills, monster AI (including infighting), corpse revival, level
//! persistence, and how sound propagates through the dungeon.

use crate::audio::{Mood, Sfx, Spatial};
use crate::color::Rgb;
use crate::data::*;
use crate::entity::*;
use crate::fx::{BoltEnd, Fx};
use crate::map::*;
use crate::profile::Bonuses;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use std::collections::HashMap;
use std::f32::consts::PI;

/// In real-time mode, how many world ticks pass per second (100 ticks = 1 turn).
pub const TICKS_PER_SEC: f64 = 230.0;
pub const BASE_SIGHT: i32 = 9;
pub const MAX_INV: usize = 18;
pub const GREY: Rgb = Rgb(150, 150, 150);

pub fn facing_angle(f: i32) -> f32 {
    -PI / 2.0 + f as f32 * PI / 4.0
}

pub fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn a_name(kind: usize) -> String {
    let d = &MONSTERS[kind];
    if d.boss {
        return d.name.to_string();
    }
    let first = d.name.chars().next().unwrap_or('x');
    let art = if "aeiou".contains(first) { "an" } else { "a" };
    format!("{} {}", art, d.name)
}

pub fn the_name(kind: usize) -> String {
    let d = &MONSTERS[kind];
    if d.boss {
        d.name.to_string()
    } else {
        format!("the {}", d.name)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Move in an absolute direction; `true` also turns to face it.
    Move(i32, bool),
    Turn(i32),
    Attack,
    Wait,
    Pickup,
    Stairs,
    Skill(usize),
    Use(usize),
    Drop(usize),
    QuickHeal,
}

#[derive(Clone, Debug)]
pub enum DeathCause {
    Monster(usize),
    Spell(usize),
    OwnFire,
    Poison(String),
    Trap(TrapKind),
    Abandoned,
}

impl DeathCause {
    pub fn describe(&self) -> String {
        match self {
            DeathCause::Monster(k) => format!("slain by {}", a_name(*k)),
            DeathCause::Spell(k) => format!("blasted by the sorcery of {}", a_name(*k)),
            DeathCause::OwnFire => "burned by their own fireball".to_string(),
            DeathCause::Poison(s) => format!("poisoned by {}", s),
            DeathCause::Trap(t) => match t {
                TrapKind::Spikes => "impaled on a spike trap",
                TrapKind::Gas => "choked by poison gas",
                TrapKind::Fire => "immolated by a fire trap",
                TrapKind::Alarm => "undone by an alarm trap",
            }
            .to_string(),
            DeathCause::Abandoned => "lost their nerve and fled into madness".to_string(),
        }
    }
}

pub enum Outcome {
    Died(DeathCause),
    Won,
}

pub struct Msg {
    pub text: String,
    pub color: Rgb,
    pub age: f32,
}

pub struct SoundEv {
    pub sfx: Sfx,
    pub x: f32,
    pub y: f32,
    pub vol: f32,
}

/// Smoothly interpolated camera following the grid-locked player.
pub struct Cam {
    pub x: f32,
    pub y: f32,
    pub ang: f32,
    pub walk: f32,
    pub swing: f32,
}

pub struct Player {
    pub x: i32,
    pub y: i32,
    pub facing: i32,
    pub class: usize,
    pub hp: i32,
    pub max_hp: i32,
    pub mp: i32,
    pub max_mp: i32,
    pub level: i32,
    pub xp: i32,
    pub wpn: Gear,
    pub arm: Gear,
    pub inv: Vec<Item>,
    pub next_act: f64,
    pub poison: i32,
    pub poison_src: String,
    pub invis: i32,
    pub ward: i32,
    pub cds: HashMap<Skill, f64>,
    pub has_amulet: bool,
    pub regen_ctr: i32,
    pub mp_ctr: i32,
    pub extra_dmg: i32,
}

pub struct Game {
    pub rng: StdRng,
    pub depth: u32,
    pub level: Level,
    /// Levels you have left, remembered for the rest of the run.
    pub stash: HashMap<u32, Level>,
    pub p: Player,
    pub bonus: Bonuses,
    pub time: f64,
    pub next_tick: f64,
    pub turns: u64,
    pub realtime: bool,
    pub overhead: bool,
    pub msgs: Vec<Msg>,
    pub sounds: Vec<SoundEv>,
    pub fx: Fx,
    pub dmap: Vec<i32>,
    pub next_id: u32,
    pub over: Option<Outcome>,
    pub kills: u32,
    pub max_depth: u32,
    pub cam: Cam,
    pub queued: Option<Action>,
    pub combat_heat: f32,
    pub room_size: f32,
    last_fight_msg: u64,
}

// ---------------------------------------------------------------- population

pub fn random_item(depth: u32, rng: &mut StdRng, luck: i32) -> Item {
    let r: f32 = rng.gen();
    if r < 0.40 {
        let p: f32 = rng.gen();
        Item::Potion(if p < 0.5 {
            Potion::Healing
        } else if p < 0.7 {
            Potion::Mana
        } else if p < 0.82 {
            Potion::Cure
        } else if p < 0.91 {
            Potion::Might
        } else {
            Potion::Vigor
        })
    } else if r < 0.60 {
        let s: f32 = rng.gen();
        Item::Scroll(if s < 0.4 {
            Scroll::Teleport
        } else if s < 0.75 {
            Scroll::Mapping
        } else {
            Scroll::Fire
        })
    } else if r < 0.82 {
        let opts: Vec<usize> = (0..WEAPONS.len()).filter(|&i| WEAPONS[i].depth <= depth + 1).collect();
        let def = *opts.choose(rng).unwrap_or(&1);
        let mut bonus = rng.gen_range(0..=(depth as i32 / 3 + luck / 2));
        if rng.gen_bool(0.1) {
            bonus = -1;
        }
        Item::Weapon(Gear { def, bonus })
    } else {
        let opts: Vec<usize> = (0..ARMORS.len()).filter(|&i| ARMORS[i].depth <= depth + 1).collect();
        let def = *opts.choose(rng).unwrap_or(&2);
        let bonus = rng.gen_range(0..=(depth as i32 / 4 + luck / 2));
        Item::Armor(Gear { def, bonus })
    }
}

fn populate(lv: &mut Level, rng: &mut StdRng, next_id: &mut u32, luck: i32, time: f64) {
    let depth = lv.depth;
    let kinds: Vec<usize> = (0..MONSTERS.len())
        .filter(|&i| {
            let d = &MONSTERS[i];
            !d.boss && depth >= d.depth.0 && depth <= d.depth.1
        })
        .collect();
    let n_rooms = lv.rooms.len();
    let target = (6 + depth as usize * 2).min(26);
    let mut placed = 0;
    let mut guard = 0;
    while placed < target && guard < 400 {
        guard += 1;
        let ri = rng.gen_range(1..n_rooms);
        let room = lv.rooms[ri];
        let kind = *kinds.choose(rng).unwrap_or(&0);
        let pack = if matches!(MONSTERS[kind].glyph, 'r' | 'k' | 'g' | 'w' | 's' | 'b') && rng.gen_bool(0.35) {
            rng.gen_range(2..4)
        } else {
            1
        };
        let awake = rng.gen_bool(0.25);
        for _ in 0..pack {
            if let Some((x, y)) = lv.random_floor_in(&room, rng) {
                let mut m = Monster::new(*next_id, kind, x, y, time);
                *next_id += 1;
                m.awake = awake;
                let extra = rng.gen_range(0..=(depth as i32 / 2));
                m.hp += extra;
                m.max_hp += extra;
                lv.monsters.push(m);
                placed += 1;
            }
        }
    }

    // Level 10: the Warden guards the Amulet in the deepest room.
    if depth == 10 {
        let (dx, dy) = lv.down;
        let mut amulet_pos = (dx, dy);
        for &(ox, oy) in DIRS.iter() {
            if lv.tile(dx + ox, dy + oy) == Tile::Floor {
                amulet_pos = (dx + ox, dy + oy);
                break;
            }
        }
        lv.items.push(FloorItem { x: amulet_pos.0, y: amulet_pos.1, item: Item::Amulet });
        let room = lv.rooms.iter().find(|r| r.contains(dx, dy)).copied();
        let pos = room
            .and_then(|r| lv.random_floor_in(&r, rng))
            .unwrap_or_else(|| lv.random_floor(rng));
        let mut w = Monster::new(*next_id, M_WARDEN, pos.0, pos.1, time);
        *next_id += 1;
        w.awake = false;
        lv.monsters.push(w);
    }

    let n_items = 5 + rng.gen_range(0..4) + luck as usize;
    for _ in 0..n_items {
        let (x, y) = lv.random_floor(rng);
        let it = random_item(depth, rng, luck);
        lv.items.push(FloorItem { x, y, item: it });
    }

    let n_traps = 2 + depth / 2;
    for _ in 0..n_traps {
        let (x, y) = lv.random_floor(rng);
        if lv.rooms[0].contains(x, y) || lv.items.iter().any(|i| i.x == x && i.y == y) {
            continue;
        }
        let kind = match rng.gen_range(0..10) {
            0..=3 => TrapKind::Spikes,
            4..=5 => TrapKind::Gas,
            6..=7 => TrapKind::Fire,
            _ => TrapKind::Alarm,
        };
        lv.traps.push(Trap { x, y, kind, revealed: false });
    }
}

// ---------------------------------------------------------------- game

impl Game {
    pub fn new(class: usize, bonus: Bonuses) -> Game {
        let seed: u64 = rand::thread_rng().gen();
        let mut rng = StdRng::seed_from_u64(seed);
        let mut next_id = 1u32;
        let mut level = generate(1, &mut rng);
        populate(&mut level, &mut rng, &mut next_id, bonus.luck, 0.0);
        let cd = &CLASSES[class];
        let mut inv = Vec::new();
        for _ in 0..(1 + bonus.potions) {
            inv.push(Item::Potion(Potion::Healing));
        }
        if cd.mp >= 14 {
            inv.push(Item::Potion(Potion::Mana));
        }
        let p = Player {
            x: level.up.0,
            y: level.up.1,
            facing: 2,
            class,
            hp: cd.hp + bonus.hp,
            max_hp: cd.hp + bonus.hp,
            mp: cd.mp + bonus.mp,
            max_mp: cd.mp + bonus.mp,
            level: 1,
            xp: 0,
            wpn: Gear { def: cd.weapon, bonus: 0 },
            arm: Gear { def: cd.armor, bonus: 0 },
            inv,
            next_act: 0.0,
            poison: 0,
            poison_src: String::new(),
            invis: 0,
            ward: 0,
            cds: HashMap::new(),
            has_amulet: false,
            regen_ctr: 0,
            mp_ctr: 0,
            extra_dmg: 0,
        };
        let mut g = Game {
            rng,
            depth: 1,
            level,
            stash: HashMap::new(),
            p,
            bonus,
            time: 0.0,
            next_tick: 100.0,
            turns: 0,
            realtime: false,
            overhead: false,
            msgs: Vec::new(),
            sounds: Vec::new(),
            fx: Fx::new(),
            dmap: Vec::new(),
            next_id,
            over: None,
            kills: 0,
            max_depth: 1,
            cam: Cam { x: 0.0, y: 0.0, ang: 0.0, walk: 0.0, swing: 0.0 },
            queued: None,
            combat_heat: 0.0,
            room_size: 0.4,
            last_fight_msg: 0,
        };
        g.enter_level();
        g.msg(
            format!("You descend into the dungeon of GRAZZARK as a {}.", cd.name),
            Rgb(220, 200, 150),
        );
        g.msg(
            "Claim the Amulet on level 10 and return to the surface. Press ? for help.",
            Rgb(170, 170, 170),
        );
        g
    }

    // ------------------------------------------------------------ stats

    pub fn cdef(&self) -> &'static ClassDef {
        &CLASSES[self.p.class]
    }
    pub fn wdef(&self) -> &'static WeaponDef {
        &WEAPONS[self.p.wpn.def]
    }
    pub fn sight(&self) -> i32 {
        BASE_SIGHT + self.bonus.sight
    }
    pub fn hit_bonus(&self) -> i32 {
        self.cdef().hit + self.p.level + self.wdef().hit + self.p.wpn.bonus
    }
    pub fn player_ac(&self) -> i32 {
        ARMORS[self.p.arm.def].ac + self.p.arm.bonus + self.bonus.ac
    }
    pub fn dmg_bonus(&self) -> i32 {
        self.p.wpn.bonus + self.p.extra_dmg + self.bonus.dmg + self.p.level / 3
    }
    pub fn crit_chance(&self) -> f32 {
        self.wdef().crit + self.bonus.crit + if self.p.class == C_ROGUE { 0.08 } else { 0.0 }
    }
    pub fn skills(&self) -> Vec<Skill> {
        let mut v = self.cdef().skills.to_vec();
        v.extend(self.bonus.skills.iter().copied());
        v
    }
    pub fn xp_needed(level: i32) -> i32 {
        (15.0 * (level as f32).powf(1.6)) as i32
    }
    pub fn skill_ready_in(&self, s: Skill) -> f64 {
        match self.p.cds.get(&s) {
            Some(&t) if t > self.time => ((t - self.time) / 100.0).ceil(),
            _ => 0.0,
        }
    }

    // ------------------------------------------------------------ helpers

    pub fn msg(&mut self, t: impl Into<String>, c: Rgb) {
        self.msgs.push(Msg { text: t.into(), color: c, age: 0.0 });
        if self.msgs.len() > 80 {
            self.msgs.remove(0);
        }
    }

    pub fn emit(&mut self, sfx: Sfx, x: i32, y: i32, vol: f32) {
        self.sounds.push(SoundEv { sfx, x: x as f32 + 0.5, y: y as f32 + 0.5, vol });
    }

    fn new_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn mon_index(&self, id: u32) -> Option<usize> {
        self.level.monsters.iter().position(|m| m.alive && m.id == id)
    }

    fn target_pos(&self, t: Target) -> (i32, i32) {
        match t {
            Target::Player => (self.p.x, self.p.y),
            Target::Mon(id) => match self.mon_index(id) {
                Some(i) => (self.level.monsters[i].x, self.level.monsters[i].y),
                None => (-999, -999),
            },
        }
    }

    pub fn refresh_view(&mut self) {
        let s = self.sight();
        let (px, py) = (self.p.x, self.p.y);
        self.level.compute_fov(px, py, s);
        self.dmap = self.level.dijkstra(px, py);
        let mut open = 0;
        let mut tot = 0;
        for dy in -4..=4 {
            for dx in -4..=4 {
                tot += 1;
                if !self.level.tile(px + dx, py + dy).opaque() {
                    open += 1;
                }
            }
        }
        self.room_size = open as f32 / tot as f32;
    }

    fn enter_level(&mut self) {
        let t = self.time;
        for m in self.level.monsters.iter_mut() {
            m.next_act = t + self.rng.gen_range(10.0..100.0);
            m.rx = m.x as f32;
            m.ry = m.y as f32;
        }
        let (px, py) = (self.p.x, self.p.y);
        for f in [2, 4, 6, 0, 1, 3, 5, 7] {
            let (dx, dy) = DIRS[f as usize];
            if self.level.tile(px + dx, py + dy).walkable() {
                self.p.facing = f;
                break;
            }
        }
        self.fx.clear();
        self.refresh_view();
        self.cam.x = px as f32 + 0.5;
        self.cam.y = py as f32 + 0.5;
        self.cam.ang = facing_angle(self.p.facing);
    }

    /// Wake monsters that can hear a noise. Noise from the player travels
    /// along corridors (path distance); other noise uses straight distance.
    fn noise(&mut self, x: i32, y: i32, radius: i32) {
        let from_player = x == self.p.x && y == self.p.y;
        for i in 0..self.level.monsters.len() {
            let (mx, my, alive, fac) = {
                let m = &self.level.monsters[i];
                (m.x, m.y, m.alive, m.faction)
            };
            if !alive || fac == Faction::Ally {
                continue;
            }
            let d = if from_player { self.dmap[self.level.idx(mx, my)] } else { cheb(x, y, mx, my) };
            if d <= radius {
                let wake = self.rng.gen_bool(0.75);
                let m = &mut self.level.monsters[i];
                if wake {
                    m.awake = true;
                }
                if m.awake && m.target.is_none() {
                    m.last_known = Some((x, y));
                }
            }
        }
    }

    fn monsters_within(&self, x: i32, y: i32, r: f32, hostile_only: bool) -> Vec<usize> {
        self.level
            .monsters
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                m.alive
                    && dist(x, y, m.x, m.y) <= r
                    && (!hostile_only || m.faction != Faction::Ally)
                    && self.level.los(x, y, m.x, m.y)
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn free_adjacent(&mut self, x: i32, y: i32) -> Option<(i32, i32)> {
        let mut dirs = DIRS.to_vec();
        dirs.shuffle(&mut self.rng);
        for (dx, dy) in dirs {
            let (nx, ny) = (x + dx, y + dy);
            if self.level.tile(nx, ny).walkable()
                && self.level.can_step(x, y, dx, dy)
                && self.level.monster_at(nx, ny).is_none()
                && !(nx == self.p.x && ny == self.p.y)
            {
                return Some((nx, ny));
            }
        }
        None
    }

    /// Walk a line from (x0,y0) toward (x1,y1). Returns impact tile and
    /// what was hit. `shooter` None means the player is shooting.
    fn trace(&self, x0: i32, y0: i32, x1: i32, y1: i32, max: i32, shooter: Option<u32>) -> (i32, i32, Option<Target>) {
        let pts = line(x0, y0, x1, y1);
        let mut last = (x0, y0);
        for (i, &(x, y)) in pts.iter().enumerate().skip(1) {
            if i as i32 > max {
                break;
            }
            if self.level.tile(x, y).opaque() {
                return (last.0, last.1, None);
            }
            if x == self.p.x && y == self.p.y && shooter.is_some() {
                return (x, y, Some(Target::Player));
            }
            if let Some(oi) = self.level.monster_at(x, y) {
                let o = &self.level.monsters[oi];
                let ally_pass = shooter.is_none() && o.faction == Faction::Ally;
                if Some(o.id) != shooter && !ally_pass {
                    return (x, y, Some(Target::Mon(o.id)));
                }
            }
            last = (x, y);
        }
        (last.0, last.1, None)
    }

    fn fire_line(&self, dir: i32, range: i32) -> (i32, i32, Option<usize>) {
        let (dx, dy) = DIRS[dir.rem_euclid(8) as usize];
        let (px, py) = (self.p.x, self.p.y);
        let (hx, hy, hit) = self.trace(px, py, px + dx * range, py + dy * range, range, None);
        let mi = match hit {
            Some(Target::Mon(id)) => self.mon_index(id),
            _ => None,
        };
        (hx, hy, mi)
    }

    // ------------------------------------------------------------ clock

    /// Entry point for player input.
    pub fn do_action(&mut self, a: Action) {
        if self.over.is_some() {
            return;
        }
        if self.realtime {
            if let Action::Turn(_) = a {
                self.player_action(a);
                return;
            }
            if self.p.next_act > self.time {
                self.queued = Some(a);
                return;
            }
            if let Some(c) = self.player_action(a) {
                self.p.next_act = self.time + c;
            }
        } else {
            if self.p.next_act < self.time {
                self.p.next_act = self.time;
            }
            if let Some(c) = self.player_action(a) {
                let t = self.time + c;
                self.p.next_act = t;
                self.advance_to(t);
            }
        }
    }

    pub fn toggle_realtime(&mut self) {
        self.realtime = !self.realtime;
        self.queued = None;
        if self.realtime {
            self.msg("REAL-TIME combat engaged. The dungeon no longer waits for you!", Rgb(255, 120, 80));
        } else {
            self.msg("Turn-based mode. Time moves only when you act.", Rgb(120, 200, 255));
        }
    }

    /// Process every monster action and turn tick due before world time `t`.
    fn advance_to(&mut self, t: f64) {
        let mut guard = 0;
        loop {
            guard += 1;
            if self.over.is_some() || guard > 5000 {
                break;
            }
            let mut best: Option<(usize, f64)> = None;
            for (i, m) in self.level.monsters.iter().enumerate() {
                if m.alive && m.next_act <= t && best.map_or(true, |b| m.next_act < b.1) {
                    best = Some((i, m.next_act));
                }
            }
            let tick_due = self.next_tick <= t;
            match best {
                Some((i, at)) if !tick_due || at < self.next_tick => {
                    self.time = self.time.max(at);
                    self.monster_act(i);
                }
                _ if tick_due => {
                    self.time = self.time.max(self.next_tick);
                    self.turn_tick();
                    self.next_tick += 100.0;
                }
                _ => break,
            }
        }
        if self.over.is_none() {
            self.time = self.time.max(t);
        }
        self.level.monsters.retain(|m| m.alive);
    }

    /// Once per 100 ticks: regeneration, poison, buffs and corpse revival.
    fn turn_tick(&mut self) {
        self.turns += 1;
        let (px, py) = (self.p.x, self.p.y);

        if self.p.poison > 0 {
            self.p.poison -= 1;
            if self.turns % 2 == 0 {
                self.p.hp -= 1;
                if self.p.hp <= 0 {
                    self.p.hp = 0;
                    let src = self.p.poison_src.clone();
                    self.die(DeathCause::Poison(src));
                    return;
                }
            }
            if self.p.poison == 0 {
                self.msg("The poison wears off.", Rgb(150, 255, 150));
            }
        }
        self.p.regen_ctr += 1;
        let period = (10 - self.bonus.regen * 2 - self.p.level / 4).max(2);
        if self.p.regen_ctr >= period {
            self.p.regen_ctr = 0;
            if self.p.hp < self.p.max_hp && self.p.poison == 0 {
                self.p.hp += 1;
            }
        }
        self.p.mp_ctr += 1;
        let base_period: i32 = if self.p.class == C_MAGE || self.p.class == C_NECRO { 2 } else { 4 };
        let staff_bonus: i32 = if WEAPONS[self.p.wpn.def].art == WArt::Staff { 1 } else { 0 };
        let mperiod = base_period - staff_bonus;
        if self.p.mp_ctr >= mperiod.max(1) {
            self.p.mp_ctr = 0;
            if self.p.mp < self.p.max_mp {
                self.p.mp += 1;
            }
        }
        if self.p.invis > 0 {
            self.p.invis -= 1;
            if self.p.invis == 0 {
                self.msg("You fade back into view.", Rgb(150, 150, 200));
            }
        }

        let turns = self.turns;
        for m in self.level.monsters.iter_mut() {
            if m.alive && m.def().regen && m.hp < m.max_hp && turns % 2 == 0 {
                m.hp = (m.hp + 2).min(m.max_hp);
            }
        }

        // The dead do not rest in GRAZZARK's halls.
        let mut rise = Vec::new();
        for (ci, c) in self.level.corpses.iter().enumerate() {
            if c.revive_at > 0.0
                && c.revive_at <= self.time
                && !self.level.is_visible(c.x, c.y)
                && self.level.monster_at(c.x, c.y).is_none()
                && !(c.x == px && c.y == py)
            {
                rise.push(ci);
            }
        }
        for &ci in rise.iter().rev() {
            let c = self.level.corpses.remove(ci);
            let id = self.new_id();
            let mut m = Monster::new(id, c.kind, c.x, c.y, self.time + 100.0);
            m.awake = self.rng.gen_bool(0.5);
            self.level.monsters.push(m);
            self.emit(Sfx::Revive, c.x, c.y, 0.9);
            if cheb(c.x, c.y, px, py) < 16 {
                let t = if MONSTERS[c.kind].death == DeathKind::Undead {
                    "You hear bones knitting together in the darkness..."
                } else {
                    "Something stirs in the darkness... the dead rise again."
                };
                self.msg(t, Rgb(170, 140, 200));
            }
        }
    }

    /// Per-frame update: effects, camera, and the real-time clock.
    pub fn update(&mut self, dt: f32, paused: bool) {
        self.fx.update(dt);

        let tx = self.p.x as f32 + 0.5;
        let ty = self.p.y as f32 + 0.5;
        let k = (dt * 11.0).min(1.0);
        let (ox, oy) = (self.cam.x, self.cam.y);
        self.cam.x += (tx - self.cam.x) * k;
        self.cam.y += (ty - self.cam.y) * k;
        let moved = ((self.cam.x - ox).powi(2) + (self.cam.y - oy).powi(2)).sqrt();
        self.cam.walk += moved * 7.0;
        let ta = facing_angle(self.p.facing);
        let mut d = ta - self.cam.ang;
        while d > PI {
            d -= 2.0 * PI;
        }
        while d < -PI {
            d += 2.0 * PI;
        }
        self.cam.ang += d * (dt * 13.0).min(1.0);
        self.cam.swing = (self.cam.swing - dt * 4.0).max(0.0);

        for m in self.level.monsters.iter_mut() {
            m.rx += (m.x as f32 - m.rx) * k;
            m.ry += (m.y as f32 - m.ry) * k;
            m.flash = (m.flash - dt * 4.0).max(0.0);
            m.lunge.2 = (m.lunge.2 - dt * 5.0).max(0.0);
        }
        for msg in self.msgs.iter_mut() {
            msg.age += dt;
        }
        self.combat_heat = (self.combat_heat - dt * 0.06).max(0.0);

        if self.realtime && !paused && self.over.is_none() {
            let target = self.time + dt as f64 * TICKS_PER_SEC;
            self.advance_to(target);
            if let Some(a) = self.queued {
                if self.p.next_act <= self.time {
                    self.queued = None;
                    self.do_action(a);
                }
            }
        }
    }

    // ------------------------------------------------------------ player

    fn player_action(&mut self, a: Action) -> Option<f64> {
        match a {
            Action::Turn(d) => {
                self.p.facing = (self.p.facing + d).rem_euclid(8);
                None
            }
            Action::Move(dir, face) => {
                let dir = dir.rem_euclid(8);
                if face {
                    self.p.facing = dir;
                }
                self.player_move(dir)
            }
            Action::Attack => self.player_attack(),
            Action::Wait => {
                self.search_traps(0.3);
                Some(100.0)
            }
            Action::Pickup => self.pickup(),
            Action::Stairs => self.use_stairs(),
            Action::Skill(i) => {
                let sk = self.skills();
                if i < sk.len() {
                    self.use_skill(sk[i])
                } else {
                    self.msg("You have no skill in that slot. Buy more at the Soul Altar.", GREY);
                    None
                }
            }
            Action::Use(i) => self.use_item(i),
            Action::Drop(i) => self.drop_item(i),
            Action::QuickHeal => {
                if let Some(i) = self.p.inv.iter().position(|it| *it == Item::Potion(Potion::Healing)) {
                    self.use_item(i)
                } else {
                    self.msg("You have no Potions of Healing!", Rgb(255, 90, 90));
                    None
                }
            }
        }
    }

    fn player_move(&mut self, dir: i32) -> Option<f64> {
        let (dx, dy) = DIRS[dir as usize];
        let (px, py) = (self.p.x, self.p.y);
        let (nx, ny) = (px + dx, py + dy);
        if !self.level.can_step(px, py, dx, dy) {
            self.fx.shake = self.fx.shake.max(0.15);
            return None;
        }
        if let Some(mi) = self.level.monster_at(nx, ny) {
            if self.level.monsters[mi].faction == Faction::Ally {
                let m = &mut self.level.monsters[mi];
                m.x = px;
                m.y = py;
                self.p.x = nx;
                self.p.y = ny;
                self.after_player_move();
                return Some(100.0);
            }
            return Some(self.player_melee(mi));
        }
        match self.level.tile(nx, ny) {
            Tile::DoorClosed => {
                self.level.set(nx, ny, Tile::DoorOpen);
                self.emit(Sfx::Creak, nx, ny, 0.8);
                self.msg("The door creaks open.", GREY);
                self.refresh_view();
                self.noise(px, py, 4);
                Some(100.0)
            }
            t if t.walkable() => {
                self.p.x = nx;
                self.p.y = ny;
                self.emit(Sfx::Step, nx, ny, 0.5);
                self.after_player_move();
                Some(100.0)
            }
            _ => {
                self.fx.shake = self.fx.shake.max(0.15);
                None
            }
        }
    }

    fn search_traps(&mut self, chance: f64) {
        let (px, py) = (self.p.x, self.p.y);
        let mut found = false;
        for t in self.level.traps.iter_mut() {
            if !t.revealed && cheb(t.x, t.y, px, py) <= 1 && self.rng.gen::<f64>() < chance {
                t.revealed = true;
                found = true;
            }
        }
        if found {
            self.msg("You spot a hidden trap!", Rgb(255, 160, 60));
        }
    }

    fn after_player_move(&mut self) {
        self.refresh_view();
        let (px, py) = (self.p.x, self.p.y);
        self.check_trap(px, py);
        if self.over.is_some() {
            return;
        }
        let chance = if self.p.class == C_ROGUE { 0.35 } else { 0.12 };
        self.search_traps(chance);
        let here: Vec<String> = self
            .level
            .items
            .iter()
            .filter(|i| i.x == px && i.y == py)
            .map(|i| i.item.name())
            .collect();
        if !here.is_empty() {
            self.msg(format!("You see here: {}. (g to pick up)", here.join(", ")), Rgb(220, 210, 120));
        }
        match self.level.tile(px, py) {
            Tile::StairsDown => self.msg("A stairway leads down into darkness. (> or Enter)", Rgb(200, 150, 255)),
            Tile::StairsUp => self.msg("A stairway leads up. (< or Enter)", Rgb(150, 200, 255)),
            _ => {}
        }
        let r = (3 - self.cdef().stealth).max(1);
        self.noise(px, py, r);
    }

    fn check_trap(&mut self, x: i32, y: i32) {
        let Some(ti) = self.level.traps.iter().position(|t| t.x == x && t.y == y) else { return };
        let (kind, revealed) = (self.level.traps[ti].kind, self.level.traps[ti].revealed);
        if revealed && self.rng.gen_bool(0.75) {
            self.msg("You carefully step around the trap.", GREY);
            return;
        }
        self.level.traps[ti].revealed = true;
        self.emit(Sfx::Trap, x, y, 1.0);
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let d = self.depth as i32;
        match kind {
            TrapKind::Spikes => {
                let dmg = self.rng.gen_range(2..=8) + d;
                self.msg(format!("Spikes burst from the floor! ({} damage)", dmg), Rgb(255, 90, 90));
                self.fx.blood(fx, fy, 0.2, 14);
                self.player_hit(dmg, DeathCause::Trap(TrapKind::Spikes), true);
            }
            TrapKind::Gas => {
                self.msg("A cloud of noxious gas erupts around you!", Rgb(140, 255, 90));
                self.fx.mist(fx, fy, Rgb(120, 220, 60));
                self.emit(Sfx::Voice(VoiceKind::Hiss, Mood::Alert), x, y, 0.9);
                self.p.poison += 6 + d;
                self.p.poison_src = "a poison gas trap".to_string();
                self.fx.flash(Rgb(80, 200, 40), 0.4);
            }
            TrapKind::Fire => {
                let dmg = self.rng.gen_range(3..=12) + d;
                self.msg(format!("Flames roar up around you! ({} damage)", dmg), Rgb(255, 140, 40));
                self.fx.explosion(fx, fy, Rgb(255, 120, 20));
                self.emit(Sfx::Explosion, x, y, 1.0);
                self.player_hit(dmg, DeathCause::Trap(TrapKind::Fire), false);
            }
            TrapKind::Alarm => {
                self.msg("A shrill alarm shrieks through the halls!", Rgb(255, 220, 80));
                self.emit(Sfx::Clang, x, y, 1.0);
                self.emit(Sfx::Clang, x, y, 1.0);
                for m in self.level.monsters.iter_mut() {
                    if m.alive && m.faction != Faction::Ally && self.dmap[(m.y * MAP_W + m.x) as usize] <= 30 {
                        m.awake = true;
                        m.last_known = Some((x, y));
                    }
                }
            }
        }
    }

    fn player_attack(&mut self) -> Option<f64> {
        let f = self.p.facing;
        let w = self.wdef();
        if w.range > 0 {
            self.shoot_arrow(f);
            return Some(w.speed);
        }
        let (dx, dy) = DIRS[f as usize];
        let (px, py) = (self.p.x, self.p.y);
        let (nx, ny) = (px + dx, py + dy);
        if self.level.can_step(px, py, dx, dy) {
            if let Some(mi) = self.level.monster_at(nx, ny) {
                if self.level.monsters[mi].faction == Faction::Ally {
                    self.msg("You will not strike your companion.", GREY);
                    return None;
                }
                return Some(self.player_melee(mi));
            }
        }
        if self.level.tile(nx, ny) == Tile::DoorClosed {
            return self.player_move(f);
        }
        self.cam.swing = 1.0;
        self.emit(Sfx::Swing, px, py, 0.6);
        Some(w.speed)
    }

    fn player_melee(&mut self, mi: usize) -> f64 {
        self.cam.swing = 1.0;
        let (px, py) = (self.p.x, self.p.y);
        self.emit(Sfx::Swing, px, py, 0.5);
        let w = self.wdef();
        let mult = if w.range > 0 { 0.5 } else { 1.0 };
        self.player_strike(mi, mult, 0, None);
        self.noise(px, py, 7);
        self.combat_heat = 1.0;
        w.speed
    }

    fn shoot_arrow(&mut self, dir: i32) {
        let dir = dir.rem_euclid(8);
        let w = self.wdef();
        let range = w.range.max(7);
        let (px, py) = (self.p.x, self.p.y);
        let (hx, hy, mi) = self.fire_line(dir, range);
        let ch = ['|', '/', '-', '\\', '|', '/', '-', '\\'][dir as usize];
        let end = if mi.is_some() { BoltEnd::Blood } else { BoltEnd::Sparks(Rgb(200, 200, 200)) };
        self.fx.bolt(px as f32 + 0.5, py as f32 + 0.5, hx as f32 + 0.5, hy as f32 + 0.5, 22.0, ch, Rgb(225, 205, 160), None, end);
        self.emit(Sfx::Arrow, px, py, 0.7);
        if let Some(mi) = mi {
            let custom = if w.range > 0 { None } else { Some((2, 6)) };
            self.player_strike(mi, 1.0, 0, custom);
        }
        self.noise(px, py, 4);
        self.combat_heat = 1.0;
    }

    fn player_strike(&mut self, mi: usize, mult: f32, hit_mod: i32, custom: Option<(i32, i32)>) {
        let (kind, mx, my, unaware) = {
            let m = &self.level.monsters[mi];
            (m.kind, m.x, m.y, !m.awake || m.target != Some(Target::Player))
        };
        let def = &MONSTERS[kind];
        let name = def.name;
        let roll = self.rng.gen_range(1..=20);
        let hitb = self.hit_bonus() + hit_mod + if unaware { 4 } else { 0 };
        let tn = 10 + def.ac * 3 / 2;
        let (fx, fy) = (mx as f32 + 0.5, my as f32 + 0.5);
        if roll == 20 || (roll != 1 && roll + hitb >= tn) {
            let (lo, hi) = custom.unwrap_or(self.wdef().dmg);
            let mut dmg = self.rng.gen_range(lo..=hi) + self.dmg_bonus();
            dmg = ((dmg as f32) * mult).round() as i32;
            let crit = roll == 20 || self.rng.gen::<f32>() < self.crit_chance();
            let backstab = unaware && self.p.class == C_ROGUE;
            let mut col = Rgb(235, 235, 235);
            if backstab {
                dmg *= 3;
                col = Rgb(255, 80, 80);
            } else if crit {
                dmg *= 2;
                col = Rgb(255, 210, 60);
            }
            dmg = dmg.max(1);
            if backstab {
                self.msg(format!("You backstab the {} for {}!", name, dmg), Rgb(255, 120, 120));
            } else if crit {
                self.msg(format!("Critical hit! The {} takes {}!", name, dmg), Rgb(255, 210, 80));
            } else {
                self.msg(format!("You hit the {} for {}.", name, dmg), Rgb(215, 215, 215));
            }
            let w = self.wdef();
            if custom.is_none() && w.stun > 0.0 && self.rng.gen::<f32>() < w.stun && self.level.monsters[mi].hp > dmg {
                self.level.monsters[mi].stun = 2;
                self.msg(format!("The {} is stunned!", name), Rgb(200, 200, 255));
            }
            let sfx = if def.death == DeathKind::Undead || def.ac >= 5 { Sfx::Clang } else { Sfx::Hit };
            self.emit(sfx, mx, my, 0.9);
            self.fx.text(fx, fy, 0.9, dmg.to_string(), col);
            if crit || backstab {
                self.fx.shake = self.fx.shake.max(0.4);
            }
            self.damage_monster(mi, dmg, Src::Player);
        } else {
            self.msg(format!("You miss the {}.", name), GREY);
            self.emit(Sfx::Miss, mx, my, 0.5);
            self.fx.text(fx, fy, 0.9, "miss".to_string(), GREY);
        }
        let m = &mut self.level.monsters[mi];
        if m.alive && m.faction != Faction::Ally {
            m.awake = true;
            m.target = Some(Target::Player);
        }
    }

    /// Returns true if the monster died.
    pub fn damage_monster(&mut self, mi: usize, dmg: i32, src: Src) -> bool {
        let (kind, x, y, hp) = {
            let m = &mut self.level.monsters[mi];
            if !m.alive {
                return false;
            }
            m.hp -= dmg;
            m.flash = 1.0;
            m.awake = true;
            match src {
                Src::Player => {
                    if m.faction != Faction::Ally {
                        m.target = Some(Target::Player);
                    }
                }
                Src::Mon(id) => {
                    if id != m.id {
                        m.target = Some(Target::Mon(id));
                    }
                }
                Src::Env => {}
            }
            (m.kind, m.x, m.y, m.hp)
        };
        let def = &MONSTERS[kind];
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        match def.death {
            DeathKind::Undead => self.fx.sparks(fx, fy, 0.6, Rgb(230, 230, 210)),
            DeathKind::Demon => self.fx.ichor(fx, fy, 0.5, 6, Rgb(255, 120, 30)),
            _ => self.fx.blood(fx, fy, 0.5, 6),
        }
        if hp <= 0 {
            self.kill_monster(mi, src);
            true
        } else {
            if self.rng.gen_bool(0.4) {
                self.emit(Sfx::Voice(def.voice, Mood::Hurt), x, y, 0.7);
            }
            false
        }
    }

    fn kill_monster(&mut self, mi: usize, src: Src) {
        let (kind, x, y, faction, summoned) = {
            let m = &mut self.level.monsters[mi];
            m.alive = false;
            (m.kind, m.x, m.y, m.faction, m.summoned)
        };
        let def = &MONSTERS[kind];
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let ally = faction == Faction::Ally;
        if self.level.is_visible(x, y) {
            let label = if ally { format!("Your {}", def.name) } else { cap(&the_name(kind)) };
            let verb = if def.death == DeathKind::Undead { "is destroyed" } else { "dies" };
            let col = if ally { Rgb(255, 150, 100) } else { Rgb(255, 220, 140) };
            self.msg(format!("{} {}!", label, verb), col);
        }
        self.emit(Sfx::Voice(def.voice, Mood::Die), x, y, 1.0);
        match def.death {
            DeathKind::Undead => {
                self.fx.bones(fx, fy);
                self.emit(Sfx::Bones, x, y, 0.7);
            }
            DeathKind::Demon => {
                self.fx.ichor(fx, fy, 0.5, 20, Rgb(255, 110, 20));
                self.fx.burst(fx, fy, 0.4, 20, Rgb(80, 70, 70), &['o', '~', '.'], 0.6, 1.5, -0.3, false);
                self.fx.light(fx, fy, [1.5, 0.6, 0.2], 2.0, 0.5);
            }
            _ => self.fx.blood(fx, fy, 0.5, 22),
        }
        if def.death != DeathKind::Undead {
            let i = self.level.idx(x, y);
            self.level.blood[i] = (self.level.blood[i] + 1.0).min(3.0);
            for &(dx, dy) in DIRS.iter() {
                if self.rng.gen_bool(0.3) && self.level.inb(x + dx, y + dy) && !self.level.tile(x + dx, y + dy).opaque() {
                    let j = self.level.idx(x + dx, y + dy);
                    self.level.blood[j] = (self.level.blood[j] + 0.5).min(3.0);
                }
            }
        }
        if !def.boss {
            let revive_at = if def.revive.1 > 0.0 {
                self.time + self.rng.gen_range(def.revive.0..def.revive.1) * 100.0
            } else {
                -1.0
            };
            self.level.corpses.push(Corpse { kind, x, y, revive_at });
        }
        let by_ally = match src {
            Src::Mon(id) => self.level.monsters.iter().any(|m| m.id == id && m.faction == Faction::Ally),
            _ => false,
        };
        if !ally && (src == Src::Player || by_ally) {
            let xp = if by_ally { (def.xp + 1) / 2 } else { def.xp };
            self.gain_xp(xp);
            if src == Src::Player {
                self.kills += 1;
                if self.bonus.siphon > 0 {
                    self.p.hp = (self.p.hp + self.bonus.siphon).min(self.p.max_hp);
                }
            }
        }
        if !ally && !summoned && self.rng.gen::<f32>() < 0.10 + self.bonus.luck as f32 * 0.04 {
            let it = random_item(self.depth, &mut self.rng, self.bonus.luck);
            self.level.items.push(FloorItem { x, y, item: it });
        }
        if def.boss {
            self.msg("VORTH, WARDEN OF GRAZZARK, FALLS! The Amulet lies unguarded!", Rgb(255, 80, 60));
            self.fx.explosion(fx, fy, Rgb(255, 60, 30));
            self.fx.shake = 2.0;
            self.emit(Sfx::Explosion, x, y, 1.0);
        }
    }

    fn gain_xp(&mut self, xp: i32) {
        let xp = ((xp as f32) * self.bonus.xp_mult).round() as i32;
        self.p.xp += xp;
        while self.p.xp >= Self::xp_needed(self.p.level) {
            self.level_up();
        }
    }

    fn level_up(&mut self) {
        self.p.level += 1;
        let cd = self.cdef();
        let hpg = cd.hp_lvl + self.rng.gen_range(0..=2);
        let mpg = cd.mp_lvl + self.rng.gen_range(0..=1);
        self.p.max_hp += hpg;
        self.p.hp = (self.p.hp + hpg + self.p.max_hp / 4).min(self.p.max_hp);
        self.p.max_mp += mpg;
        self.p.mp = (self.p.mp + mpg + self.p.max_mp / 4).min(self.p.max_mp);
        let lvl = self.p.level;
        self.msg(format!("You feel stronger! Welcome to experience level {}.", lvl), Rgb(255, 220, 80));
        let (fx, fy) = (self.p.x as f32 + 0.5, self.p.y as f32 + 0.5);
        self.fx.levelup(fx, fy);
        let (px, py) = (self.p.x, self.p.y);
        self.emit(Sfx::LevelUp, px, py, 0.9);
    }

    fn player_hit(&mut self, dmg: i32, cause: DeathCause, crush: bool) {
        if self.over.is_some() {
            return;
        }
        let mut dmg = dmg;
        if self.p.ward > 0 {
            let a = dmg.min(self.p.ward);
            self.p.ward -= a;
            dmg -= a;
            if a > 0 {
                self.msg(format!("Your ward absorbs {} damage.", a), Rgb(140, 180, 255));
                self.fx.flash(Rgb(120, 160, 255), 0.25);
            }
            if self.p.ward == 0 {
                self.msg("Your ward shatters!", Rgb(140, 180, 255));
            }
        }
        if dmg <= 0 {
            return;
        }
        self.p.hp -= dmg;
        let amt = (0.3 + dmg as f32 / self.p.max_hp.max(1) as f32).min(0.8);
        self.fx.flash(Rgb(200, 0, 0), amt);
        self.fx.shake = self.fx.shake.max(if crush { 1.4 } else { 0.5 });
        let (px, py) = (self.p.x, self.p.y);
        self.emit(Sfx::PlayerHurt, px, py, 0.7);
        self.combat_heat = 1.0;
        if self.p.hp <= 0 {
            self.p.hp = 0;
            self.die(cause);
        }
    }

    fn die(&mut self, cause: DeathCause) {
        if self.over.is_some() {
            return;
        }
        self.msg("You die...", Rgb(255, 40, 40));
        let (px, py) = (self.p.x, self.p.y);
        self.emit(Sfx::PlayerDie, px, py, 1.0);
        self.fx.flash(Rgb(150, 0, 0), 1.0);
        self.over = Some(Outcome::Died(cause));
    }

    pub fn abandon(&mut self) {
        self.die(DeathCause::Abandoned);
    }

    fn explode(&mut self, cx: i32, cy: i32, dice: i32, bonus: i32, cause: DeathCause, hurts_player: bool) {
        let ids: Vec<usize> = self
            .level
            .monsters
            .iter()
            .enumerate()
            .filter(|(_, m)| m.alive && cheb(m.x, m.y, cx, cy) <= 1)
            .map(|(i, _)| i)
            .collect();
        for mi in ids {
            let mut dmg = bonus;
            for _ in 0..dice {
                dmg += self.rng.gen_range(1..=6);
            }
            let (mx, my) = (self.level.monsters[mi].x, self.level.monsters[mi].y);
            self.fx.text(mx as f32 + 0.5, my as f32 + 0.5, 0.9, dmg.to_string(), Rgb(255, 150, 40));
            self.damage_monster(mi, dmg, Src::Player);
        }
        if hurts_player && cheb(self.p.x, self.p.y, cx, cy) <= 1 {
            let mut dmg = bonus;
            for _ in 0..dice {
                dmg += self.rng.gen_range(1..=6);
            }
            dmg /= 2;
            self.msg(format!("You are caught in the blast! ({} damage)", dmg), Rgb(255, 120, 40));
            self.player_hit(dmg, cause, false);
        }
        self.noise(cx, cy, 12);
        self.combat_heat = 1.0;
    }

    fn use_skill(&mut self, s: Skill) -> Option<f64> {
        let wait = self.skill_ready_in(s);
        if wait > 0.0 {
            self.msg(format!("{} is not ready ({} turns).", s.name(), wait), GREY);
            return None;
        }
        if self.p.mp < s.mp() {
            self.msg(format!("Not enough mana for {}.", s.name()), Rgb(120, 140, 255));
            return None;
        }
        let (px, py) = (self.p.x, self.p.y);
        let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
        let f = self.p.facing;
        let lvl = self.p.level;
        let ok = match s {
            Skill::Cleave => {
                let mut targets = Vec::new();
                for &(dx, dy) in DIRS.iter() {
                    if !self.level.can_step(px, py, dx, dy) {
                        continue;
                    }
                    if let Some(mi) = self.level.monster_at(px + dx, py + dy) {
                        if self.level.monsters[mi].faction != Faction::Ally {
                            targets.push(mi);
                        }
                    }
                }
                if targets.is_empty() {
                    self.msg("There is nothing within reach to cleave.", GREY);
                    false
                } else {
                    self.cam.swing = 1.0;
                    self.emit(Sfx::Swing, px, py, 0.9);
                    self.fx.ring(fx, fy, 24, 1.5, Rgb(230, 230, 255), &['-', '=', '~'], 0.3);
                    self.msg("You cleave in a wide arc!", Rgb(255, 200, 120));
                    for mi in targets {
                        self.player_strike(mi, 1.0, 1, None);
                    }
                    self.noise(px, py, 7);
                    true
                }
            }
            Skill::WarStomp => {
                self.emit(Sfx::Stomp, px, py, 1.0);
                self.fx.shake = 1.5;
                self.fx.ring(fx, fy, 40, 3.0, Rgb(170, 140, 100), &['.', ',', 'o', '~'], 0.6);
                self.msg("You stomp the ground with tremendous force!", Rgb(255, 200, 120));
                let ids = self.monsters_within(px, py, 2.5, true);
                for mi in ids {
                    let dmg = self.rng.gen_range(1..=6) + lvl / 2;
                    self.level.monsters[mi].stun = 2 + self.rng.gen_range(0..2);
                    self.damage_monster(mi, dmg, Src::Player);
                }
                self.noise(px, py, 10);
                true
            }
            Skill::ShadowVeil => {
                self.p.invis = 12;
                for m in self.level.monsters.iter_mut() {
                    if m.target == Some(Target::Player) && cheb(m.x, m.y, px, py) > 1 {
                        m.target = None;
                    }
                }
                self.fx.mist(fx, fy, Rgb(80, 80, 130));
                self.emit(Sfx::Blink, px, py, 0.5);
                self.msg("You melt into the shadows.", Rgb(150, 150, 210));
                true
            }
            Skill::FanOfKnives => {
                for k in -1..=1 {
                    let d = (f + k).rem_euclid(8);
                    let (hx, hy, mi) = self.fire_line(d, 6);
                    self.fx.bolt(fx, fy, hx as f32 + 0.5, hy as f32 + 0.5, 20.0, '+', Rgb(200, 210, 230), None, BoltEnd::Sparks(Rgb(220, 220, 240)));
                    if let Some(mi) = mi {
                        self.player_strike(mi, 1.0, 1, Some((2, 6 + lvl / 2)));
                    }
                }
                self.emit(Sfx::Swing, px, py, 0.8);
                self.msg("You hurl a fan of knives!", Rgb(210, 210, 240));
                true
            }
            Skill::Fireball => {
                let (cx, cy, _) = self.fire_line(f, 8);
                self.fx.bolt(fx, fy, cx as f32 + 0.5, cy as f32 + 0.5, 12.0, '@', Rgb(255, 140, 30), Some([2.2, 1.0, 0.3]), BoltEnd::Explosion(Rgb(255, 120, 20)));
                self.emit(Sfx::Fireball, px, py, 0.8);
                self.emit(Sfx::Explosion, cx, cy, 1.0);
                self.msg("You hurl a roaring fireball!", Rgb(255, 160, 60));
                self.explode(cx, cy, 3, 4 + lvl, DeathCause::OwnFire, true);
                true
            }
            Skill::FrostNova => {
                self.fx.ring(fx, fy, 48, 3.5, Rgb(160, 220, 255), &['*', '+', 'x'], 0.6);
                self.fx.ice(fx, fy);
                self.emit(Sfx::Frost, px, py, 1.0);
                self.msg("A wave of bitter frost explodes outward!", Rgb(150, 210, 255));
                let ids = self.monsters_within(px, py, 2.5, true);
                for mi in ids {
                    let dmg = self.rng.gen_range(2..=12) + lvl / 2;
                    self.level.monsters[mi].stun = 3;
                    self.damage_monster(mi, dmg, Src::Player);
                }
                true
            }
            Skill::Multishot => {
                for k in -1..=1 {
                    self.shoot_arrow(f + k);
                }
                self.msg("You loose a volley of arrows!", Rgb(200, 230, 160));
                true
            }
            Skill::SummonWolf => {
                if self.level.monsters.iter().any(|m| m.alive && m.summoned && m.kind == M_WOLF) {
                    self.msg("Your wolf is already at your side.", GREY);
                    false
                } else if let Some((sx, sy)) = self.free_adjacent(px, py) {
                    let id = self.new_id();
                    let mut m = Monster::new(id, M_WOLF, sx, sy, self.time + 50.0);
                    m.faction = Faction::Ally;
                    m.summoned = true;
                    m.awake = true;
                    m.max_hp = 16 + lvl * 3;
                    m.hp = m.max_hp;
                    self.level.monsters.push(m);
                    self.fx.mist(sx as f32 + 0.5, sy as f32 + 0.5, Rgb(180, 200, 255));
                    self.emit(Sfx::Voice(VoiceKind::Snarl, Mood::Alert), sx, sy, 1.0);
                    self.msg("A dire wolf answers your call!", Rgb(140, 220, 140));
                    true
                } else {
                    self.msg("There is no room for a companion here.", GREY);
                    false
                }
            }
            Skill::Heal => {
                let amt = 8 + lvl * 3 + self.p.max_hp / 5;
                self.p.hp = (self.p.hp + amt).min(self.p.max_hp);
                self.p.poison = 0;
                self.fx.heal(fx, fy);
                self.emit(Sfx::Heal, px, py, 0.8);
                self.msg(format!("Holy light mends your wounds (+{}).", amt), Rgb(255, 240, 150));
                true
            }
            Skill::TurnUndead => {
                let ids: Vec<usize> = self
                    .monsters_within(px, py, 6.5, true)
                    .into_iter()
                    .filter(|&i| self.level.monsters[i].def().faction == Faction::Undead)
                    .collect();
                self.fx.ring(fx, fy, 60, 5.0, Rgb(255, 250, 200), &['+', '*', '\''], 0.8);
                self.fx.flash(Rgb(255, 250, 220), 0.4);
                self.fx.light(fx, fy, [2.5, 2.4, 1.8], 6.0, 0.8);
                self.emit(Sfx::Heal, px, py, 1.0);
                if ids.is_empty() {
                    self.msg("Holy light floods the area... but no undead are near.", Rgb(255, 240, 180));
                } else {
                    self.msg("The undead shriek in holy agony!", Rgb(255, 240, 150));
                    for mi in ids {
                        let dmg = self.rng.gen_range(2..=12) + lvl;
                        self.level.monsters[mi].fear = 5;
                        self.damage_monster(mi, dmg, Src::Player);
                    }
                }
                true
            }
            Skill::RaiseDead => {
                let thralls = self.level.monsters.iter().filter(|m| m.alive && m.summoned).count();
                if thralls >= 3 {
                    self.msg("You cannot bind any more thralls.", GREY);
                    false
                } else {
                    let mut best: Option<(usize, f32)> = None;
                    for (i, c) in self.level.corpses.iter().enumerate() {
                        let d = dist(c.x, c.y, px, py);
                        if d <= 5.5
                            && self.level.monster_at(c.x, c.y).is_none()
                            && !(c.x == px && c.y == py)
                            && best.map_or(true, |b| d < b.1)
                        {
                            best = Some((i, d));
                        }
                    }
                    match best {
                        None => {
                            self.msg("There are no corpses nearby to raise.", GREY);
                            false
                        }
                        Some((ci, _)) => {
                            let c = self.level.corpses.remove(ci);
                            let id = self.new_id();
                            let mut m = Monster::new(id, c.kind, c.x, c.y, self.time + 50.0);
                            m.faction = Faction::Ally;
                            m.summoned = true;
                            m.awake = true;
                            self.level.monsters.push(m);
                            self.fx.mist(c.x as f32 + 0.5, c.y as f32 + 0.5, Rgb(120, 255, 120));
                            self.emit(Sfx::Revive, c.x, c.y, 1.0);
                            self.msg(format!("The {} rises to serve you!", MONSTERS[c.kind].name), Rgb(150, 255, 150));
                            true
                        }
                    }
                }
            }
            Skill::DrainLife => {
                let (hx, hy, mi) = self.fire_line(f, 6);
                match mi {
                    None => {
                        self.msg("There is nothing there to drain.", GREY);
                        false
                    }
                    Some(mi) => {
                        let dmg = self.rng.gen_range(2..=12) + lvl;
                        self.fx.bolt(hx as f32 + 0.5, hy as f32 + 0.5, fx, fy, 10.0, '*', Rgb(150, 255, 120), Some([0.4, 1.4, 0.5]), BoltEnd::None);
                        self.emit(Sfx::Bolt, px, py, 0.8);
                        let heal = dmg / 2 + 1;
                        self.p.hp = (self.p.hp + heal).min(self.p.max_hp);
                        let name = self.level.monsters[mi].def().name;
                        self.msg(format!("You drain {} life from the {}.", dmg, name), Rgb(150, 255, 150));
                        self.fx.text(hx as f32 + 0.5, hy as f32 + 0.5, 0.9, dmg.to_string(), Rgb(150, 255, 120));
                        self.damage_monster(mi, dmg, Src::Player);
                        true
                    }
                }
            }
            Skill::Blink => {
                let (dx, dy) = DIRS[f as usize];
                let mut dest = None;
                for k in 1..=4 {
                    let (x, y) = (px + dx * k, py + dy * k);
                    let t = self.level.tile(x, y);
                    if t.opaque() {
                        break;
                    }
                    if t.walkable() && self.level.monster_at(x, y).is_none() {
                        dest = Some((x, y));
                    }
                }
                match dest {
                    None => {
                        self.msg("You cannot blink there.", GREY);
                        false
                    }
                    Some((x, y)) => {
                        self.fx.rise(fx, fy, 20, Rgb(180, 120, 255), &['*', '.', '+'], 0.8);
                        self.p.x = x;
                        self.p.y = y;
                        self.fx.rise(x as f32 + 0.5, y as f32 + 0.5, 20, Rgb(180, 120, 255), &['*', '.', '+'], 0.8);
                        self.emit(Sfx::Blink, x, y, 0.8);
                        self.after_player_move();
                        true
                    }
                }
            }
            Skill::SoulBurst => {
                self.fx.ring(fx, fy, 60, 4.0, Rgb(200, 120, 255), &['*', '@', '%'], 0.7);
                self.fx.light(fx, fy, [1.5, 0.6, 2.2], 4.0, 0.7);
                self.fx.shake = 1.0;
                self.emit(Sfx::Explosion, px, py, 0.9);
                self.msg("Soulfire bursts from your body!", Rgb(210, 150, 255));
                let ids = self.monsters_within(px, py, 2.9, true);
                for mi in ids {
                    let dmg = (0..3).map(|_| self.rng.gen_range(1..=6)).sum::<i32>() + lvl;
                    self.damage_monster(mi, dmg, Src::Player);
                }
                true
            }
            Skill::PhaseWard => {
                self.p.ward = 20 + lvl * 2;
                self.fx.ring(fx, fy, 30, 1.0, Rgb(120, 170, 255), &['o', 'O', '0'], 0.6);
                self.emit(Sfx::Frost, px, py, 0.7);
                let w = self.p.ward;
                self.msg(format!("A shimmering ward surrounds you ({}).", w), Rgb(140, 180, 255));
                true
            }
            Skill::SecondWind => {
                let amt = self.p.max_hp / 2;
                self.p.hp = (self.p.hp + amt).min(self.p.max_hp);
                self.fx.heal(fx, fy);
                self.emit(Sfx::Heal, px, py, 0.9);
                self.msg(format!("You catch your second wind (+{}).", amt), Rgb(150, 255, 150));
                true
            }
        };
        if ok {
            self.p.mp -= s.mp();
            self.p.cds.insert(s, self.time + s.cooldown() * 100.0);
            self.combat_heat = self.combat_heat.max(0.5);
            Some(100.0)
        } else {
            None
        }
    }

    fn pickup(&mut self) -> Option<f64> {
        let (px, py) = (self.p.x, self.p.y);
        let Some(ii) = self.level.items.iter().position(|i| i.x == px && i.y == py) else {
            self.msg("There is nothing here to pick up.", GREY);
            return None;
        };
        let item = self.level.items[ii].item;
        if item == Item::Amulet {
            self.level.items.remove(ii);
            self.p.has_amulet = true;
            let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
            self.fx.ring(fx, fy, 60, 4.0, Rgb(255, 210, 60), &['*', '+', '$'], 1.0);
            self.fx.flash(Rgb(255, 200, 40), 0.8);
            self.fx.light(fx, fy, [2.5, 2.0, 0.6], 6.0, 2.0);
            self.fx.shake = 1.5;
            self.emit(Sfx::Amulet, px, py, 1.0);
            self.msg("You seize the AMULET OF GRAZZARK! It throbs with dark power.", Rgb(255, 215, 60));
            self.msg("The whole dungeon shudders. Now RETURN TO THE SURFACE alive!", Rgb(255, 150, 60));
            self.combat_heat = 1.0;
            return Some(100.0);
        }
        if self.p.inv.len() >= MAX_INV {
            self.msg("Your pack is full.", GREY);
            return None;
        }
        self.level.items.remove(ii);
        self.msg(format!("You pick up {}.", item.name()), Rgb(220, 220, 150));
        self.p.inv.push(item);
        self.emit(Sfx::Pickup, px, py, 0.6);
        Some(50.0)
    }

    fn drop_item(&mut self, i: usize) -> Option<f64> {
        if i >= self.p.inv.len() {
            return None;
        }
        let it = self.p.inv.remove(i);
        let (px, py) = (self.p.x, self.p.y);
        self.level.items.push(FloorItem { x: px, y: py, item: it });
        self.msg(format!("You drop {}.", it.name()), GREY);
        Some(50.0)
    }

    pub fn use_item(&mut self, i: usize) -> Option<f64> {
        if i >= self.p.inv.len() {
            return None;
        }
        let it = self.p.inv[i];
        let (px, py) = (self.p.x, self.p.y);
        let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
        match it {
            Item::Potion(k) => {
                self.p.inv.remove(i);
                match k {
                    Potion::Healing => {
                        let amt = 12 + self.depth as i32 * 2 + self.p.max_hp / 4;
                        self.p.hp = (self.p.hp + amt).min(self.p.max_hp);
                        self.p.poison = 0;
                        self.msg(format!("You feel much better (+{} HP).", amt), Rgb(255, 120, 120));
                    }
                    Potion::Mana => {
                        let amt = 10 + self.p.max_mp / 2;
                        self.p.mp = (self.p.mp + amt).min(self.p.max_mp);
                        self.msg(format!("Arcane energy surges through you (+{} MP).", amt), Rgb(120, 150, 255));
                    }
                    Potion::Might => {
                        self.p.extra_dmg += 1;
                        self.msg("Your muscles swell with power! (+1 damage)", Rgb(255, 170, 60));
                    }
                    Potion::Vigor => {
                        self.p.max_hp += 5;
                        self.p.hp += 5;
                        self.msg("You feel hardier! (+5 max HP)", Rgb(120, 255, 120));
                    }
                    Potion::Cure => {
                        self.p.poison = 0;
                        self.p.hp = (self.p.hp + 5).min(self.p.max_hp);
                        self.msg("The poison is purged from your blood.", Rgb(200, 255, 200));
                    }
                }
                self.fx.heal(fx, fy);
                self.emit(Sfx::Heal, px, py, 0.6);
                Some(100.0)
            }
            Item::Scroll(s) => {
                self.p.inv.remove(i);
                match s {
                    Scroll::Teleport => {
                        let mut dest = self.level.random_floor(&mut self.rng);
                        for _ in 0..30 {
                            let c = self.level.random_floor(&mut self.rng);
                            if cheb(c.0, c.1, px, py) >= 10 {
                                dest = c;
                                break;
                            }
                        }
                        self.fx.rise(fx, fy, 25, Rgb(180, 120, 255), &['*', '.', '+'], 0.8);
                        self.p.x = dest.0;
                        self.p.y = dest.1;
                        self.emit(Sfx::Blink, dest.0, dest.1, 0.8);
                        self.msg("The world twists around you!", Rgb(200, 150, 255));
                        self.cam.x = dest.0 as f32 + 0.5;
                        self.cam.y = dest.1 as f32 + 0.5;
                        self.after_player_move();
                    }
                    Scroll::Mapping => {
                        let w = self.level.w;
                        let h = self.level.h;
                        for y in 0..h {
                            for x in 0..w {
                                let near_open = DIRS.iter().any(|&(dx, dy)| !self.level.tile(x + dx, y + dy).opaque())
                                    || !self.level.tile(x, y).opaque();
                                if near_open {
                                    let i = self.level.idx(x, y);
                                    self.level.explored[i] = true;
                                }
                            }
                        }
                        self.emit(Sfx::Amulet, px, py, 0.4);
                        self.msg("A map of the level burns itself into your mind.", Rgb(230, 220, 160));
                    }
                    Scroll::Fire => {
                        self.fx.explosion(fx, fy, Rgb(255, 120, 20));
                        self.fx.ring(fx, fy, 50, 4.0, Rgb(255, 140, 30), &['*', '#', '%'], 0.6);
                        self.emit(Sfx::Explosion, px, py, 1.0);
                        self.msg("A ring of fire erupts around you!", Rgb(255, 150, 60));
                        let ids = self.monsters_within(px, py, 2.9, true);
                        for mi in ids {
                            let dmg = (0..4).map(|_| self.rng.gen_range(1..=6)).sum::<i32>();
                            self.damage_monster(mi, dmg, Src::Player);
                        }
                        self.noise(px, py, 12);
                    }
                }
                Some(100.0)
            }
            Item::Weapon(g) => {
                let old = self.p.wpn;
                self.p.wpn = g;
                self.p.inv[i] = Item::Weapon(old);
                self.msg(format!("You wield the {}.", it.name()), Rgb(220, 220, 220));
                self.emit(Sfx::Clang, px, py, 0.4);
                Some(100.0)
            }
            Item::Armor(g) => {
                let old = self.p.arm;
                self.p.arm = g;
                self.p.inv[i] = Item::Armor(old);
                self.msg(format!("You put on the {}.", it.name()), Rgb(220, 220, 220));
                Some(150.0)
            }
            Item::Amulet => None,
        }
    }

    fn use_stairs(&mut self) -> Option<f64> {
        let (px, py) = (self.p.x, self.p.y);
        match self.level.tile(px, py) {
            Tile::StairsDown => {
                if self.depth >= 10 {
                    self.msg("The stair plunges into a wall of black flame. Nothing lies deeper.", Rgb(255, 120, 80));
                    None
                } else {
                    self.change_level(true);
                    Some(100.0)
                }
            }
            Tile::StairsUp => {
                if self.depth == 1 {
                    if self.p.has_amulet {
                        self.msg("You climb into the light of day, the Amulet blazing in your fist!", Rgb(255, 230, 120));
                        self.over = Some(Outcome::Won);
                        Some(0.0)
                    } else {
                        self.msg("You cannot leave without the Amulet of GRAZZARK!", Rgb(255, 160, 80));
                        None
                    }
                } else {
                    self.change_level(false);
                    Some(100.0)
                }
            }
            _ => {
                self.msg("There are no stairs here.", GREY);
                None
            }
        }
    }

    fn change_level(&mut self, down: bool) {
        let old_depth = self.depth;
        let new_depth = if down { old_depth + 1 } else { old_depth - 1 };
        let (px, py) = (self.p.x, self.p.y);

        // Companions next to you follow you through the stairs.
        let mut followers = Vec::new();
        let mut i = 0;
        while i < self.level.monsters.len() {
            let m = &self.level.monsters[i];
            if m.alive && m.faction == Faction::Ally && cheb(m.x, m.y, px, py) <= 2 {
                followers.push(self.level.monsters.remove(i));
            } else {
                i += 1;
            }
        }

        let new_level = match self.stash.remove(&new_depth) {
            Some(l) => l,
            None => {
                let mut l = generate(new_depth, &mut self.rng);
                let luck = self.bonus.luck;
                populate(&mut l, &mut self.rng, &mut self.next_id, luck, self.time);
                l
            }
        };
        let old = std::mem::replace(&mut self.level, new_level);
        self.stash.insert(old_depth, old);
        self.depth = new_depth;
        self.max_depth = self.max_depth.max(new_depth);
        let (sx, sy) = if down { self.level.up } else { self.level.down };
        self.p.x = sx;
        self.p.y = sy;
        self.enter_level();

        for mut m in followers {
            if let Some((fx, fy)) = self.free_adjacent(sx, sy) {
                m.x = fx;
                m.y = fy;
                m.rx = fx as f32;
                m.ry = fy as f32;
                m.next_act = self.time + 50.0;
                m.target = None;
                m.last_known = None;
                self.level.monsters.push(m);
            }
        }

        self.emit(Sfx::Stairs, sx, sy, 0.8);
        let name = DEPTH_NAMES[(new_depth - 1) as usize];
        self.msg(
            format!("You {} to depth {}: {}.", if down { "descend" } else { "climb" }, new_depth, name),
            Rgb(200, 180, 255),
        );
        if new_depth == 10 && down {
            self.msg("An overwhelming malice fills these halls. The Warden is near.", Rgb(255, 80, 80));
        }

        if self.p.has_amulet && !self.level.hunted {
            self.level.hunted = true;
            let kinds: Vec<usize> = (0..MONSTERS.len())
                .filter(|&k| {
                    let d = &MONSTERS[k];
                    !d.boss && d.depth.1 >= new_depth && d.depth.0 <= new_depth + 3
                })
                .collect();
            let n = 2 + new_depth / 3;
            for _ in 0..n {
                let mut pos = None;
                for _ in 0..40 {
                    let c = self.level.random_floor(&mut self.rng);
                    if self.dmap[self.level.idx(c.0, c.1)] >= 8 {
                        pos = Some(c);
                        break;
                    }
                }
                if let (Some((x, y)), Some(&k)) = (pos, kinds.choose(&mut self.rng)) {
                    let id = self.new_id();
                    let mut m = Monster::new(id, k, x, y, self.time + 100.0);
                    m.awake = true;
                    m.last_known = Some((sx, sy));
                    self.level.monsters.push(m);
                }
            }
            self.msg("The Amulet's pulse echoes through the halls. Things are hunting you...", Rgb(255, 120, 80));
            self.emit(Sfx::Rumble, sx, sy, 1.0);
        }
    }

    // ------------------------------------------------------------ monsters

    fn monster_act(&mut self, mi: usize) {
        let kind = self.level.monsters[mi].kind;
        let def = &MONSTERS[kind];
        {
            let m = &mut self.level.monsters[mi];
            m.next_act += def.speed;
            if m.stun > 0 {
                m.stun -= 1;
                return;
            }
        }
        let (mx, my, fac, awake) = {
            let m = &self.level.monsters[mi];
            (m.x, m.y, m.faction, m.awake)
        };
        let (px, py) = (self.p.x, self.p.y);
        let pd = cheb(mx, my, px, py);
        let sees_player = hostile(fac, Faction::Player)
            && dist(mx, my, px, py) <= def.sight as f32
            && (self.p.invis == 0 || pd <= 1)
            && self.level.los(mx, my, px, py);

        if !awake {
            if sees_player {
                let stealth = self.cdef().stealth;
                let chance = (if pd <= 2 { 0.7 } else { 0.3 }) - stealth as f64 * 0.06;
                if self.rng.gen_bool(chance.clamp(0.05, 0.95)) {
                    {
                        let m = &mut self.level.monsters[mi];
                        m.awake = true;
                        m.target = Some(Target::Player);
                        m.last_known = Some((px, py));
                    }
                    self.emit(Sfx::Voice(def.voice, Mood::Alert), mx, my, 1.0);
                    if self.level.is_visible(mx, my) {
                        self.msg(format!("{} notices you!", cap(&the_name(kind))), Rgb(255, 170, 90));
                    }
                }
            } else if self.rng.gen_bool(0.003) {
                self.emit(Sfx::Voice(def.voice, Mood::Idle), mx, my, 0.5);
            }
            return;
        }

        if self.level.monsters[mi].fear > 0 {
            self.level.monsters[mi].fear -= 1;
            self.monster_flee(mi);
            return;
        }

        // Choose a target: the player, or any hostile monster in view.
        let mut tgt = self.level.monsters[mi].target;
        tgt = match tgt {
            Some(Target::Player) => {
                if sees_player {
                    tgt
                } else {
                    None
                }
            }
            Some(Target::Mon(id)) => match self.mon_index(id) {
                Some(oi) if cheb(self.level.monsters[oi].x, self.level.monsters[oi].y, mx, my) <= 10 => tgt,
                _ => None,
            },
            None => None,
        };
        if tgt.is_none() || self.rng.gen_bool(0.15) {
            let mut best: Option<(Target, i32)> = if sees_player { Some((Target::Player, pd)) } else { None };
            for (oi, o) in self.level.monsters.iter().enumerate() {
                if oi == mi || !o.alive || !hostile(fac, o.faction) {
                    continue;
                }
                let d = cheb(o.x, o.y, mx, my);
                if d > 7 {
                    continue;
                }
                let dd = d + 1;
                if best.map_or(true, |b| dd < b.1) && self.level.los(mx, my, o.x, o.y) {
                    best = Some((Target::Mon(o.id), dd));
                }
            }
            if let Some((t, _)) = best {
                if tgt.is_none() || self.rng.gen_bool(0.5) {
                    tgt = Some(t);
                }
            }
        }
        self.level.monsters[mi].target = tgt;

        match tgt {
            Some(t) => {
                let (tx, ty) = self.target_pos(t);
                if t == Target::Player {
                    self.level.monsters[mi].last_known = Some((tx, ty));
                }
                let d = cheb(mx, my, tx, ty);
                if d == 1 && self.level.can_step(mx, my, tx - mx, ty - my) {
                    self.monster_melee(mi, t);
                    return;
                }
                if let Some(r) = &def.ranged {
                    if d > 1 && d <= r.range && self.level.los(mx, my, tx, ty) && self.rng.gen::<f32>() < r.chance {
                        self.monster_ranged(mi, t);
                        return;
                    }
                }
                if def.erratic && self.rng.gen_bool(0.35) {
                    self.monster_wander(mi);
                    return;
                }
                self.monster_step_toward(mi, tx, ty, t == Target::Player);
            }
            None => {
                if fac == Faction::Ally {
                    if pd > 2 {
                        self.monster_step_toward(mi, px, py, true);
                    } else if self.rng.gen_bool(0.2) {
                        self.monster_wander(mi);
                    }
                    return;
                }
                let lk = self.level.monsters[mi].last_known;
                match lk {
                    Some((lx, ly)) if (lx, ly) != (mx, my) => {
                        let use_d = (lx, ly) == (px, py);
                        if !self.monster_step_toward(mi, lx, ly, use_d) {
                            self.level.monsters[mi].last_known = None;
                        }
                    }
                    _ => {
                        self.level.monsters[mi].last_known = None;
                        if self.rng.gen_bool(0.3) {
                            self.monster_wander(mi);
                        }
                        if self.rng.gen_bool(0.015) {
                            self.emit(Sfx::Voice(def.voice, Mood::Idle), mx, my, 0.6);
                        }
                    }
                }
            }
        }
    }

    fn monster_free(&self, mx: i32, my: i32, dx: i32, dy: i32) -> bool {
        let (nx, ny) = (mx + dx, my + dy);
        let t = self.level.tile(nx, ny);
        (t.walkable() || t == Tile::DoorClosed)
            && self.level.can_step(mx, my, dx, dy)
            && self.level.monster_at(nx, ny).is_none()
            && !(nx == self.p.x && ny == self.p.y)
    }

    fn monster_wander(&mut self, mi: usize) {
        let (mx, my) = (self.level.monsters[mi].x, self.level.monsters[mi].y);
        let (dx, dy) = DIRS[self.rng.gen_range(0..8)];
        if self.monster_free(mx, my, dx, dy) {
            self.move_monster(mi, mx + dx, my + dy);
        }
    }

    fn monster_flee(&mut self, mi: usize) {
        let (mx, my) = (self.level.monsters[mi].x, self.level.monsters[mi].y);
        let cur = self.dmap[self.level.idx(mx, my)];
        let mut best: Option<(i32, i32, i32)> = None;
        for &(dx, dy) in DIRS.iter() {
            if !self.monster_free(mx, my, dx, dy) {
                continue;
            }
            let v = self.dmap[self.level.idx(mx + dx, my + dy)];
            if v < UNREACH && v > cur && best.map_or(true, |b| v > b.0) {
                best = Some((v, dx, dy));
            }
        }
        if let Some((_, dx, dy)) = best {
            self.move_monster(mi, mx + dx, my + dy);
        }
    }

    fn move_monster(&mut self, mi: usize, nx: i32, ny: i32) {
        if self.level.tile(nx, ny) == Tile::DoorClosed {
            self.level.set(nx, ny, Tile::DoorOpen);
            self.emit(Sfx::Creak, nx, ny, 0.8);
            self.refresh_view();
            if self.level.is_visible(nx, ny) {
                self.msg("A door creaks open.", GREY);
            }
            return;
        }
        {
            let m = &mut self.level.monsters[mi];
            m.x = nx;
            m.y = ny;
        }
        // Distant, unseen footsteps give you something to listen for.
        if !self.level.is_visible(nx, ny) && self.dmap[self.level.idx(nx, ny)] <= 12 && self.rng.gen_bool(0.25) {
            self.emit(Sfx::Step, nx, ny, 0.45);
        }
    }

    /// Step toward (tx,ty). Uses the player distance map when chasing the
    /// player so monsters path around corners. Returns false if stuck.
    fn monster_step_toward(&mut self, mi: usize, tx: i32, ty: i32, use_dmap: bool) -> bool {
        let (mx, my, fac) = {
            let m = &self.level.monsters[mi];
            (m.x, m.y, m.faction)
        };
        let (px, py) = (self.p.x, self.p.y);
        let score = |g: &Game, x: i32, y: i32| -> i32 {
            if use_dmap {
                g.dmap[g.level.idx(x, y)]
            } else {
                cheb(x, y, tx, ty) * 10 + (x - tx).abs() + (y - ty).abs()
            }
        };
        let cur = score(self, mx, my);
        let mut opts: Vec<(i32, i32, i32, u32)> = Vec::new();
        for &(dx, dy) in DIRS.iter() {
            let (nx, ny) = (mx + dx, my + dy);
            if !self.level.can_step(mx, my, dx, dy) {
                continue;
            }
            let t = self.level.tile(nx, ny);
            if !(t.walkable() || t == Tile::DoorClosed) {
                continue;
            }
            if nx == px && ny == py {
                continue;
            }
            let s = score(self, nx, ny);
            let r: u32 = self.rng.gen();
            opts.push((s, dx, dy, r));
        }
        opts.sort_by_key(|o| (o.0, o.3));
        for (s, dx, dy, _) in opts {
            if s > cur {
                break;
            }
            let (nx, ny) = (mx + dx, my + dy);
            if let Some(oi) = self.level.monster_at(nx, ny) {
                let ofac = self.level.monsters[oi].faction;
                if hostile(fac, ofac) {
                    let oid = self.level.monsters[oi].id;
                    self.level.monsters[mi].target = Some(Target::Mon(oid));
                    self.monster_melee(mi, Target::Mon(oid));
                    return true;
                }
                continue;
            }
            self.move_monster(mi, nx, ny);
            return true;
        }
        false
    }

    fn monster_melee(&mut self, mi: usize, t: Target) {
        let (kind, mx, my, mid, fac) = {
            let m = &self.level.monsters[mi];
            (m.kind, m.x, m.y, m.id, m.faction)
        };
        let def = &MONSTERS[kind];
        let (tx, ty) = self.target_pos(t);
        self.level.monsters[mi].lunge = ((tx - mx) as f32, (ty - my) as f32, 1.0);
        let vis = self.level.is_visible(mx, my);
        let ally = fac == Faction::Ally;
        let aname = if ally { format!("your {}", def.name) } else { the_name(kind) };
        match t {
            Target::Player => {
                if self.rng.gen_bool(0.5) {
                    self.emit(Sfx::Voice(def.voice, Mood::Attack), mx, my, 0.8);
                } else {
                    self.emit(Sfx::Swing, mx, my, 0.5);
                }
                let roll = self.rng.gen_range(1..=20);
                let ac = 10 + self.player_ac();
                if roll == 20 || (roll != 1 && roll + def.hit >= ac) {
                    let dmg = self.rng.gen_range(def.dmg.0..=def.dmg.1);
                    self.msg(format!("{} hits you for {}!", cap(&aname), dmg), Rgb(255, 90, 90));
                    let (px, py) = (self.p.x, self.p.y);
                    self.emit(Sfx::Hit, px, py, 0.8);
                    self.player_hit(dmg, DeathCause::Monster(kind), def.crush);
                    if def.poison && self.over.is_none() && self.rng.gen_bool(0.35) {
                        self.p.poison += 4 + self.depth as i32 / 2;
                        self.p.poison_src = a_name(kind);
                        self.msg("You are poisoned!", Rgb(120, 255, 80));
                        self.fx.flash(Rgb(80, 200, 40), 0.3);
                    }
                } else {
                    self.msg(format!("{} misses you.", cap(&aname)), GREY);
                }
            }
            Target::Mon(oid) => {
                let Some(oi) = self.mon_index(oid) else { return };
                let (okind, ox, oy, ofac) = {
                    let o = &self.level.monsters[oi];
                    (o.kind, o.x, o.y, o.faction)
                };
                let odef = &MONSTERS[okind];
                let ovis = self.level.is_visible(ox, oy);
                let oname = if ofac == Faction::Ally { format!("your {}", odef.name) } else { the_name(okind) };
                let roll = self.rng.gen_range(1..=20);
                if roll + def.hit >= 10 + odef.ac {
                    let dmg = self.rng.gen_range(def.dmg.0..=def.dmg.1);
                    if vis || ovis {
                        self.msg(format!("{} hits {}.", cap(&aname), oname), Rgb(200, 170, 140));
                    }
                    self.emit(Sfx::Hit, ox, oy, 0.8);
                    if self.rng.gen_bool(0.4) {
                        self.emit(Sfx::Voice(def.voice, Mood::Attack), mx, my, 0.8);
                    }
                    self.fx.text(ox as f32 + 0.5, oy as f32 + 0.5, 0.9, dmg.to_string(), Rgb(255, 160, 80));
                    self.damage_monster(oi, dmg, Src::Mon(mid));
                } else if vis || ovis {
                    self.msg(format!("{} misses {}.", cap(&aname), oname), GREY);
                }
                if !(vis || ovis) && self.turns > self.last_fight_msg + 20 {
                    self.last_fight_msg = self.turns;
                    self.msg("You hear the sounds of a fight somewhere nearby.", Rgb(180, 160, 140));
                }
                self.noise(mx, my, 6);
            }
        }
    }

    fn monster_ranged(&mut self, mi: usize, t: Target) {
        let (kind, mx, my, mid) = {
            let m = &self.level.monsters[mi];
            (m.kind, m.x, m.y, m.id)
        };
        let def = &MONSTERS[kind];
        let r = match &def.ranged {
            Some(r) => r,
            None => return,
        };
        let (tx, ty) = self.target_pos(t);
        let (hx, hy, hit) = self.trace(mx, my, tx, ty, r.range + 2, Some(mid));
        let end = match r.element {
            Element::Fire => BoltEnd::Sparks(r.color),
            Element::Magic => BoltEnd::Magic(r.color),
            Element::Frost => BoltEnd::Ice,
            Element::Phys => BoltEnd::Sparks(Rgb(200, 200, 200)),
        };
        let light = match r.element {
            Element::Fire => Some([2.0, 0.9, 0.3]),
            Element::Magic => Some([0.5, 1.6, 0.6]),
            Element::Frost => Some([0.5, 0.9, 1.8]),
            Element::Phys => None,
        };
        self.fx.bolt(mx as f32 + 0.5, my as f32 + 0.5, hx as f32 + 0.5, hy as f32 + 0.5, 14.0, r.ch, r.color, light, end);
        let sfx = match r.element {
            Element::Fire => Sfx::Fireball,
            Element::Magic => Sfx::Bolt,
            Element::Frost => Sfx::Frost,
            Element::Phys => Sfx::Arrow,
        };
        self.emit(sfx, mx, my, 0.8);
        if self.level.is_visible(mx, my) {
            self.msg(format!("{} {}!", cap(&the_name(kind)), r.verb), Rgb(255, 180, 120));
        }
        self.level.monsters[mi].lunge = ((tx - mx).signum() as f32 * 0.5, (ty - my).signum() as f32 * 0.5, 0.6);
        match hit {
            Some(Target::Player) => {
                let roll = self.rng.gen_range(1..=20);
                if roll == 20 || (roll != 1 && roll + def.hit >= 10 + self.player_ac()) {
                    let dmg = self.rng.gen_range(r.dmg.0..=r.dmg.1);
                    self.msg(format!("You are struck for {}!", dmg), Rgb(255, 90, 90));
                    let cause = if r.element == Element::Phys { DeathCause::Monster(kind) } else { DeathCause::Spell(kind) };
                    self.player_hit(dmg, cause, false);
                } else {
                    self.msg("It misses you.", GREY);
                }
            }
            Some(Target::Mon(oid)) => {
                if let Some(oi) = self.mon_index(oid) {
                    if self.rng.gen_bool(0.75) {
                        let dmg = self.rng.gen_range(r.dmg.0..=r.dmg.1);
                        let (ox, oy) = (self.level.monsters[oi].x, self.level.monsters[oi].y);
                        self.fx.text(ox as f32 + 0.5, oy as f32 + 0.5, 0.9, dmg.to_string(), Rgb(255, 160, 80));
                        self.damage_monster(oi, dmg, Src::Mon(mid));
                    }
                }
            }
            None => {}
        }
        self.combat_heat = self.combat_heat.max(if t == Target::Player { 1.0 } else { 0.3 });
    }

    // ------------------------------------------------------------ sound

    /// Work out how a sound at (sx,sy) reaches the player's ears. With line
    /// of sight the sound is direct. Otherwise it travels the corridors: it is
    /// quieter (path length), muffled, appears to come from the corner it
    /// bends around, and arrives with a delayed echo and more reverb.
    pub fn spatial(&self, sx: f32, sy: f32, vol: f32) -> Option<Spatial> {
        let (px, py) = (self.p.x, self.p.y);
        let (lx, ly) = (self.cam.x, self.cam.y);
        let tx = sx.floor() as i32;
        let ty = sy.floor() as i32;
        let straight = ((sx - lx).powi(2) + (sy - ly).powi(2)).sqrt();
        let los = straight < 1.2 || self.level.los(px, py, tx, ty);
        let path = if self.level.inb(tx, ty) { self.dmap[self.level.idx(tx, ty)] } else { UNREACH };
        let path_f = if path >= UNREACH { straight * 2.5 + 12.0 } else { path as f32 };

        let (eff, mut cutoff, ax, ay, extra) = if los {
            (straight, 14000.0, sx, sy, 0.0)
        } else {
            let (mut cx, mut cy) = (tx, ty);
            if path < UNREACH {
                for _ in 0..48 {
                    if self.level.los(px, py, cx, cy) {
                        break;
                    }
                    let here = self.dmap[self.level.idx(cx, cy)];
                    let mut next = None;
                    let mut best = here;
                    for &(dx, dy) in DIRS.iter() {
                        let (nx, ny) = (cx + dx, cy + dy);
                        if !self.level.inb(nx, ny) {
                            continue;
                        }
                        let v = self.dmap[self.level.idx(nx, ny)];
                        if v < best {
                            best = v;
                            next = Some((nx, ny));
                        }
                    }
                    match next {
                        Some((nx, ny)) => {
                            cx = nx;
                            cy = ny;
                        }
                        None => break,
                    }
                }
            }
            let extra = (path_f - straight).max(0.0);
            let cut = (4500.0 / (1.0 + extra * 0.35)).max(320.0);
            (path_f + 1.5, cut, cx as f32 + 0.5, cy as f32 + 0.5, extra)
        };

        let gain = vol / (1.0 + (eff / 3.5).powf(1.5));
        if gain < 0.006 {
            return None;
        }
        let ear_ang = if self.overhead { -PI / 2.0 } else { self.cam.ang };
        let rel = (ay - ly).atan2(ax - lx) - ear_ang;
        let dist_here = ((ax - lx).powi(2) + (ay - ly).powi(2)).sqrt();
        let pan = if dist_here < 0.3 { 0.0 } else { rel.sin() * 0.9 };
        if rel.cos() < 0.0 && dist_here >= 0.3 {
            cutoff *= 0.65;
        }
        let room = self.room_size;
        let mut echoes = vec![
            (0.025 + room * 0.05, 0.22 + room * 0.1, -0.6),
            (0.045 + room * 0.08, 0.16 + room * 0.08, 0.6),
        ];
        if !los {
            let d = (extra * 3.0 / 343.0) + 0.03;
            echoes.push((d.min(0.4), 0.35, -pan));
            echoes.push(((d * 1.9).min(0.6), 0.18, pan * 0.5));
        }
        let reverb = (0.15 + 0.35 * room + if los { 0.0 } else { 0.25 } + (eff / 30.0).min(0.3)).min(0.95);
        Some(Spatial { gain, pan, cutoff, echoes, reverb, room })
    }

    /// A random ambient event somewhere in the level.
    pub fn ambient_event(&mut self) {
        if self.over.is_some() {
            return;
        }
        let (px, py) = (self.p.x, self.p.y);
        let mut pos = None;
        for _ in 0..30 {
            let x = px + self.rng.gen_range(-18..=18);
            let y = py + self.rng.gen_range(-18..=18);
            if self.level.tile(x, y).walkable() {
                pos = Some((x, y));
                break;
            }
        }
        let Some((x, y)) = pos else { return };
        let r: f32 = self.rng.gen();
        let (sfx, vol) = if r < 0.34 {
            (Sfx::Drip, 0.7)
        } else if r < 0.47 {
            (Sfx::Rumble, 1.0)
        } else if r < 0.58 {
            (Sfx::Chains, 0.7)
        } else if r < 0.68 && self.depth >= 3 {
            (Sfx::Whisper, 0.7)
        } else if r < 0.76 {
            (Sfx::Creak, 0.6)
        } else if r < 0.84 {
            (Sfx::Wind, 0.8)
        } else {
            let cands: Vec<usize> = self
                .level
                .monsters
                .iter()
                .enumerate()
                .filter(|(_, m)| m.alive && m.faction != Faction::Ally && !self.level.is_visible(m.x, m.y))
                .map(|(i, _)| i)
                .collect();
            if let Some(&i) = cands.choose(&mut self.rng) {
                let m = &self.level.monsters[i];
                let (mx, my, v) = (m.x, m.y, m.def().voice);
                self.emit(Sfx::Voice(v, Mood::Idle), mx, my, 0.8);
                return;
            }
            (Sfx::Drip, 0.7)
        };
        self.emit(sfx, x, y, vol);
    }
}

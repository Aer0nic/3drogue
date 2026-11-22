//! Core game state and rules: the turn / real-time clock, player actions,
//! combat, skills, monster AI (including infighting), corpse revival, level
//! persistence, and how sound propagates through the dungeon.

use crate::audio::{Mood, Sfx, Spatial};
use crate::color::Rgb;
use crate::data::*;
use crate::entity::*;
use crate::fx::{BoltEnd, DoorAnim, DoorOpening, Fx};
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
    if d.boss || d.unique > 0 {
        return d.name.to_string();
    }
    let first = d.name.chars().next().unwrap_or('x');
    let art = if "aeiou".contains(first) { "an" } else { "a" };
    format!("{} {}", art, d.name)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Move in an absolute direction; `true` also turns to face it.
    Move(i32, bool),
    Turn(i32),
    Attack,
    Wait,
    Pickup,
    /// Pick up the k-th item on the floor here.
    PickupOne(usize),
    /// Gather everything here in a single turn.
    PickupAll,
    Stairs,
    Skill(usize),
    Use(usize),
    Drop(usize),
    QuickHeal,
    /// Relight a torch the wind blew out.
    Relight,
    /// Raise the corpse at this spot (chosen from the raise menu).
    RaiseAt(i32, i32),
    /// Keen Hearing: hold still and listen for what moves nearby (Shift+Q).
    Listen,
}

/// A sound picked out by listening intently (Keen Hearing).
#[derive(Clone, Debug)]
pub struct Ping {
    /// Where the sound was made.
    pub x: i32,
    pub y: i32,
    /// Where it reaches you from: the last corner it came around.
    pub ax: f32,
    pub ay: f32,
    /// Your rough guess at how far off it is, in tiles.
    pub dist: i32,
    /// Seconds since you heard it.
    pub age: f32,
    /// How the sound spreads: every tile it reaches and how far it travelled
    /// to get there. Walls it strikes are included, one step past the floor.
    pub wave: Vec<(i32, i32, u16)>,
    /// How far the wave spreads before it fades.
    pub reach: u16,
}

impl Ping {
    /// How long a ping stays on screen, in seconds.
    pub const LIFE: f32 = 7.0;
}

/// A corpse the necromancer could raise, as offered in the raise menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RaiseOption {
    pub x: i32,
    pub y: i32,
    pub kind: usize,
    /// Max HP it will hold while it lives.
    pub cost: i32,
    /// A fallen thrall raised once more (Undying Bond).
    pub again: bool,
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
                TrapKind::Maw => "devoured by the living floor",
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

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Msg {
    pub text: String,
    pub color: Rgb,
    pub age: f32,
    /// The turn it was written on, for the message log.
    #[serde(default)]
    pub turn: u64,
}

/// How many turns of messages the log (L) keeps.
pub const LOG_TURNS: u64 = 100;

pub struct SoundEv {
    pub sfx: Sfx,
    pub x: f32,
    pub y: f32,
    pub vol: f32,
    /// Seconds before it is heard (an arrow still in flight).
    pub delay: f32,
}

/// Smoothly interpolated camera following the grid-locked player.
#[derive(Default)]
pub struct Cam {
    pub x: f32,
    pub y: f32,
    pub ang: f32,
    pub walk: f32,
    pub swing: f32,
}

/// A wisp of smoke from the torch in your hand, in screen cells relative to
/// the tip of the flame.
#[derive(Clone, Copy, Debug)]
pub struct Smoke {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub max: f32,
}

/// Torments the living dungeon inflicts on whoever carries the Amulet out.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Curse {
    /// Your memory of the floor is wiped until it passes.
    Forgetting,
    /// Harmless phantom monsters stalk you.
    Figments,
    /// Your torch gutters down to almost nothing.
    Darkness,
    /// Some of your steps lurch in a random direction.
    Vertigo,
    /// Everything you do takes 40% longer.
    Leaden,
    /// No skills, no scrolls.
    Hush,
    /// An unnatural wind snuffs your torch: total darkness, no map.
    Gale,
}

impl Curse {
    pub const ALL: [Curse; 7] = [
        Curse::Forgetting,
        Curse::Figments,
        Curse::Darkness,
        Curse::Vertigo,
        Curse::Leaden,
        Curse::Hush,
        Curse::Gale,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Curse::Forgetting => "FORGETTING",
            Curse::Figments => "PARANOIA",
            Curse::Darkness => "DARKNESS",
            Curse::Vertigo => "VERTIGO",
            Curse::Leaden => "LEADEN",
            Curse::Hush => "HUSH",
            Curse::Gale => "GALE",
        }
    }
    pub fn onset(self) -> &'static str {
        match self {
            Curse::Forgetting => {
                "The dungeon reaches into your mind... you can no longer remember the way you came!"
            }
            Curse::Figments => "Shapes peel themselves out of the walls. Are they real?",
            Curse::Darkness => "A cold breath from the stones smothers your torch to a dim ember!",
            Curse::Vertigo => {
                "The floor heaves like the belly of a beast. You can barely keep your feet!"
            }
            Curse::Leaden => "The air thickens like tar. Your limbs turn to lead!",
            Curse::Hush => "The walls drink your voice. You cannot speak a word of power!",
            Curse::Gale => {
                "A howling, unnatural wind tears through the halls and SNUFFS OUT YOUR TORCH!"
            }
        }
    }
    pub fn end(self) -> &'static str {
        match self {
            Curse::Forgetting => "Your memories come flooding back.",
            Curse::Figments => "The phantoms melt back into the stone.",
            Curse::Darkness => "Your torch flares back to life.",
            Curse::Vertigo => "The floor stills beneath you.",
            Curse::Leaden => "The weight lifts from your limbs.",
            Curse::Hush => "Your voice returns to you.",
            Curse::Gale => "The unnatural wind dies away. Press T to relight your torch.",
        }
    }
    pub fn color(self) -> Rgb {
        match self {
            Curse::Forgetting => Rgb(170, 140, 255),
            Curse::Figments => Rgb(220, 120, 255),
            Curse::Darkness => Rgb(150, 150, 190),
            Curse::Vertigo => Rgb(255, 170, 90),
            Curse::Leaden => Rgb(170, 170, 140),
            Curse::Hush => Rgb(120, 200, 255),
            Curse::Gale => Rgb(190, 210, 230),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub struct ActiveCurse {
    pub kind: Curse,
    pub turns: i32,
}

/// A phantom monster: looks real, cannot hurt you, vanishes when touched.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Figment {
    pub id: u32,
    pub kind: usize,
    pub x: i32,
    pub y: i32,
    pub rx: f32,
    pub ry: f32,
}

#[derive(serde::Serialize, serde::Deserialize)]
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
    /// Equipped unique ring and amulet (indices into UNIQUES).
    pub ring: Option<u8>,
    pub neck: Option<u8>,
    /// The Undying power has been spent on this floor.
    pub undying_used: bool,
    /// Extra ticks added to the next action (frozen by a frost attack).
    pub slow: f64,
    /// Permanent boosts stolen from slain overlords.
    pub extra_ac: i32,
    pub extra_crit: f32,
    /// Turns of Monster Vision left.
    pub vision: i32,
    /// Turns of Battle Cry fury left (+50% damage).
    #[serde(default)]
    pub fury: i32,
    /// The Soul Anchor has been spent this descent.
    #[serde(default)]
    pub anchor_used: bool,
    /// Maximum health currently bound up in keeping thralls alive.
    #[serde(default)]
    pub thrall_upkeep: i32,
    /// The wind blew your torch out: you see nothing until you relight it.
    #[serde(default)]
    pub torch_out: bool,
    /// Turns held fast by a web or a grip: you can fight but not move.
    #[serde(default)]
    pub rooted: i32,
    /// Turns half-blinded by grit or darkness.
    #[serde(default)]
    pub blind: i32,
    /// Paralysis cannot strike again before this world time.
    #[serde(default)]
    pub para_guard: f64,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Game {
    /// Last Rites switched off for now (Shift+C), e.g. to keep bodies to raise.
    #[serde(default)]
    pub rites_off: bool,
    #[serde(skip, default = "fresh_rng")]
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
    #[serde(skip)]
    pub sounds: Vec<SoundEv>,
    #[serde(skip, default = "Fx::new")]
    pub fx: Fx,
    #[serde(skip)]
    pub dmap: Vec<i32>,
    /// How far sound travels to reach you from each tile, in half-tiles
    /// (corners and doors muffle it); see `Level::sound_field`.
    #[serde(skip)]
    pub smap: Vec<u16>,
    pub next_id: u32,
    #[serde(skip)]
    pub over: Option<Outcome>,
    pub kills: u32,
    pub max_depth: u32,
    #[serde(skip, default = "Cam::default")]
    pub cam: Cam,
    #[serde(skip)]
    pub queued: Option<Action>,
    pub combat_heat: f32,
    pub room_size: f32,
    /// How far sound carries around you, in tiles: the longest clear run in
    /// any of the eight directions (a long hallway, or across a big hall).
    #[serde(skip)]
    pub echo_len: f32,
    /// Keen Hearing: when you can next listen intently.
    #[serde(default)]
    pub listen_ready: f64,
    /// Sounds you picked out by listening.
    #[serde(skip)]
    pub pings: Vec<Ping>,
    /// Seconds an arrow takes to reach what the next strike hits.
    #[serde(skip)]
    strike_delay: f32,
    last_fight_msg: u64,
    /// Chance that each floor's unique monster walks its halls, rolled at the start of the run.
    pub unique_chance: [f32; 10],
    /// Unique items already handed out this run.
    pub found_uniques: Vec<usize>,
    /// Index into DIFFICULTIES.
    #[serde(default)]
    pub difficulty: u8,
    /// Kills this run by monster name, for the bestiary.
    #[serde(default)]
    pub slain: HashMap<String, u32>,
    /// Bloodlust fired: the current action costs no time.
    #[serde(skip)]
    free_turn: bool,
    /// Wall style of depths 1-9, dealt out at the start of the run.
    #[serde(default = "crate::data::classic_themes")]
    pub themes: Vec<u8>,
    /// The living dungeon's torments, once you carry the Amulet.
    #[serde(default)]
    pub curses: Vec<ActiveCurse>,
    #[serde(default)]
    pub figments: Vec<Figment>,
    /// Map memory taken away by Forgetting, given back when it ends.
    #[serde(default)]
    pub forgotten: Vec<bool>,
    /// Corpses offered by the last Raise Dead, waiting for the player to pick one.
    #[serde(skip)]
    pub raise_menu: Vec<RaiseOption>,
    /// The corpse picked from the raise menu.
    #[serde(skip)]
    raise_pick: Option<(i32, i32)>,
    /// Smoke curling from your torch (presentation only).
    #[serde(skip)]
    pub smoke: Vec<Smoke>,
    #[serde(skip)]
    smoke_acc: f32,
    /// Seconds since the torch was snuffed, for the dying wisp from the wick.
    #[serde(skip)]
    pub snuff_age: f32,
    /// Seconds since the torch was relit: flint sparks, then the flame catches.
    #[serde(skip, default = "long_ago")]
    pub light_age: f32,
    /// The dying flame has gone out in its puff of smoke.
    #[serde(skip)]
    puffed: bool,
    /// The walls' eyes cannot stare again before this world time.
    #[serde(default)]
    pub stare_cd: f64,
    /// The eye that is staring at you, and until when (world time).
    #[serde(skip)]
    pub stare: Option<(i32, i32, f64)>,
    /// A camera swoop between views: (seconds elapsed, heading to overhead).
    #[serde(skip)]
    pub view_anim: Option<(f32, bool)>,
    /// The floors that hold a ward prism this run (one in each of 1-3, 4-5, 6-8, 9-10).
    #[serde(default)]
    pub ward_depths: Vec<u32>,
    /// Ward prisms broken so far (each weakens the Warden).
    #[serde(default)]
    pub wards_destroyed: u8,
    /// Accumulator for the souls streaming out of a ward.
    #[serde(skip)]
    soul_acc: f32,
}

/// How long the camera takes to swoop between first-person and overhead.
pub const VIEW_SWOOP: f32 = 0.7;

fn long_ago() -> f32 {
    99.0
}

fn fresh_rng() -> StdRng {
    StdRng::from_entropy()
}

/// Where a run in progress is saved when you quit.
pub fn save_path() -> std::path::PathBuf {
    crate::profile::Profile::path().with_file_name("savegame.json")
}

pub fn has_save() -> bool {
    save_path().exists()
}

pub fn delete_save() {
    let _ = std::fs::remove_file(save_path());
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
        Item::Scroll(if s < 0.33 {
            Scroll::Teleport
        } else if s < 0.6 {
            Scroll::Mapping
        } else if s < 0.82 {
            Scroll::Fire
        } else {
            Scroll::Vision
        })
    } else if r < 0.82 {
        let opts: Vec<usize> = (0..WEAPONS.len())
            .filter(|&i| WEAPONS[i].depth <= depth + 1)
            .collect();
        let def = *opts.choose(rng).unwrap_or(&1);
        let mut bonus = rng.gen_range(0..=(depth as i32 / 3 + luck / 2));
        if rng.gen_bool(0.1) {
            bonus = -1;
        }
        Item::Weapon(Gear::plain(def, bonus))
    } else {
        let opts: Vec<usize> = (0..ARMORS.len())
            .filter(|&i| ARMORS[i].depth <= depth + 1)
            .collect();
        let def = *opts.choose(rng).unwrap_or(&2);
        let bonus = rng.gen_range(0..=(depth as i32 / 4 + luck / 2));
        Item::Armor(Gear::plain(def, bonus))
    }
}

/// A random weapon or armour piece (champion drops).
pub fn random_gear(depth: u32, rng: &mut StdRng, luck: i32) -> Item {
    loop {
        match random_item(depth, rng, luck) {
            Item::Weapon(mut g) => {
                g.bonus = g.bonus.max(0) + 1;
                return Item::Weapon(g);
            }
            Item::Armor(mut g) => {
                g.bonus += 1;
                return Item::Armor(g);
            }
            _ => {}
        }
    }
}

/// Choose a monster kind for `depth`, favouring creatures at home there.
fn pick_kind(kinds: &[usize], depth: u32, rng: &mut StdRng) -> usize {
    use rand::distributions::{Distribution, WeightedIndex};
    let ws: Vec<f32> = kinds
        .iter()
        .map(|&k| spawn_weight(&MONSTERS[k], depth))
        .collect();
    match WeightedIndex::new(&ws) {
        Ok(w) => kinds[w.sample(rng)],
        Err(_) => *kinds.choose(rng).unwrap_or(&0),
    }
}

fn populate(
    lv: &mut Level,
    rng: &mut StdRng,
    next_id: &mut u32,
    luck: i32,
    time: f64,
    unique: bool,
    hp_mult: f32,
) {
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
        let kind = pick_kind(&kinds, depth, rng);
        let pack = if matches!(
            MONSTERS[kind].glyph,
            'r' | 'k' | 'g' | 'w' | 's' | 'b' | 'c' | 'j'
        ) && rng.gen_bool(0.35)
        {
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
                m.scale_hp(hp_mult);
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
        lv.items.push(FloorItem {
            x: amulet_pos.0,
            y: amulet_pos.1,
            item: Item::Amulet,
        });
        let room = lv.rooms.iter().find(|r| r.contains(dx, dy)).copied();
        let pos = room
            .and_then(|r| lv.random_floor_in(&r, rng))
            .unwrap_or_else(|| lv.random_floor(rng));
        let mut w = Monster::new(*next_id, M_WARDEN, pos.0, pos.1, time);
        *next_id += 1;
        w.awake = false;
        w.scale_hp(hp_mult);
        lv.monsters.push(w);
    }

    // This floor's unique monster, if fate allows it to walk these halls.
    if unique && (1..=10).contains(&depth) {
        let kind = M_UNIQUE0 + depth as usize - 1;
        let mut placed = false;
        for _ in 0..40 {
            let ri = rng.gen_range(1..n_rooms);
            let room = lv.rooms[ri];
            if room.contains(lv.down.0, lv.down.1) && depth == 10 {
                continue;
            }
            if let Some((x, y)) = lv.random_floor_in(&room, rng) {
                let mut m = Monster::new(*next_id, kind, x, y, time);
                *next_id += 1;
                // Uniques roam their floor instead of sleeping.
                m.awake = true;
                m.scale_hp(hp_mult);
                lv.monsters.push(m);
                placed = true;
                break;
            }
        }
        lv.has_unique = placed;
    }

    fill_features(lv, rng, next_id, luck, time, hp_mult, &kinds);

    // A monster closet: a big room packed wall to wall with sleeping monsters.
    if depth >= 2 && depth <= 10 && rng.gen::<f32>() < 0.12 + depth as f32 * 0.03 {
        place_closet(lv, rng, next_id, &kinds, time, hp_mult);
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
        lv.traps.push(Trap {
            x,
            y,
            kind,
            revealed: false,
        });
    }
}

/// Set a ward prism in the middle of some room, clear of stairs and walls.
pub fn place_ward(lv: &mut Level, rng: &mut StdRng) {
    let (ux, uy) = lv.up;
    let (dx, dy) = lv.down;
    let mut spots = Vec::new();
    // Ideally a roomy hall of its own; failing that, any open spot in a room.
    for strict in [true, false] {
        for (i, r) in lv.rooms.iter().enumerate().skip(1) {
            if strict
                && (r.w < 5
                    || r.h < 5
                    || r.contains(ux, uy)
                    || r.contains(dx, dy)
                    || lv.closet.map_or(false, |c| c.x == r.x && c.y == r.y))
            {
                continue;
            }
            let m = if strict { 2 } else { 1 };
            for y in r.y + m..r.y + r.h - m {
                for x in r.x + m..r.x + r.w - m {
                    if (x, y) == (ux, uy) || (x, y) == (dx, dy) {
                        continue;
                    }
                    let clear = DIRS
                        .iter()
                        .all(|&(ox, oy)| lv.tile(x + ox, y + oy) == Tile::Floor);
                    if clear
                        && lv.tile(x, y) == Tile::Floor
                        && lv.monster_at(x, y).is_none()
                        && !lv.items.iter().any(|it| it.x == x && it.y == y)
                        && !lv.traps.iter().any(|t| t.x == x && t.y == y)
                    {
                        spots.push((i, x, y));
                    }
                }
            }
        }
        if !spots.is_empty() {
            break;
        }
    }
    let Some(&(i, x, y)) = spots.choose(rng) else {
        return;
    };
    let room = lv.rooms[i];
    lv.set(x, y, Tile::Ward);
    lv.clouds.retain(|c| (c.x, c.y) != (x, y));
    lv.ward = Some(Ward {
        x,
        y,
        room,
        state: WardState::Intact,
        guardian: None,
        seen: false,
    });
}

/// Stock the special rooms: bones that rise, racks of arms, scrolls, shrines,
/// trapped galleries with a prize, beasts' dens and spore gardens.
fn fill_features(
    lv: &mut Level,
    rng: &mut StdRng,
    next_id: &mut u32,
    luck: i32,
    time: f64,
    hp_mult: f32,
    kinds: &[usize],
) {
    let depth = lv.depth;
    let feats = lv.features.clone();
    let (ux, uy) = lv.up;
    let (dx, dy) = lv.down;
    for f in feats {
        let r = f.room;
        let spot = |lv: &Level, rng: &mut StdRng| -> Option<(i32, i32)> {
            for _ in 0..40 {
                let (x, y) = (rng.gen_range(r.x..r.x + r.w), rng.gen_range(r.y..r.y + r.h));
                if lv.tile(x, y) == Tile::Floor
                    && (x, y) != (ux, uy)
                    && (x, y) != (dx, dy)
                    && lv.monster_at(x, y).is_none()
                    && !lv.items.iter().any(|i| i.x == x && i.y == y)
                {
                    return Some((x, y));
                }
            }
            None
        };
        match f.kind {
            RoomKind::Ossuary => {
                let dead: Vec<usize> = kinds
                    .iter()
                    .copied()
                    .filter(|&k| MONSTERS[k].death == DeathKind::Undead)
                    .collect();
                let pool = if dead.is_empty() {
                    kinds.to_vec()
                } else {
                    dead
                };
                for _ in 0..rng.gen_range(5..=9) {
                    if let (Some((x, y)), Some(&kind)) = (spot(lv, rng), pool.choose(rng)) {
                        // Some of the bones are not as still as they look.
                        let revive_at = if rng.gen_bool(0.5) {
                            time + rng.gen_range(30.0..200.0) * 100.0
                        } else {
                            -1.0
                        };
                        lv.corpses.push(Corpse {
                            kind,
                            x,
                            y,
                            revive_at,
                            raises: 0,
                        });
                    }
                }
            }
            RoomKind::Armory => {
                for _ in 0..rng.gen_range(2..=3) {
                    if let Some((x, y)) = spot(lv, rng) {
                        let item = random_gear(depth + 1, rng, luck);
                        lv.items.push(FloorItem { x, y, item });
                    }
                }
            }
            RoomKind::Library => {
                for _ in 0..rng.gen_range(2..=3) {
                    if let Some((x, y)) = spot(lv, rng) {
                        let sc = *[
                            Scroll::Teleport,
                            Scroll::Mapping,
                            Scroll::Fire,
                            Scroll::Vision,
                        ]
                        .choose(rng)
                        .unwrap();
                        lv.items.push(FloorItem {
                            x,
                            y,
                            item: Item::Scroll(sc),
                        });
                    }
                }
            }
            RoomKind::Shrine => {
                for k in 0..rng.gen_range(1..=2) {
                    if let Some((x, y)) = spot(lv, rng) {
                        let item = if k == 0 {
                            Item::Potion(Potion::Healing)
                        } else {
                            Item::Potion(Potion::Mana)
                        };
                        lv.items.push(FloorItem { x, y, item });
                    }
                }
            }
            RoomKind::TrapHall => {
                if let Some((x, y)) = spot(lv, rng) {
                    let item = random_gear(depth + 2, rng, luck);
                    lv.items.push(FloorItem { x, y, item });
                }
                for _ in 0..rng.gen_range(4..=7) {
                    if let Some((x, y)) = spot(lv, rng) {
                        if lv.traps.iter().any(|t| t.x == x && t.y == y) {
                            continue;
                        }
                        let kind = *[TrapKind::Spikes, TrapKind::Gas, TrapKind::Fire]
                            .choose(rng)
                            .unwrap();
                        lv.traps.push(Trap {
                            x,
                            y,
                            kind,
                            revealed: false,
                        });
                    }
                }
            }
            RoomKind::Den => {
                let beasts: Vec<usize> = kinds
                    .iter()
                    .copied()
                    .filter(|&k| {
                        MONSTERS[k].faction == Faction::Beast
                            || MONSTERS[k].faction == Faction::Vermin
                    })
                    .collect();
                if let Some(&kind) = beasts.choose(rng) {
                    for _ in 0..rng.gen_range(2..=4) {
                        if let Some((x, y)) = spot(lv, rng) {
                            let mut m = Monster::new(*next_id, kind, x, y, time);
                            *next_id += 1;
                            m.scale_hp(hp_mult);
                            lv.monsters.push(m);
                        }
                    }
                }
                for _ in 0..3 {
                    if let (Some((x, y)), Some(&kind)) = (spot(lv, rng), kinds.choose(rng)) {
                        lv.corpses.push(Corpse {
                            kind,
                            x,
                            y,
                            revive_at: -1.0,
                            raises: 0,
                        });
                    }
                }
            }
            RoomKind::Garden => {
                for y in r.y..r.y + r.h {
                    for x in r.x..r.x + r.w {
                        if lv.tile(x, y) == Tile::Floor
                            && (x, y) != (ux, uy)
                            && (x, y) != (dx, dy)
                            && rng.gen_bool(0.25)
                        {
                            lv.clouds.push(Cloud {
                                x,
                                y,
                                until: f64::MAX / 4.0,
                            });
                        }
                    }
                }
            }
            RoomKind::Chamber => {
                // The prize behind the door.
                let inner = Rect {
                    x: r.x + 2,
                    y: r.y + 2,
                    w: (r.w - 4).max(1),
                    h: (r.h - 4).max(1),
                };
                for k in 0..2 {
                    if let Some((x, y)) = lv.random_floor_in(&inner, rng) {
                        if (x, y) == (dx, dy) || (x, y) == (ux, uy) {
                            continue;
                        }
                        let item = if k == 0 {
                            random_gear(depth + 2, rng, luck)
                        } else {
                            random_item(depth + 1, rng, luck)
                        };
                        lv.items.push(FloorItem { x, y, item });
                    }
                }
            }
            RoomKind::Pillars | RoomKind::Rubble => {}
        }
    }
}

/// Fill the largest suitable room with monsters and shut its doors.
fn place_closet(
    lv: &mut Level,
    rng: &mut StdRng,
    next_id: &mut u32,
    kinds: &[usize],
    time: f64,
    hp_mult: f32,
) {
    if kinds.is_empty() {
        return;
    }
    let (ux, uy) = lv.up;
    let (dx, dy) = lv.down;
    let best = lv
        .rooms
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, r)| {
            r.w * r.h >= 30
                && !r.contains(dx, dy)
                && !r.contains(ux, uy)
                && !lv
                    .features
                    .iter()
                    .any(|f| f.room.x == r.x && f.room.y == r.y)
        })
        .max_by_key(|(_, r)| r.w * r.h)
        .map(|(i, r)| (i, *r));
    let Some((_, room)) = best else { return };
    // Doorways become closed doors so the closet is a surprise.
    for x in room.x - 1..=room.x + room.w {
        for y in room.y - 1..=room.y + room.h {
            let edge =
                x == room.x - 1 || x == room.x + room.w || y == room.y - 1 || y == room.y + room.h;
            if !edge || lv.tile(x, y) != Tile::Floor {
                continue;
            }
            let horiz = lv.tile(x - 1, y).opaque() && lv.tile(x + 1, y).opaque();
            let vert = lv.tile(x, y - 1).opaque() && lv.tile(x, y + 1).opaque();
            if horiz || vert {
                lv.set(x, y, Tile::DoorClosed);
            }
        }
    }
    let n = ((room.w * room.h) / 4).clamp(8, 18);
    let mut placed = 0;
    for _ in 0..n * 3 {
        if placed >= n {
            break;
        }
        let Some((x, y)) = lv.random_floor_in(&room, rng) else {
            continue;
        };
        if lv.monster_at(x, y).is_some() || lv.items.iter().any(|i| i.x == x && i.y == y) {
            continue;
        }
        let kind = pick_kind(kinds, lv.depth, rng);
        let mut m = Monster::new(*next_id, kind, x, y, time);
        *next_id += 1;
        m.awake = rng.gen_bool(0.15);
        let roll: f32 = rng.gen();
        if roll < 0.02 {
            m.promote_overlord();
        } else if roll < 0.10 {
            m.promote_champion();
        }
        m.scale_hp(hp_mult);
        lv.monsters.push(m);
        placed += 1;
    }
    if placed > 0 {
        lv.closet = Some(room);
    }
}

/// Cartographer: reveal the way down, and with rank 2 every room.
fn chart_level(lv: &mut Level, rank: i32) {
    if rank <= 0 {
        return;
    }
    let mark = |lv: &mut Level, x0: i32, y0: i32, x1: i32, y1: i32| {
        for y in y0..=y1 {
            for x in x0..=x1 {
                if lv.inb(x, y) {
                    let i = lv.idx(x, y);
                    lv.explored[i] = true;
                }
            }
        }
    };
    let (dx, dy) = lv.down;
    mark(lv, dx - 1, dy - 1, dx + 1, dy + 1);
    if rank >= 2 {
        let rooms = lv.rooms.clone();
        for r in rooms {
            mark(lv, r.x - 1, r.y - 1, r.x + r.w, r.y + r.h);
        }
    }
}

// ---------------------------------------------------------------- game

impl Game {
    pub fn new(class: usize, mut bonus: Bonuses, difficulty: u8) -> Game {
        let difficulty = difficulty.min(DIFFICULTIES.len() as u8 - 1);
        let hp_mult = DIFFICULTIES[difficulty as usize].hp;
        // Only this class's altar skills come along.
        bonus.skills.retain(|s| s.class() == Some(class));
        let seed: u64 = rand::thread_rng().gen();
        let mut rng = StdRng::seed_from_u64(seed);
        let mut next_id = 1u32;
        let mut unique_chance = [0.0f32; 10];
        for c in unique_chance.iter_mut() {
            *c = rng.gen_range(0.15..0.55);
        }
        let themes = roll_themes(&mut rng);
        let mut level = generate(1, &mut rng);
        level.style = themes[0];
        let u1 = rng.gen::<f32>() < unique_chance[0];
        populate(
            &mut level,
            &mut rng,
            &mut next_id,
            bonus.luck,
            0.0,
            u1,
            hp_mult,
        );
        // One ward prism in each band of floors.
        let ward_depths = vec![
            rng.gen_range(1..=3),
            rng.gen_range(4..=5),
            rng.gen_range(6..=8),
            rng.gen_range(9..=10),
        ];
        if ward_depths.contains(&1) {
            place_ward(&mut level, &mut rng);
        }
        level.theme_doors();
        chart_level(&mut level, bonus.carto);
        let cd = &CLASSES[class];
        let mut inv = Vec::new();
        for _ in 0..(1 + bonus.potions) {
            inv.push(Item::Potion(Potion::Healing));
        }
        if cd.mp >= 14 {
            inv.push(Item::Potion(Potion::Mana));
        }
        for _ in 0..bonus.scavenger {
            let sc = *[
                Scroll::Teleport,
                Scroll::Mapping,
                Scroll::Fire,
                Scroll::Vision,
            ]
            .choose(&mut rng)
            .unwrap();
            inv.push(Item::Scroll(sc));
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
            wpn: Gear::plain(cd.weapon, bonus.heirloom),
            arm: Gear::plain(cd.armor, bonus.heirloom),
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
            ring: None,
            neck: None,
            undying_used: false,
            slow: 0.0,
            extra_ac: 0,
            extra_crit: 0.0,
            vision: 0,
            fury: 0,
            anchor_used: false,
            thrall_upkeep: 0,
            torch_out: false,
            rooted: 0,
            blind: 0,
            para_guard: 0.0,
        };
        let mut g = Game {
            rites_off: false,
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
            smap: Vec::new(),
            next_id,
            over: None,
            kills: 0,
            max_depth: 1,
            cam: Cam {
                x: 0.0,
                y: 0.0,
                ang: 0.0,
                walk: 0.0,
                swing: 0.0,
            },
            queued: None,
            combat_heat: 0.0,
            room_size: 0.4,
            echo_len: 0.0,
            listen_ready: 0.0,
            pings: Vec::new(),
            strike_delay: 0.0,
            last_fight_msg: 0,
            unique_chance,
            found_uniques: Vec::new(),
            difficulty,
            slain: HashMap::new(),
            free_turn: false,
            themes,
            curses: Vec::new(),
            figments: Vec::new(),
            forgotten: Vec::new(),
            raise_menu: Vec::new(),
            raise_pick: None,
            smoke: Vec::new(),
            smoke_acc: 0.0,
            snuff_age: 0.0,
            light_age: 99.0,
            puffed: false,
            stare_cd: 0.0,
            stare: None,
            view_anim: None,
            ward_depths,
            wards_destroyed: 0,
            soul_acc: 0.0,
        };
        g.enter_level();
        g.announce_unique();
        g.msg(
            format!("You descend into the dungeon of GRAZZARK as a {}.", cd.name),
            Rgb(220, 200, 150),
        );
        if difficulty > 0 {
            let dd = &DIFFICULTIES[difficulty as usize];
            g.msg(format!("Difficulty: {}. {}", dd.name, dd.desc), dd.color);
        }
        g.msg(
            "Claim the Amulet on level 10 and return to the surface. Press ? for help.",
            Rgb(170, 170, 170),
        );
        g
    }

    // ------------------------------------------------------------ save / load

    /// Write the run to disk so it can be continued later.
    pub fn save(&self) -> bool {
        if self.over.is_some() {
            return false;
        }
        let p = save_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        match serde_json::to_string(self) {
            Ok(s) => {
                let tmp = p.with_extension("json.tmp");
                std::fs::write(&tmp, s).is_ok() && std::fs::rename(&tmp, &p).is_ok()
            }
            Err(_) => false,
        }
    }

    /// Load a saved run. The save is removed once loaded, so a death is final.
    pub fn load() -> Option<Game> {
        let s = std::fs::read_to_string(save_path()).ok()?;
        delete_save();
        let mut g: Game = serde_json::from_str(&s).ok()?;
        g.queued = None;
        g.p.next_act = g.p.next_act.min(g.time);
        g.enter_level_keep_facing();
        g.msg(
            "You return to the dungeon of GRAZZARK. Your journey continues...",
            Rgb(220, 200, 150),
        );
        Some(g)
    }

    fn enter_level_keep_facing(&mut self) {
        let (px, py) = (self.p.x, self.p.y);
        for m in self.level.monsters.iter_mut() {
            m.rx = m.x as f32;
            m.ry = m.y as f32;
        }
        self.refresh_view();
        self.cam.x = px as f32 + 0.5;
        self.cam.y = py as f32 + 0.5;
        self.cam.ang = facing_angle(self.p.facing);
    }

    // ------------------------------------------------------------ stats

    pub fn diff(&self) -> &'static DiffDef {
        &DIFFICULTIES[(self.difficulty as usize).min(DIFFICULTIES.len() - 1)]
    }
    /// How many slots your pack has.
    pub fn inv_cap(&self) -> usize {
        MAX_INV + 3 * self.bonus.pockets.max(0) as usize
    }

    /// How many potions of one kind share a slot (Potion Belt).
    pub fn stack_size(&self) -> usize {
        1 + self.bonus.belt.max(0) as usize
    }

    /// The pack as its slots: (index in `p.inv` of the slot's first item,
    /// how many items are in it). Potions of a kind fill a slot up to the
    /// stack size before starting another; everything else takes a slot of
    /// its own. Slots are in the order their first item appears.
    pub fn inv_slots(&self) -> Vec<(usize, usize)> {
        slots_of(&self.p.inv, self.stack_size())
    }

    /// Pack slots still free.
    pub fn free_slots(&self) -> usize {
        self.inv_cap().saturating_sub(self.inv_slots().len())
    }

    /// Is there room in the pack for `it` (a free slot, or a potion slot
    /// with room on the belt)?
    pub fn can_carry(&self, it: &Item) -> bool {
        let mut inv = self.p.inv.clone();
        inv.push(*it);
        slots_of(&inv, self.stack_size()).len() <= self.inv_cap()
    }

    /// The item in pack slot `s` (as lettered in the inventory screen), as an
    /// index into `p.inv`.
    pub fn slot_item(&self, s: usize) -> Option<usize> {
        self.inv_slots().get(s).map(|&(i, _)| i)
    }

    /// Sort the pack by type: weapons, armour, rings and amulets, potions,
    /// scrolls. Takes no time.
    pub fn sort_inv(&mut self) {
        fn key(it: &Item) -> (u8, u8) {
            match it {
                Item::Weapon(_) => (0, 0),
                Item::Armor(_) => (1, 0),
                Item::Trinket(_) => (2, 0),
                Item::Potion(p) => (3, *p as u8),
                Item::Scroll(s) => (4, *s as u8),
                Item::Amulet => (5, 0),
            }
        }
        self.p.inv.sort_by_key(key);
        self.msg("You sort your pack by type.", GREY);
    }
    /// Below a quarter of your health (Last Stand).
    pub fn last_stand(&self) -> bool {
        self.bonus.laststand > 0 && self.p.hp * 4 < self.p.max_hp
    }
    pub fn cdef(&self) -> &'static ClassDef {
        &CLASSES[self.p.class]
    }
    pub fn wdef(&self) -> &'static WeaponDef {
        &WEAPONS[self.p.wpn.def]
    }
    pub fn sight(&self) -> i32 {
        if self.p.torch_out {
            return 0;
        }
        let s = BASE_SIGHT + self.bonus.sight;
        if self.p.blind > 0 {
            return s.min(1);
        }
        if self.cursed(Curse::Darkness) {
            s.min(2)
        } else {
            s
        }
    }
    /// Is this torment upon you right now?
    pub fn cursed(&self, c: Curse) -> bool {
        self.curses.iter().any(|a| a.kind == c)
    }
    /// How far the living dungeon has rotted around you, 0..1: nothing until
    /// you take the Amulet, then thicker on every floor nearer the surface.
    pub fn corruption(&self) -> f32 {
        if !self.p.has_amulet || self.depth >= 10 {
            0.0
        } else {
            0.15 + 0.75 * (9 - self.depth.max(1)) as f32 / 8.0
        }
    }
    /// Wall style for a depth in this run.
    pub fn style_for(&self, depth: u32) -> u8 {
        if depth >= 10 {
            SANCTUM_STYLE
        } else {
            self.themes
                .get(depth.max(1) as usize - 1)
                .copied()
                .unwrap_or(0)
        }
    }
    pub fn floor_name(&self) -> &'static str {
        floor_name(&self.themes, self.depth)
    }
    pub fn hit_bonus(&self) -> i32 {
        self.cdef().hit + self.p.level + self.wdef().hit + self.p.wpn.bonus
    }
    pub fn player_ac(&self) -> i32 {
        ARMORS[self.p.arm.def].ac
            + self.p.arm.bonus
            + self.bonus.ac
            + self.p.extra_ac
            + if self.last_stand() {
                self.bonus.laststand
            } else {
                0
            }
    }
    pub fn dmg_bonus(&self) -> i32 {
        self.p.wpn.bonus
            + self.p.extra_dmg
            + self.bonus.dmg
            + self.p.level / 3
            + if self.has(UPower::Wrath) { 3 } else { 0 }
    }
    pub fn crit_chance(&self) -> f32 {
        self.wdef().crit
            + self.bonus.crit
            + if self.p.class == C_ROGUE { 0.08 } else { 0.0 }
            + if self.has(UPower::Executioner) {
                0.12
            } else {
                0.0
            }
            + self.p.extra_crit
    }
    /// How fast the dead rise while you carry the Amulet: the normal rate on
    /// depth 10, faster and faster as you climb toward the surface.
    pub fn respawn_factor(&self) -> f64 {
        if !self.amulet_rage() {
            return 1.0;
        }
        let d = self.depth.clamp(1, 10) as f64;
        0.3 + 0.7 * (d - 1.0) / 9.0
    }
    /// Range of Monster Vision, in tiles.
    pub const VISION_RANGE: i32 = 20;
    /// Is monster `m` revealed by Monster Vision?
    pub fn vision_shows(&self, m: &Monster) -> bool {
        self.p.vision > 0 && m.alive && cheb(m.x, m.y, self.p.x, self.p.y) <= Self::VISION_RANGE
    }
    /// Indices into level.items of the items lying at your feet.
    pub fn items_here(&self) -> Vec<usize> {
        let (px, py) = (self.p.x, self.p.y);
        self.level
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.x == px && i.y == py)
            .map(|(i, _)| i)
            .collect()
    }
    /// Unique items currently equipped.
    pub fn equipped_uniques(&self) -> Vec<usize> {
        [self.p.wpn.uniq, self.p.arm.uniq, self.p.ring, self.p.neck]
            .iter()
            .flatten()
            .map(|&u| u as usize)
            .collect()
    }
    /// Does an equipped unique grant this power?
    pub fn has(&self, pw: UPower) -> bool {
        [self.p.wpn.uniq, self.p.arm.uniq, self.p.ring, self.p.neck]
            .iter()
            .flatten()
            .any(|&u| UNIQUES[u as usize].power == pw)
    }
    pub fn stealth(&self) -> i32 {
        self.cdef().stealth + if self.has(UPower::Shadow) { 3 } else { 0 }
    }
    pub fn luck(&self) -> i32 {
        self.bonus.luck + if self.has(UPower::Fortune) { 2 } else { 0 }
    }
    /// Mana cost of a skill after the Arcane power.
    pub fn skill_cost(&self, s: Skill) -> i32 {
        if self.has(UPower::Arcane) {
            s.mp() / 2
        } else {
            s.mp()
        }
    }
    /// Does the Amulet of GRAZZARK turn every hostile monster against the player?
    pub fn amulet_rage(&self) -> bool {
        self.p.has_amulet
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
        let turn = self.turns;
        self.msgs.push(Msg {
            text: t.into(),
            color: c,
            age: 0.0,
            turn,
        });
        // Keep the last 100 turns for the log (always the last few for the HUD).
        let cutoff = turn.saturating_sub(LOG_TURNS);
        let old = self.msgs.iter().take_while(|m| m.turn < cutoff).count();
        let keep_min = 8;
        let drop = old.min(self.msgs.len().saturating_sub(keep_min));
        if drop > 0 {
            self.msgs.drain(..drop);
        }
        if self.msgs.len() > 2000 {
            let n = self.msgs.len() - 2000;
            self.msgs.drain(..n);
        }
    }

    pub fn emit(&mut self, sfx: Sfx, x: i32, y: i32, vol: f32) {
        self.emit_after(sfx, x, y, vol, 0.0);
    }

    /// A sound heard `delay` seconds from now.
    pub fn emit_after(&mut self, sfx: Sfx, x: i32, y: i32, vol: f32, delay: f32) {
        self.sounds.push(SoundEv {
            sfx,
            x: x as f32 + 0.5,
            y: y as f32 + 0.5,
            vol,
            delay,
        });
    }

    fn new_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn mon_index(&self, id: u32) -> Option<usize> {
        self.level
            .monsters
            .iter()
            .position(|m| m.alive && m.id == id)
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
        // Sound paths are the same either way, so one map from you serves
        // every sound on the floor.
        let lim = (self.hear_radius() as u16 + 8) * 2;
        self.smap = self.level.sound_field(px, py, lim);
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
        let mut longest = 0;
        for &(dx, dy) in DIRS.iter() {
            let mut n = 0;
            while n < 24 && !self.level.tile(px + dx * (n + 1), py + dy * (n + 1)).opaque() {
                n += 1;
            }
            longest = longest.max(n);
        }
        self.echo_len = longest as f32;
        self.check_closet();
        self.check_features();
        self.check_ward();
    }

    /// Walking into a special room for the first time.
    fn check_features(&mut self) {
        let (px, py) = (self.p.x, self.p.y);
        let Some(fi) = self
            .level
            .features
            .iter()
            .position(|f| !f.seen && f.room.contains(px, py))
        else {
            return;
        };
        self.level.features[fi].seen = true;
        if let Some(t) = self.level.features[fi].kind.enter_text() {
            self.msg(t, Rgb(210, 200, 160));
        }
    }

    /// The first glimpse inside a monster closet.
    fn check_closet(&mut self) {
        if self.level.closet_found {
            return;
        }
        let Some(r) = self.level.closet else { return };
        let seen = (r.y..r.y + r.h).any(|y| (r.x..r.x + r.w).any(|x| self.level.is_visible(x, y)));
        if !seen {
            return;
        }
        self.level.closet_found = true;
        let (px, py) = (self.p.x, self.p.y);
        let n = self
            .level
            .monsters
            .iter()
            .filter(|m| m.alive && m.faction != Faction::Ally && r.contains(m.x, m.y))
            .count();
        if n < 4 {
            return;
        }
        self.msg(
            format!(
                "A MONSTER CLOSET! The room is packed with {} creatures...",
                n
            ),
            Rgb(255, 110, 60),
        );
        self.fx.shake = self.fx.shake.max(0.6);
        self.emit(Sfx::Rumble, px, py, 0.8);
        self.combat_heat = self.combat_heat.max(0.8);
    }

    // ------------------------------------------------------------ thralls

    /// Max health a thrall of this kind holds while it lives: more for
    /// stronger monsters, a fifth less for every 10 levels you have, and
    /// about half again for a body raised a second time.
    pub fn thrall_cost(&self, kind: usize, again: bool) -> i32 {
        let base = 1.0 + MONSTERS[kind].xp as f32 / 12.0;
        let lvl = (1.0 - 0.2 * (self.p.level / 10) as f32).max(0.4);
        let c = (base * lvl).round().max(1.0) as i32;
        if again {
            c + (c / 2).max(1)
        } else {
            c
        }
    }

    /// Corpses within reach that will answer your call, nearest first.
    pub fn raise_options(&self) -> Vec<RaiseOption> {
        let (px, py) = (self.p.x, self.p.y);
        let limit = if self.bonus.rebind { 2 } else { 1 };
        let mut v: Vec<(f32, RaiseOption)> = self
            .level
            .corpses
            .iter()
            .filter(|c| {
                dist(c.x, c.y, px, py) <= 5.5
                    && self.level.monster_at(c.x, c.y).is_none()
                    && !(c.x == px && c.y == py)
                    && c.raises < limit
            })
            .map(|c| {
                let again = c.raises > 0;
                (
                    dist(c.x, c.y, px, py),
                    RaiseOption {
                        x: c.x,
                        y: c.y,
                        kind: c.kind,
                        cost: self.thrall_cost(c.kind, again),
                        again,
                    },
                )
            })
            .collect();
        v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        v.into_iter().map(|(_, o)| o).collect()
    }

    /// Bind (or release) max health to match the thralls on this floor.
    pub fn sync_upkeep(&mut self) {
        let want: i32 = self
            .level
            .monsters
            .iter()
            .filter(|m| m.alive && m.thrall)
            .map(|m| self.thrall_cost(m.kind, m.raises >= 2))
            .sum();
        let diff = want - self.p.thrall_upkeep;
        if diff != 0 {
            self.p.max_hp -= diff;
            self.p.thrall_upkeep = want;
            if self.p.max_hp < 1 {
                // Never bind away the last of your life.
                self.p.thrall_upkeep -= 1 - self.p.max_hp;
                self.p.max_hp = 1;
            }
            self.p.hp = self.p.hp.min(self.p.max_hp);
        }
    }

    /// Short-lived minions crumble when their time is up.
    fn expire_minions(&mut self) {
        let now = self.time;
        for i in 0..self.level.monsters.len() {
            let (alive, exp, x, y) = {
                let m = &self.level.monsters[i];
                (m.alive, m.expires, m.x, m.y)
            };
            if alive && exp > 0.0 && exp <= now {
                self.level.monsters[i].alive = false;
                self.fx.bones(x as f32 + 0.5, y as f32 + 0.5);
                if self.level.is_visible(x, y) {
                    self.msg("A minion crumbles back into dust.", Rgb(150, 170, 150));
                }
            }
        }
    }

    /// An overlord thrall in battle: roar its foes into flight, or raise
    /// minions. Each power has its own 25-50 turn cooldown. Returns true if
    /// it spent its action.
    fn thrall_power(&mut self, mi: usize) -> bool {
        let now = self.time;
        let (mx, my, roar_cd, raise_cd, name) = {
            let m = &self.level.monsters[mi];
            (m.x, m.y, m.roar_cd, m.abil_cd, m.def().name)
        };
        let (px, py) = (self.p.x, self.p.y);
        let near = |g: &Game, o: &Monster| {
            o.alive
                && o.faction != Faction::Ally
                && !o.fearless()
                && (cheb(o.x, o.y, mx, my) <= 8 || cheb(o.x, o.y, px, py) <= 8)
                && (g.level.los(o.x, o.y, mx, my) || g.level.los(o.x, o.y, px, py))
        };
        let foes = self.level.monsters.iter().filter(|o| near(self, o)).count();
        if roar_cd <= now && foes >= 2 {
            let idx: Vec<usize> = (0..self.level.monsters.len())
                .filter(|&i| near(self, &self.level.monsters[i]))
                .collect();
            for i in idx {
                let turns = self.rng.gen_range(5..=20);
                let o = &mut self.level.monsters[i];
                o.awake = true;
                o.fear = o.fear.max(turns);
                o.target = None;
            }
            let cd = self.rng.gen_range(25..=50) as f64 * 100.0;
            self.level.monsters[mi].roar_cd = now + cd;
            let (fx, fy) = (mx as f32 + 0.5, my as f32 + 0.5);
            self.fx
                .ring(fx, fy, 40, 3.0, Rgb(190, 120, 255), &['~', '*', '\''], 0.7);
            self.fx.shake = self.fx.shake.max(0.6);
            self.emit(
                Sfx::Voice(MONSTERS[self.level.monsters[mi].kind].voice, Mood::Alert),
                mx,
                my,
                1.0,
            );
            self.emit(Sfx::Rumble, mx, my, 0.7);
            self.msg(
                format!(
                    "Your overlord {} lets out a dreadful roar! Your foes flee in terror!",
                    name
                ),
                Rgb(200, 140, 255),
            );
            return true;
        }
        if raise_cd <= now && foes >= 1 {
            let skel = MONSTERS
                .iter()
                .position(|d| d.name == "skeleton")
                .unwrap_or(0);
            let mut raised = 0;
            for _ in 0..2 {
                let Some((x, y)) = self.free_adjacent(mx, my) else {
                    break;
                };
                let id = self.new_id();
                let mut m = Monster::new(id, skel, x, y, now + 50.0);
                m.faction = Faction::Ally;
                m.summoned = true;
                m.awake = true;
                m.expires = now + 1000.0;
                self.fx
                    .mist(x as f32 + 0.5, y as f32 + 0.5, Rgb(150, 220, 150));
                self.level.monsters.push(m);
                raised += 1;
            }
            if raised == 0 {
                return false;
            }
            let cd = self.rng.gen_range(25..=50) as f64 * 100.0;
            self.level.monsters[mi].abil_cd = now + cd;
            self.emit(Sfx::Revive, mx, my, 0.9);
            if self.level.is_visible(mx, my) {
                self.msg(
                    format!(
                        "Your overlord {} tears minions from the earth to fight for you!",
                        name
                    ),
                    Rgb(160, 230, 160),
                );
            }
            return true;
        }
        false
    }

    // ------------------------------------------------------------ ward prisms

    /// May this monster harm the Warden? Only the necromancer's thralls and
    /// the minions they raise.
    fn may_harm_boss(&self, mi: usize) -> bool {
        let m = &self.level.monsters[mi];
        m.faction == Faction::Ally && (m.thrall || m.expires > 0.0)
    }

    /// The first glimpse of a ward prism.
    fn check_ward(&mut self) {
        let Some(w) = self.level.ward else { return };
        if w.seen || !self.level.is_visible(w.x, w.y) {
            return;
        }
        if let Some(wm) = self.level.ward.as_mut() {
            wm.seen = true;
        }
        if w.state == WardState::Broken {
            self.msg("The shattered remains of a ward prism lie here.", GREY);
        } else {
            self.msg("A prism floats here, souls streaming out of it toward the ceiling, shrieking in torment.", Rgb(200, 170, 255));
            self.msg(
                "It is one of the Warden's four wards. Break it, and Vorth grows weaker.",
                Rgb(170, 150, 220),
            );
        }
    }

    /// You strike the ward prism: the first blow wakes its Guardian and seals
    /// the room; while the Guardian lives, a shield turns every blow aside.
    fn strike_ward(&mut self) -> Option<f64> {
        let Some(w) = self.level.ward else {
            return None;
        };
        self.cam.swing = 1.0;
        let (wx, wy) = (w.x as f32 + 0.5, w.y as f32 + 0.5);
        match w.state {
            WardState::Broken => None,
            WardState::Shielded => {
                self.msg(
                    "A shimmering shield turns your blow aside. The Guardian must fall first!",
                    Rgb(150, 220, 255),
                );
                self.fx
                    .ring(wx, wy, 20, 1.2, Rgb(150, 230, 255), &['+', '*', '.'], 0.4);
                self.emit(Sfx::Clang, w.x, w.y, 0.8);
                Some(self.wdef().speed)
            }
            WardState::Intact => {
                let Some((gx, gy)) = self.free_adjacent(w.x, w.y) else {
                    return Some(100.0);
                };
                let depth = self.depth as i32;
                let id = self.new_id();
                let mut m = Monster::new(id, M_GUARDIAN, gx, gy, self.time + 100.0);
                m.max_hp = 35 + depth * 10;
                m.hp = m.max_hp;
                m.bonus_dmg = depth / 2;
                m.awake = true;
                m.target = Some(Target::Player);
                m.scale_hp(self.diff().hp);
                self.level.monsters.push(m);
                if let Some(wm) = self.level.ward.as_mut() {
                    wm.state = WardState::Shielded;
                    wm.guardian = Some(id);
                }
                self.seal_ward_room();
                self.fx.flash(Rgb(160, 230, 255), 0.6);
                self.fx.ring(
                    wx,
                    wy,
                    60,
                    3.0,
                    Rgb(170, 240, 255),
                    &['*', '+', '/', '\\'],
                    0.9,
                );
                self.fx.light(wx, wy, [1.0, 1.8, 2.4], 5.0, 1.2);
                self.fx.shake = self.fx.shake.max(1.0);
                self.emit(Sfx::Explosion, w.x, w.y, 0.8);
                self.emit(Sfx::Voice(VoiceKind::Chime, Mood::Alert), gx, gy, 1.0);
                self.msg("The prism flares blinding white, and a PRISM GUARDIAN tears itself out of the light!", Rgb(170, 240, 255));
                self.msg(
                    "Walls of crystal slam down across every way out of the room!",
                    Rgb(150, 220, 255),
                );
                self.combat_heat = 1.0;
                Some(self.wdef().speed)
            }
        }
    }

    /// Crystal walls across every opening around the ward's room.
    fn seal_ward_room(&mut self) {
        let Some(w) = self.level.ward else { return };
        let r = w.room;
        for y in r.y - 1..=r.y + r.h {
            for x in r.x - 1..=r.x + r.w {
                let edge = x == r.x - 1 || x == r.x + r.w || y == r.y - 1 || y == r.y + r.h;
                if !edge {
                    continue;
                }
                let t = self.level.tile(x, y);
                let open = t.walkable() || t.is_closed_door();
                if open && self.level.monster_at(x, y).is_none() && (x, y) != (self.p.x, self.p.y) {
                    self.level.crystal.push((x, y, t));
                    self.level.set(x, y, Tile::Crystal);
                    self.fx.burst(
                        x as f32 + 0.5,
                        y as f32 + 0.5,
                        0.5,
                        10,
                        Rgb(150, 230, 255),
                        &['*', '/', '\\'],
                        0.6,
                        0.6,
                        0.5,
                        true,
                    );
                }
            }
        }
        self.refresh_view();
    }

    fn unseal_ward_room(&mut self) {
        let walls = std::mem::take(&mut self.level.crystal);
        for (x, y, t) in walls {
            if self.level.tile(x, y) == Tile::Crystal {
                self.level.set(x, y, t);
                self.fx.burst(
                    x as f32 + 0.5,
                    y as f32 + 0.5,
                    0.5,
                    12,
                    Rgb(170, 240, 255),
                    &['*', '.', '\''],
                    1.0,
                    0.8,
                    1.0,
                    true,
                );
            }
        }
    }

    /// The Guardian is dead: the ward crumbles, its souls fly free, and far
    /// below the Warden screams.
    fn break_ward(&mut self) {
        let Some(w) = self.level.ward else { return };
        if w.state == WardState::Broken {
            return;
        }
        self.unseal_ward_room();
        if let Some(wm) = self.level.ward.as_mut() {
            wm.state = WardState::Broken;
            wm.guardian = None;
        }
        self.level.set(w.x, w.y, Tile::Floor);
        self.wards_destroyed = (self.wards_destroyed + 1).min(4);
        let (wx, wy) = (w.x as f32 + 0.5, w.y as f32 + 0.5);
        self.fx.shake = self.fx.shake.max(1.8);
        self.fx.rise(
            wx,
            wy,
            60,
            Rgb(200, 230, 255),
            &['o', '~', '@', '0', '*'],
            1.2,
        );
        self.fx.burst(
            wx,
            wy,
            0.6,
            30,
            Rgb(160, 200, 230),
            &['/', '\\', '*', '.'],
            1.5,
            1.2,
            1.5,
            true,
        );
        let (px, py) = (self.p.x, self.p.y);
        self.emit(Sfx::Rumble, px, py, 1.0);
        self.emit(Sfx::Bones, w.x, w.y, 0.9);
        // A scream from very far below: quiet, and muffled by all that stone.
        self.emit(Sfx::Voice(VoiceKind::Demon, Mood::Die), px, py, 0.25);
        self.refresh_view();
        self.msg(
            "The floor shakes. The crystal walls shatter, and the ward crumbles to the ground!",
            Rgb(200, 230, 255),
        );
        self.msg(
            "Its souls stream free into the ceiling, weeping with relief.",
            Rgb(190, 210, 240),
        );
        self.msg(
            "From deep, deep below, an enraged scream of pain echoes up through the stone...",
            Rgb(255, 110, 90),
        );
        let n = self.wards_destroyed;
        self.msg(
            format!("Ward {} of 4 destroyed. The Warden grows weaker.", n),
            Rgb(220, 170, 255),
        );
        self.weaken_wardens();
    }

    /// Bring the Warden down to its strength for the wards broken so far,
    /// wherever it waits: here, or on the stashed tenth floor.
    fn weaken_wardens(&mut self) {
        let n = self.wards_destroyed.min(4);
        let apply = |m: &mut Monster| {
            if m.def().boss && m.alive && m.ward_weaken < n {
                let old = WARD_HP[m.ward_weaken as usize];
                let new = WARD_HP[n as usize];
                let base = m.max_hp as f32 / old;
                let new_max = (base * new).round().max(1.0) as i32;
                m.hp =
                    ((m.hp as f32 * new_max as f32 / m.max_hp.max(1) as f32).round() as i32).max(1);
                m.max_hp = new_max;
                m.ward_weaken = n;
            }
        };
        self.level.monsters.iter_mut().for_each(apply);
        if let Some(lv) = self.stash.get_mut(&10) {
            lv.monsters.iter_mut().for_each(apply);
        }
    }

    /// The heart leaves its hall: every ward still standing crumbles to shards.
    fn crumble_all_wards(&mut self) {
        let mut any = false;
        let crumble = |lv: &mut Level| -> bool {
            let Some(w) = lv.ward else { return false };
            if w.state == WardState::Broken {
                return false;
            }
            for (x, y, t) in std::mem::take(&mut lv.crystal) {
                if lv.tile(x, y) == Tile::Crystal {
                    lv.set(x, y, t);
                }
            }
            lv.set(w.x, w.y, Tile::Floor);
            if let Some(wm) = lv.ward.as_mut() {
                wm.state = WardState::Broken;
                wm.guardian = None;
            }
            true
        };
        any |= crumble(&mut self.level);
        for lv in self.stash.values_mut() {
            any |= crumble(lv);
        }
        if any {
            self.msg(
                "Somewhere in the dungeon, the wards still standing shatter into shards.",
                Rgb(200, 180, 240),
            );
            self.refresh_view();
        }
    }

    // ------------------------------------------------------------ the living dungeon

    /// Corridor chokepoints near (cx,cy) where sinew could grow across the way.
    fn sinew_spots(&self, cx: i32, cy: i32, near: i32, far: i32) -> Vec<(i32, i32)> {
        let lv = &self.level;
        let mut v = Vec::new();
        for y in cy - far..=cy + far {
            for x in cx - far..=cx + far {
                let d = cheb(x, y, cx, cy);
                if d < near || d > far || lv.tile(x, y) != Tile::Floor {
                    continue;
                }
                let ns = lv.tile(x, y - 1).walkable()
                    && lv.tile(x, y + 1).walkable()
                    && lv.tile(x - 1, y).opaque()
                    && lv.tile(x + 1, y).opaque();
                let ew = lv.tile(x - 1, y).walkable()
                    && lv.tile(x + 1, y).walkable()
                    && lv.tile(x, y - 1).opaque()
                    && lv.tile(x, y + 1).opaque();
                if !(ns || ew)
                    || (x, y) == (self.p.x, self.p.y)
                    || lv.monster_at(x, y).is_some()
                    || lv.items.iter().any(|i| i.x == x && i.y == y)
                    || lv.traps.iter().any(|t| t.x == x && t.y == y)
                    || self.figment_at(x, y).is_some()
                {
                    continue;
                }
                v.push((x, y));
            }
        }
        v
    }

    fn grow_sinew(&mut self, x: i32, y: i32) {
        let hp = 8 + (self.corruption() * 12.0) as i32;
        self.level.set(x, y, Tile::Sinew);
        self.level.sinews.retain(|s| (s.x, s.y) != (x, y));
        self.level.sinews.push(Sinew { x, y, hp });
    }

    /// The first time you climb onto a floor with the Amulet, the living
    /// dungeon readies itself: biting maws in the floor, sinew across the halls.
    fn infest_level(&mut self) {
        let c = self.corruption();
        if c <= 0.0 {
            return;
        }
        let (px, py) = (self.p.x, self.p.y);
        let maws = 1 + (c * 7.0).round() as usize;
        for _ in 0..maws {
            for _ in 0..30 {
                let (x, y) = self.level.random_floor(&mut self.rng);
                if cheb(x, y, px, py) >= 3
                    && (x, y) != self.level.up
                    && (x, y) != self.level.down
                    && !self.level.traps.iter().any(|t| t.x == x && t.y == y)
                    && !self.level.items.iter().any(|i| i.x == x && i.y == y)
                {
                    self.level.traps.push(Trap {
                        x,
                        y,
                        kind: TrapKind::Maw,
                        revealed: false,
                    });
                    break;
                }
            }
        }
        let mut spots = self.sinew_spots(px, py, 4, 60);
        spots.shuffle(&mut self.rng);
        for (x, y) in spots.into_iter().take((c * 5.0).round() as usize) {
            self.grow_sinew(x, y);
        }
        self.refresh_view();
    }

    /// Hack at a wall of sinew. It takes a few blows to cut through.
    fn cut_sinew(&mut self, x: i32, y: i32) -> Option<f64> {
        let w = self.wdef();
        let dmg = self.rng.gen_range(w.dmg.0..=w.dmg.1) + self.dmg_bonus().max(0);
        let i = match self.level.sinews.iter().position(|s| (s.x, s.y) == (x, y)) {
            Some(i) => i,
            None => {
                self.level.sinews.push(Sinew { x, y, hp: 10 });
                self.level.sinews.len() - 1
            }
        };
        self.level.sinews[i].hp -= dmg.max(1);
        self.cam.swing = 1.0;
        let (px, py) = (self.p.x, self.p.y);
        self.emit(Sfx::Hit, x, y, 0.8);
        self.fx.blood(x as f32 + 0.5, y as f32 + 0.5, 0.5, 12);
        if self.level.sinews[i].hp <= 0 {
            self.level.sinews.remove(i);
            self.level.set(x, y, Tile::Floor);
            let k = self.level.idx(x, y);
            self.level.blood[k] = (self.level.blood[k] + 2.0).min(3.0);
            self.msg(
                "You hack through the sinew! It parts with a wet, shuddering tear.",
                Rgb(230, 110, 110),
            );
            self.fx.blood(x as f32 + 0.5, y as f32 + 0.5, 0.6, 30);
            self.refresh_view();
        } else {
            let left = self.level.sinews[i].hp;
            let how = if left > 10 {
                "barely gives"
            } else if left > 4 {
                "tears"
            } else {
                "is nearly through"
            };
            self.msg(
                format!("You hack at the wall of sinew. It {}.", how),
                Rgb(220, 140, 130),
            );
        }
        self.noise(px, py, 5);
        Some(w.speed)
    }

    /// Once a turn while you carry the Amulet: the dungeon grows sinew across
    /// your path and its eyes try to hold you with their stare.
    fn living_dungeon_tick(&mut self) {
        let c = self.corruption();
        if !self.p.has_amulet || c <= 0.0 {
            return;
        }
        let (px, py) = (self.p.x, self.p.y);
        if self.level.sinews.len() < 12 && self.rng.gen_bool((0.012 + 0.03 * c) as f64) {
            let spots: Vec<(i32, i32)> = self
                .sinew_spots(px, py, 2, 7)
                .into_iter()
                .filter(|&(x, y)| self.level.is_visible(x, y))
                .collect();
            if let Some(&(x, y)) = spots.choose(&mut self.rng) {
                self.grow_sinew(x, y);
                self.msg(
                    "Sinew bursts from the walls and knits itself across the passage!",
                    Rgb(230, 90, 90),
                );
                self.fx.blood(x as f32 + 0.5, y as f32 + 0.5, 0.5, 16);
                self.emit(Sfx::Voice(VoiceKind::Moan, Mood::Alert), x, y, 0.6);
                self.refresh_view();
            }
        }
        if self.time >= self.stare_cd && self.rng.gen_bool((0.04 + 0.08 * c) as f64) {
            let mut eyes = Vec::new();
            for y in py - 6..=py + 6 {
                for x in px - 6..=px + 6 {
                    if self.level.tile(x, y) == Tile::Wall
                        && self.level.is_visible(x, y)
                        && crate::render::wall_eye(x, y, c)
                    {
                        eyes.push((x, y));
                    }
                }
            }
            if let Some(&(x, y)) = eyes.choose(&mut self.rng) {
                self.stare_cd = self.time + self.rng.gen_range(800.0..1600.0);
                if self.stagger(Target::Player, 150.0) {
                    self.stare = Some((x, y, self.time + 300.0));
                    self.msg(
                        "An eye in the wall fixes its gaze on you. You cannot move!",
                        Rgb(255, 80, 80),
                    );
                    self.fx.flash(Rgb(140, 0, 0), 0.4);
                    self.emit(Sfx::Whisper, x, y, 0.9);
                }
            }
        }
    }

    /// Once a turn: count torments down, and maybe inflict a new one.
    fn tick_curses(&mut self) {
        let mut ended = Vec::new();
        for a in self.curses.iter_mut() {
            a.turns -= 1;
            if a.turns <= 0 {
                ended.push(a.kind);
            }
        }
        self.curses.retain(|a| a.turns > 0);
        for c in ended {
            self.end_curse(c);
        }
        if self.p.has_amulet && self.curses.len() < 2 {
            // Rare in the Sanctum, more frequent the closer you get to daylight.
            let climbed = 10u32.saturating_sub(self.depth) as f64;
            let p = 0.003 + 0.0008 * climbed;
            if self.rng.gen_bool(p) {
                let free: Vec<Curse> = Curse::ALL
                    .iter()
                    .copied()
                    .filter(|&c| !self.cursed(c))
                    .collect();
                if let Some(&c) = free.choose(&mut self.rng) {
                    let turns = self.rng.gen_range(8..=24);
                    self.start_curse(c, turns);
                }
            }
        }
        if self.cursed(Curse::Figments) {
            self.move_figments();
        }
    }

    pub fn start_curse(&mut self, c: Curse, turns: i32) {
        if self.cursed(c) {
            return;
        }
        self.curses.push(ActiveCurse { kind: c, turns });
        self.msg(c.onset(), c.color());
        self.fx.flash(Rgb(110, 0, 30), 0.35);
        self.fx.shake = self.fx.shake.max(0.5);
        let (px, py) = (self.p.x, self.p.y);
        self.emit(Sfx::Revive, px, py, 0.5);
        match c {
            Curse::Forgetting => self.forget_floor(),
            Curse::Figments => self.spawn_figments(),
            Curse::Darkness => self.refresh_view(),
            Curse::Gale => {
                self.p.torch_out = true;
                self.snuff_age = 0.0;
                self.puffed = false;
                // The flame is torn away in a burst of sparks and smoke.
                for _ in 0..25 {
                    self.smoke.push(Smoke {
                        x: rand::random::<f32>() - 0.5,
                        y: -rand::random::<f32>(),
                        vx: 10.0 + rand::random::<f32>() * 14.0,
                        vy: -1.0 - rand::random::<f32>() * 3.0,
                        life: 0.0,
                        max: 0.6 + rand::random::<f32>() * 0.8,
                    });
                }
                self.refresh_view();
            }
            _ => {}
        }
    }

    fn end_curse(&mut self, c: Curse) {
        self.msg(c.end(), Rgb(170, 210, 170));
        match c {
            Curse::Forgetting => self.restore_memory(),
            Curse::Figments => {
                let fs = std::mem::take(&mut self.figments);
                for f in fs {
                    if self.level.is_visible(f.x, f.y) {
                        self.fx.ring(
                            f.x as f32 + 0.5,
                            f.y as f32 + 0.5,
                            10,
                            1.2,
                            Rgb(150, 90, 200),
                            &['.', ':', '\''],
                            0.6,
                        );
                    }
                }
            }
            Curse::Darkness => self.refresh_view(),
            _ => {}
        }
    }

    /// Strike a light, if the wind will let you.
    fn relight(&mut self) -> Option<f64> {
        if !self.p.torch_out {
            self.msg("Your torch is already burning.", GREY);
            return None;
        }
        if self.cursed(Curse::Gale) {
            self.msg(
                "The wind tears the flame away the moment it catches!",
                Curse::Gale.color(),
            );
            return Some(100.0);
        }
        self.p.torch_out = false;
        self.light_age = 0.0;
        self.msg(
            "You strike your flint. Sparks catch, and your torch flares back to life.",
            Rgb(255, 200, 120),
        );
        let (lx, ly) = (self.p.x, self.p.y);
        self.emit(Sfx::Clang, lx, ly, 0.3);
        let (px, py) = (self.p.x, self.p.y);
        self.fx
            .light(px as f32 + 0.5, py as f32 + 0.5, [1.5, 1.0, 0.5], 4.0, 0.5);
        self.refresh_view();
        Some(100.0)
    }

    /// Smoke from the torch: it rises, curls, trails behind you as you walk,
    /// sweeps across the view as you turn, and is ripped sideways by a gale.
    fn update_smoke(&mut self, dt: f32, turn: f32, speed: f32) {
        let gale = self.cursed(Curse::Gale);
        self.light_age += dt;
        if self.p.torch_out {
            self.snuff_age += dt;
            // The guttering flame dies in a round puff of smoke.
            if self.snuff_age >= 0.3 && !self.puffed {
                self.puffed = true;
                for k in 0..36 {
                    let a = k as f32 / 36.0 * std::f32::consts::TAU;
                    let sp = 2.0 + rand::random::<f32>() * 3.5;
                    self.smoke.push(Smoke {
                        x: a.cos() * 0.5,
                        y: 0.5 + a.sin() * 0.4,
                        vx: a.cos() * sp * 1.6,
                        vy: a.sin() * sp - 1.5,
                        life: 0.0,
                        max: 0.9 + rand::random::<f32>() * 0.9,
                    });
                }
            }
        }
        // Spawning: a steady stream while lit, a dying wisp from the wick when snuffed.
        let rate = if self.p.invis > 0 {
            0.0
        } else if !self.p.torch_out {
            if self.light_age < 0.3 {
                0.0
            } else {
                18.0
            }
        } else if !self.puffed {
            0.0
        } else {
            (12.0 * (1.0 - self.snuff_age / 4.0)).max(0.0)
        };
        self.smoke_acc += rate * dt;
        while self.smoke_acc >= 1.0 {
            self.smoke_acc -= 1.0;
            let r = || rand::random::<f32>();
            self.smoke.push(Smoke {
                x: (r() - 0.5) * 1.2,
                y: if self.p.torch_out { 2.5 } else { -0.3 },
                vx: (r() - 0.5) * 0.8,
                vy: -2.6 - r() * 1.8,
                life: 0.0,
                max: 1.1 + r() * 1.3,
            });
        }
        let t = self.time as f32 * 0.01;
        let gust = if gale {
            22.0 + 10.0 * (t * 0.7).sin()
        } else {
            0.0
        };
        for s in self.smoke.iter_mut() {
            s.life += dt;
            // Curling turbulence.
            let curl = ((s.life * 4.0 + s.max * 17.0).sin() * 2.2)
                + ((s.y * 0.9 + s.max * 5.0).cos() * 0.8);
            s.vx += (curl + gust) * dt;
            s.vx *= 1.0 - 1.4 * dt;
            s.vy -= 0.6 * dt; // buoyancy
            s.vy *= 1.0 - 0.5 * dt;
            s.x += s.vx * dt - turn * 9.0 * dt;
            // Walking forward leaves the smoke streaming back over your shoulder.
            s.y += s.vy * dt + speed * 0.45 * dt;
        }
        self.smoke
            .retain(|s| s.life < s.max && s.y > -40.0 && s.x.abs() < 200.0);
        if self.smoke.len() > 300 {
            let n = self.smoke.len() - 300;
            self.smoke.drain(..n);
        }
    }

    /// Wipe what you remember of this floor, keeping a copy to give back.
    fn forget_floor(&mut self) {
        let n = self.level.explored.len();
        if self.forgotten.len() != n {
            self.forgotten = vec![false; n];
        }
        for i in 0..n {
            if self.level.explored[i] {
                self.forgotten[i] = true;
            }
            self.level.explored[i] = self.level.visible[i];
        }
    }

    fn restore_memory(&mut self) {
        if self.forgotten.len() == self.level.explored.len() {
            for (e, &f) in self.level.explored.iter_mut().zip(self.forgotten.iter()) {
                *e |= f;
            }
        }
        self.forgotten.clear();
    }

    pub fn figment_at(&self, x: i32, y: i32) -> Option<usize> {
        self.figments.iter().position(|f| f.x == x && f.y == y)
    }

    fn dispel_figment(&mut self, fi: usize) {
        let f = self.figments.remove(fi);
        self.msg("An evil figment of the mind!", Rgb(220, 120, 255));
        self.fx.ring(
            f.x as f32 + 0.5,
            f.y as f32 + 0.5,
            18,
            1.6,
            Rgb(170, 100, 230),
            &['.', '\'', ':', '*'],
            0.7,
        );
        self.emit(Sfx::Revive, f.x, f.y, 0.4);
    }

    /// Phantoms of the creatures that live around this depth, out of sight.
    fn spawn_figments(&mut self) {
        let depth = self.depth;
        let kinds: Vec<usize> = (0..MONSTERS.len())
            .filter(|&k| {
                let d = &MONSTERS[k];
                !d.boss && d.unique == 0 && d.depth.0 <= depth + 2 && d.depth.1 + 1 >= depth
            })
            .collect();
        let n = self.rng.gen_range(2..=4);
        for _ in 0..n {
            let mut pos = None;
            for _ in 0..60 {
                let c = self.level.random_floor(&mut self.rng);
                let i = self.level.idx(c.0, c.1);
                let d = self.dmap.get(i).copied().unwrap_or(UNREACH);
                if (5..=14).contains(&d)
                    && self.level.monster_at(c.0, c.1).is_none()
                    && self.figment_at(c.0, c.1).is_none()
                {
                    pos = Some(c);
                    break;
                }
            }
            if let (Some((x, y)), Some(&kind)) = (pos, kinds.choose(&mut self.rng)) {
                let id = self.new_id();
                self.figments.push(Figment {
                    id,
                    kind,
                    x,
                    y,
                    rx: x as f32,
                    ry: y as f32,
                });
            }
        }
    }

    /// Phantoms creep toward you and swing at the air, to keep up the lie.
    fn move_figments(&mut self) {
        let (px, py) = (self.p.x, self.p.y);
        let mut i = 0;
        while i < self.figments.len() {
            let (fx, fy) = (self.figments[i].x, self.figments[i].y);
            if self.level.monster_at(fx, fy).is_some() {
                self.figments.remove(i);
                continue;
            }
            if cheb(fx, fy, px, py) <= 1 {
                if self.rng.gen_bool(0.5) {
                    let name = MONSTERS[self.figments[i].kind].name;
                    self.msg(format!("The {} misses you.", name), GREY);
                    self.emit(Sfx::Swing, fx, fy, 0.4);
                }
                i += 1;
                continue;
            }
            let mut best = (
                self.dmap
                    .get(self.level.idx(fx, fy))
                    .copied()
                    .unwrap_or(UNREACH),
                fx,
                fy,
            );
            for &(dx, dy) in DIRS.iter() {
                let (nx, ny) = (fx + dx, fy + dy);
                if !self.level.can_step(fx, fy, dx, dy)
                    || !self.level.tile(nx, ny).walkable()
                    || (nx == px && ny == py)
                    || self.level.monster_at(nx, ny).is_some()
                    || self.figment_at(nx, ny).is_some()
                {
                    continue;
                }
                let d = self.dmap[self.level.idx(nx, ny)];
                if d < best.0 {
                    best = (d, nx, ny);
                }
            }
            self.figments[i].x = best.1;
            self.figments[i].y = best.2;
            i += 1;
        }
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
            let d = if from_player {
                self.dmap[self.level.idx(mx, my)]
            } else {
                cheb(x, y, mx, my)
            };
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
    fn trace(
        &self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        max: i32,
        shooter: Option<u32>,
    ) -> (i32, i32, Option<Target>) {
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
                let c = self.action_cost(c);
                self.p.next_act = self.time + c;
            }
        } else {
            if self.p.next_act < self.time {
                self.p.next_act = self.time;
            }
            if let Some(c) = self.player_action(a) {
                let c = self.action_cost(c);
                let t = self.time + c;
                self.p.next_act = t;
                self.advance_to(t);
            }
        }
    }

    /// Final time cost of an action: frost slows it, Bloodlust erases it.
    fn action_cost(&mut self, c: f64) -> f64 {
        let slow = std::mem::take(&mut self.p.slow);
        if std::mem::take(&mut self.free_turn) {
            self.msg(
                "Bloodlust! You strike again before the body hits the floor.",
                Rgb(255, 70, 70),
            );
            return 0.0;
        }
        if self.cursed(Curse::Leaden) {
            return c * 1.4 + slow;
        }
        c + slow
    }

    /// How a door on this floor opens: gates rise, portals and crystal
    /// dissipate, stone and bone doors part, plank doors swing.
    pub fn door_opening(&self, t: Tile) -> DoorOpening {
        if t == Tile::GateClosed {
            return DoorOpening::Rise;
        }
        match self.level.style {
            5 | 13 => DoorOpening::Dissolve,
            2 | 3 | 9 | 14 => DoorOpening::Split,
            _ => DoorOpening::Swing,
        }
    }

    /// Open the door at (x,y) with its animation and sound. Returns what you hear.
    fn open_door(&mut self, x: i32, y: i32) -> &'static str {
        let kind = self.door_opening(self.level.tile(x, y));
        self.level.set(x, y, Tile::DoorOpen);
        self.fx.doors.retain(|d| (d.x, d.y) != (x, y));
        self.fx.doors.push(DoorAnim {
            x,
            y,
            kind,
            age: 0.0,
        });
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let txt = match kind {
            DoorOpening::Rise => {
                self.emit(Sfx::Chains, x, y, 0.9);
                self.fx.burst(
                    fx,
                    fy,
                    0.9,
                    10,
                    Rgb(140, 130, 110),
                    &['.', ','],
                    0.6,
                    0.8,
                    1.0,
                    false,
                );
                "The gate rises into the lintel with a rattle of chains."
            }
            DoorOpening::Dissolve => {
                self.emit(Sfx::Blink, x, y, 0.8);
                let col = if self.level.style == 13 {
                    Rgb(180, 120, 255)
                } else {
                    Rgb(130, 240, 230)
                };
                self.fx.rise(fx, fy, 30, col, &['*', '.', '\'', '+'], 0.6);
                if self.level.style == 13 {
                    "The swirling void thins, and dissipates into the air."
                } else {
                    "The crystal door dissolves into drifting motes of light."
                }
            }
            DoorOpening::Split => {
                self.emit(Sfx::Rumble, x, y, 0.7);
                self.fx.burst(
                    fx,
                    fy,
                    0.1,
                    14,
                    Rgb(120, 110, 100),
                    &['.', ',', 'o'],
                    0.7,
                    1.0,
                    -0.2,
                    false,
                );
                "The heavy doors grind apart."
            }
            DoorOpening::Swing => {
                self.emit(Sfx::Creak, x, y, 0.8);
                "The door creaks open."
            }
        };
        self.refresh_view();
        txt
    }

    /// Switch between first-person and overhead play, with a camera swoop.
    pub fn toggle_view(&mut self) {
        self.overhead = !self.overhead;
        self.view_anim = Some((0.0, self.overhead));
    }

    /// Shift+C: switch Last Rites on or off, once it has been bought.
    pub fn toggle_rites(&mut self) {
        if self.bonus.pyre <= 0 {
            self.msg("You have not learned the Last Rites (Soul Altar).", Rgb(150, 150, 150));
            return;
        }
        self.rites_off = !self.rites_off;
        if self.rites_off {
            self.msg("Last Rites OFF: the dead are left as they fall. (Shift+C)", Rgb(150, 150, 170));
        } else {
            self.msg(
                format!(
                    "Last Rites ON: {}% chance to burn or shatter the bodies of your kills. (Shift+C)",
                    5 * self.bonus.pyre
                ),
                Rgb(255, 170, 90),
            );
        }
    }

    /// Last Rites: the corpse just left at (x,y) by a monster of `kind` is
    /// burned with the torch, or a skeleton kicked to shards, so it never
    /// rises again and cannot be raised. Returns true if it was destroyed.
    fn last_rites(&mut self, kind: usize, x: i32, y: i32) -> bool {
        if self.bonus.pyre <= 0 || self.rites_off {
            return false;
        }
        let def = &MONSTERS[kind];
        let undead = def.death == DeathKind::Undead;
        // No flame to set a body alight with; bones can still be kicked apart.
        if !undead && self.p.torch_out {
            return false;
        }
        if self.rng.gen::<f32>() >= 0.05 * self.bonus.pyre.min(5) as f32 {
            return false;
        }
        let Some(ci) = self
            .level
            .corpses
            .iter()
            .rposition(|c| c.kind == kind && c.x == x && c.y == y)
        else {
            return false;
        };
        self.level.corpses.remove(ci);
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let seen = self.level.is_visible(x, y);
        if undead {
            self.fx.bones(fx, fy);
            self.fx.burst(fx, fy, 0.2, 18, Rgb(225, 220, 200), &[',', '.', '\'', '`'], 1.4, 0.8, 0.8, false);
            self.emit(Sfx::Bones, x, y, 0.9);
            if seen {
                self.msg(
                    format!("You kick the {}'s bones to shards. It will not rise again.", def.name),
                    Rgb(225, 215, 190),
                );
            }
        } else {
            self.fx.burst(fx, fy, 0.3, 26, Rgb(255, 140, 40), &['^', '*', '&', '\''], 0.5, 1.6, -0.6, true);
            self.fx.burst(fx, fy, 0.6, 12, Rgb(70, 65, 65), &['~', 'o', '.'], 0.3, 2.2, -0.4, false);
            self.fx.light(fx, fy, [1.4, 0.7, 0.25], 2.5, 1.2);
            self.emit(Sfx::Fireball, x, y, 0.6);
            if seen {
                self.msg(
                    format!("You put your torch to the {}'s body. It burns to ash.", def.name),
                    Rgb(255, 170, 90),
                );
            }
        }
        true
    }

    pub fn toggle_realtime(&mut self) {
        self.realtime = !self.realtime;
        self.queued = None;
        if self.realtime {
            self.msg("REAL-TIME combat engaged. The dungeon no longer waits for you! (Shift+R to switch back)", Rgb(255, 120, 80));
        } else {
            self.msg(
                "Turn-based mode. Time moves only when you act.",
                Rgb(120, 200, 255),
            );
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
            self.p.poison -= if self.bonus.ironwill >= 1 { 2 } else { 1 };
            self.p.poison = self.p.poison.max(0);
            if self.turns % 2 == 0 {
                self.p.hp -= 1;
                if self.p.hp <= 0 {
                    if self.bonus.anchor && !self.p.anchor_used {
                        self.soul_anchor();
                    } else {
                        self.p.hp = 0;
                        let src = self.p.poison_src.clone();
                        self.die(DeathCause::Poison(src));
                        return;
                    }
                }
            }
            if self.p.poison == 0 {
                self.msg("The poison wears off.", Rgb(150, 255, 150));
            }
        }
        if self.has(UPower::Regrowth) && self.p.hp < self.p.max_hp && self.p.hp > 0 {
            self.p.hp += 1;
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
        let base_period: i32 = if self.p.class == C_MAGE || self.p.class == C_NECRO {
            2
        } else {
            4
        };
        let staff_bonus: i32 = if WEAPONS[self.p.wpn.def].art == WArt::Staff {
            1
        } else {
            0
        };
        let arcane: i32 = if self.has(UPower::Arcane) { 1 } else { 0 };
        let mperiod = base_period - staff_bonus - arcane;
        if self.p.mp_ctr >= mperiod.max(1) {
            self.p.mp_ctr = 0;
            if self.p.mp < self.p.max_mp {
                self.p.mp += 1;
            }
        }
        if self.p.fury > 0 {
            self.p.fury -= 1;
            if self.p.fury == 0 {
                self.msg("Your battle fury subsides.", Rgb(200, 150, 120));
            }
        }
        if self.p.vision > 0 {
            self.p.vision -= 1;
            if self.p.vision == 0 {
                self.msg("Your monster vision fades.", Rgb(200, 160, 120));
            }
        }
        if self.p.invis > 0 {
            self.p.invis -= 1;
            if self.p.invis == 0 {
                self.msg("You fade back into view.", Rgb(150, 150, 200));
            }
        }
        self.tick_curses();
        if self.p.rooted > 0 {
            self.p.rooted -= 1;
            if self.p.rooted == 0 {
                self.msg("You tear yourself free!", Rgb(200, 220, 160));
            }
        }
        if self.p.blind > 0 {
            self.p.blind -= 1;
            if self.p.blind == 0 {
                self.msg("You blink your eyes clear.", Rgb(200, 220, 160));
                self.refresh_view();
            }
        }
        self.tick_clouds();
        self.living_dungeon_tick();
        if let Some(w) = self.level.ward {
            if w.state != WardState::Broken
                && cheb(w.x, w.y, px, py) <= 12
                && self.rng.gen_bool(0.25)
            {
                // The souls inside it shriek and weep.
                let mood = if self.rng.gen_bool(0.5) {
                    Mood::Idle
                } else {
                    Mood::Hurt
                };
                self.emit(Sfx::Voice(VoiceKind::Wail, mood), w.x, w.y, 0.45);
            }
        }

        let turns = self.turns;
        // Thralls knit a little slower than their master does.
        let thrall_period = (period + 2) as u64;
        for m in self.level.monsters.iter_mut() {
            if m.alive && m.def().regen && m.hp < m.max_hp && turns % 2 == 0 {
                m.hp = (m.hp + 2).min(m.max_hp);
            }
            if m.alive && m.thrall && m.hp < m.max_hp && turns % thrall_period == 0 {
                m.hp += 1;
            }
            if m.guard > 0 {
                m.guard -= 1;
            }
        }
        self.expire_minions();
        self.sync_upkeep();

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
            m.awake = self.amulet_rage() || self.rng.gen_bool(0.5);
            m.scale_hp(self.diff().hp);
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
        for p in self.pings.iter_mut() {
            p.age += dt;
        }
        self.pings.retain(|p| p.age < Ping::LIFE);
        if let Some((e, to)) = self.view_anim {
            let e = e + dt;
            self.view_anim = if e >= VIEW_SWOOP { None } else { Some((e, to)) };
        }

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
        let dang = d * (dt * 13.0).min(1.0);
        self.cam.ang += dang;
        let turn_rate = if dt > 1e-4 { dang / dt } else { 0.0 };
        let walk_rate = if dt > 1e-4 { moved / dt } else { 0.0 };
        self.update_smoke(dt, turn_rate, walk_rate);
        self.cam.swing = (self.cam.swing - dt * 4.0).max(0.0);

        for m in self.level.monsters.iter_mut() {
            m.rx += (m.x as f32 - m.rx) * k;
            m.ry += (m.y as f32 - m.ry) * k;
            m.flash = (m.flash - dt * 4.0).max(0.0);
            m.lunge.2 = (m.lunge.2 - dt * 5.0).max(0.0);
        }
        // Tormented souls stream up out of a ward prism toward the ceiling.
        if let Some(w) = self.level.ward {
            if w.state != WardState::Broken && cheb(w.x, w.y, self.p.x, self.p.y) <= 16 {
                self.soul_acc += dt * 6.0;
                while self.soul_acc >= 1.0 {
                    self.soul_acc -= 1.0;
                    let col = Rgb(170, 220, 255).lerp(Rgb(230, 180, 255), rand::random::<f32>());
                    self.fx.rise(
                        w.x as f32 + 0.5,
                        w.y as f32 + 0.5,
                        1,
                        col,
                        &['o', '~', '@', '0'],
                        0.35,
                    );
                }
            }
        }
        for f in self.figments.iter_mut() {
            f.rx += (f.x as f32 - f.rx) * k;
            f.ry += (f.y as f32 - f.ry) * k;
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
            Action::Pickup => match self.items_here().first() {
                Some(&ii) => self.pickup_index(ii),
                None => {
                    self.msg("There is nothing here to pick up.", GREY);
                    None
                }
            },
            Action::PickupOne(k) => match self.items_here().get(k) {
                Some(&ii) => self.pickup_index(ii),
                None => None,
            },
            Action::PickupAll => self.pickup_all(),
            Action::Stairs => self.use_stairs(),
            Action::Skill(i) => {
                let sk = self.skills();
                if i < sk.len() {
                    self.use_skill(sk[i])
                } else {
                    self.msg(
                        "You have no skill in that slot. Buy more at the Soul Altar.",
                        GREY,
                    );
                    None
                }
            }
            Action::Use(i) => self.use_item(i),
            Action::Drop(i) => self.drop_item(i),
            Action::Relight => self.relight(),
            Action::Listen => self.listen(),
            Action::RaiseAt(x, y) => {
                self.raise_menu.clear();
                self.raise_pick = Some((x, y));
                let r = self.use_skill(Skill::RaiseDead);
                self.raise_pick = None;
                r
            }
            Action::QuickHeal => {
                if let Some(i) = self
                    .p
                    .inv
                    .iter()
                    .position(|it| *it == Item::Potion(Potion::Healing))
                {
                    self.use_item(i)
                } else {
                    self.msg("You have no Potions of Healing!", Rgb(255, 90, 90));
                    None
                }
            }
        }
    }

    fn player_move(&mut self, dir: i32) -> Option<f64> {
        let mut dir = dir;
        if self.cursed(Curse::Vertigo) && self.rng.gen_bool(0.3) {
            let nd = self.rng.gen_range(0..8);
            if nd != dir {
                dir = nd;
                self.msg(
                    "The floor lurches! You stagger the wrong way.",
                    Curse::Vertigo.color(),
                );
                self.fx.shake = self.fx.shake.max(0.3);
            }
        }
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
        if self.p.rooted > 0 {
            self.msg(
                "You struggle against what holds you, but cannot move!",
                Rgb(220, 200, 160),
            );
            return Some(100.0);
        }
        if let Some(fi) = self.figment_at(nx, ny) {
            // You walk straight through it.
            self.dispel_figment(fi);
        }
        match self.level.tile(nx, ny) {
            t if t.is_closed_door() => {
                let txt = self.open_door(nx, ny);
                self.msg(txt, GREY);
                self.noise(px, py, 4);
                Some(100.0)
            }
            Tile::Sinew => self.cut_sinew(nx, ny),
            Tile::Ward => self.strike_ward(),
            t if t.walkable() => {
                self.p.x = nx;
                self.p.y = ny;
                self.emit(Sfx::Step, nx, ny, 0.5);
                self.after_player_move();
                let base = if self.has(UPower::Swift) { 70.0 } else { 100.0 };
                Some(base * (1.0 - 0.1 * self.bonus.fleet.clamp(0, 2) as f64))
            }
            _ => {
                self.fx.shake = self.fx.shake.max(0.15);
                None
            }
        }
    }

    /// Poison the player, unless Iron Will makes them immune.
    fn add_poison(&mut self, turns: i32, src: String) -> bool {
        if self.bonus.ironwill >= 2 {
            self.msg("Your iron will shrugs off the poison.", Rgb(200, 220, 200));
            return false;
        }
        self.p.poison += turns;
        self.p.poison_src = src;
        true
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
        if self.level.cloud_at(px, py) {
            self.spore_player();
        }
        let chance = if self.p.class == C_ROGUE { 0.35 } else { 0.12 };
        self.search_traps(chance);
        self.gulp_potions();
        let here: Vec<String> = self
            .level
            .items
            .iter()
            .filter(|i| i.x == px && i.y == py)
            .map(|i| i.item.name())
            .collect();
        if !here.is_empty() {
            self.msg(
                format!("You see here: {}. (g to pick up)", here.join(", ")),
                Rgb(220, 210, 120),
            );
        }
        match self.level.tile(px, py) {
            Tile::StairsDown => self.msg(
                "A stairway leads down into darkness. (> or Enter)",
                Rgb(200, 150, 255),
            ),
            Tile::StairsUp => self.msg("A stairway leads up. (< or Enter)", Rgb(150, 200, 255)),
            _ => {}
        }
        let r = (3 - self.stealth()).max(1);
        self.noise(px, py, r);
    }

    fn check_trap(&mut self, x: i32, y: i32) {
        let Some(ti) = self.level.traps.iter().position(|t| t.x == x && t.y == y) else {
            return;
        };
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
                self.msg(
                    format!("Spikes burst from the floor! ({} damage)", dmg),
                    Rgb(255, 90, 90),
                );
                self.fx.blood(fx, fy, 0.2, 14);
                self.player_hit(dmg, DeathCause::Trap(TrapKind::Spikes), true);
            }
            TrapKind::Gas => {
                self.msg(
                    "A cloud of noxious gas erupts around you!",
                    Rgb(140, 255, 90),
                );
                self.fx.mist(fx, fy, Rgb(120, 220, 60));
                self.emit(Sfx::Voice(VoiceKind::Hiss, Mood::Alert), x, y, 0.9);
                self.add_poison(6 + d, "a poison gas trap".to_string());
                self.fx.flash(Rgb(80, 200, 40), 0.4);
            }
            TrapKind::Fire => {
                let dmg = self.rng.gen_range(3..=12) + d;
                self.msg(
                    format!("Flames roar up around you! ({} damage)", dmg),
                    Rgb(255, 140, 40),
                );
                self.fx.explosion(fx, fy, Rgb(255, 120, 20));
                self.emit(Sfx::Explosion, x, y, 1.0);
                self.player_hit(dmg, DeathCause::Trap(TrapKind::Fire), false);
            }
            TrapKind::Maw => {
                let dmg = self.rng.gen_range(3..=8) + d;
                self.msg(format!("The floor splits into a fanged MAW and bites down on your leg! ({} damage)", dmg), Rgb(255, 70, 80));
                self.fx.blood(fx, fy, 0.2, 20);
                self.fx.shake = self.fx.shake.max(0.6);
                self.emit(Sfx::Voice(VoiceKind::Growl, Mood::Attack), x, y, 1.0);
                self.p.rooted = self.p.rooted.max(2);
                self.player_hit(dmg, DeathCause::Trap(TrapKind::Maw), false);
                if self.over.is_none() {
                    self.msg("Its teeth hold you fast!", Rgb(230, 150, 150));
                }
            }
            TrapKind::Alarm => {
                self.msg(
                    "A shrill alarm shrieks through the halls!",
                    Rgb(255, 220, 80),
                );
                self.emit(Sfx::Clang, x, y, 1.0);
                self.emit(Sfx::Clang, x, y, 1.0);
                for m in self.level.monsters.iter_mut() {
                    if m.alive
                        && m.faction != Faction::Ally
                        && self.dmap[(m.y * MAP_W + m.x) as usize] <= 30
                    {
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
            if let Some(fi) = self.figment_at(nx, ny) {
                self.cam.swing = 1.0;
                self.emit(Sfx::Swing, px, py, 0.6);
                self.dispel_figment(fi);
                return Some(w.speed);
            }
        }
        if self.level.tile(nx, ny) == Tile::Sinew && self.level.can_step(px, py, dx, dy) {
            return self.cut_sinew(nx, ny);
        }
        if self.level.tile(nx, ny) == Tile::Ward && self.level.can_step(px, py, dx, dy) {
            return self.strike_ward();
        }
        if self.level.tile(nx, ny).is_closed_door() {
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
        let end = if mi.is_some() {
            BoltEnd::Blood
        } else {
            BoltEnd::Sparks(Rgb(200, 200, 200))
        };
        self.fx.bolt(
            px as f32 + 0.5,
            py as f32 + 0.5,
            hx as f32 + 0.5,
            hy as f32 + 0.5,
            22.0,
            ch,
            Rgb(225, 205, 160),
            None,
            end,
        );
        self.emit(Sfx::Arrow, px, py, 0.7);
        let flight = (((hx - px).pow(2) + (hy - py).pow(2)) as f32).sqrt() / 22.0;
        if let Some(mi) = mi {
            let custom = if w.range > 0 { None } else { Some((2, 6)) };
            self.strike_delay = flight;
            self.player_strike(mi, 1.0, 0, custom);
            self.strike_delay = 0.0;
        } else {
            // The trace stops on the last open tile; a wall beyond it means
            // the arrow struck stone rather than flying out of range.
            let (dx, dy) = DIRS[dir as usize];
            if self.level.tile(hx + dx, hy + dy).opaque() {
                self.emit_after(Sfx::Impact(Material::Wall), hx, hy, 0.6, flight);
            }
        }
        self.noise(px, py, 4);
        self.combat_heat = 1.0;
    }

    fn player_strike(&mut self, mi: usize, mult: f32, hit_mod: i32, custom: Option<(i32, i32)>) {
        let (kind, mx, my, unaware, name, elite, low) = {
            let m = &self.level.monsters[mi];
            (
                m.kind,
                m.x,
                m.y,
                !m.awake || m.target != Some(Target::Player),
                m.label(),
                m.champion || m.is_unique() || m.def().boss,
                m.hp * 10 < m.max_hp * 3,
            )
        };
        let def = &MONSTERS[kind];
        let roll = self.rng.gen_range(1..=20);
        let hitb = self.hit_bonus() + hit_mod + if unaware { 4 } else { 0 };
        let tn = 10 + self.level.monsters[mi].ac() * 3 / 2;
        let (fx, fy) = (mx as f32 + 0.5, my as f32 + 0.5);
        if roll == 20 || (roll != 1 && roll + hitb >= tn) {
            let (lo, hi) = custom.unwrap_or(self.wdef().dmg);
            let mut dmg = self.rng.gen_range(lo..=hi) + self.dmg_bonus();
            dmg = ((dmg as f32) * mult * self.dmg_mult_vs(elite, low)).round() as i32;
            // Mastered in the bestiary: you know exactly where to strike.
            let mastered = self.bonus.mastered.iter().any(|n| n == def.name);
            let crit_bonus = if mastered { 0.015 } else { 0.0 };
            let crit = roll == 20 || self.rng.gen::<f32>() < self.crit_chance() + crit_bonus;
            let backstab = unaware && self.p.class == C_ROGUE;
            let mut col = Rgb(235, 235, 235);
            if backstab {
                dmg *= 3;
                col = Rgb(255, 80, 80);
            } else if crit {
                let k = if self.has(UPower::Executioner) {
                    3.0
                } else {
                    2.0
                };
                let k = if mastered { k * 1.02 } else { k };
                dmg = ((dmg as f32) * k).round() as i32;
                col = Rgb(255, 210, 60);
            }
            if self.has(UPower::Flame) {
                dmg += self.rng.gen_range(1..=6) + self.depth as i32 / 2;
                self.fx.ichor(fx, fy, 0.5, 8, Rgb(255, 140, 30));
                self.fx.light(fx, fy, [1.8, 0.8, 0.2], 1.5, 0.3);
            }
            dmg = dmg.max(1);
            if backstab {
                self.msg(
                    format!("You backstab the {} for {}!", name, dmg),
                    Rgb(255, 120, 120),
                );
            } else if crit {
                self.msg(
                    format!("Critical hit! The {} takes {}!", name, dmg),
                    Rgb(255, 210, 80),
                );
            } else {
                self.msg(
                    format!("You hit the {} for {}.", name, dmg),
                    Rgb(215, 215, 215),
                );
            }
            let w = self.wdef();
            if custom.is_none()
                && w.stun > 0.0
                && self.rng.gen::<f32>() < w.stun
                && self.level.monsters[mi].hp > dmg
            {
                self.level.monsters[mi].stun = 2;
                self.msg(format!("The {} is stunned!", name), Rgb(200, 200, 255));
            }
            if self.has(UPower::Frost)
                && self.rng.gen_bool(0.25)
                && self.level.monsters[mi].hp > dmg
            {
                self.level.monsters[mi].stun = 2;
                self.fx.ice(fx, fy);
                self.msg(format!("The {} is frozen solid!", name), Rgb(150, 220, 255));
            }
            if self.has(UPower::Vampiric) {
                let heal = (dmg / 5).max(1);
                self.p.hp = (self.p.hp + heal).min(self.p.max_hp);
            }
            let delay = self.strike_delay;
            self.emit_after(Sfx::Impact(def.material()), mx, my, 0.9, delay);
            self.fx.text(fx, fy, 0.9, dmg.to_string(), col);
            if crit || backstab {
                self.fx.shake = self.fx.shake.max(0.4);
            }
            let mid = self.level.monsters[mi].id;
            self.damage_monster(mi, dmg, Src::Player);
            if self.has(UPower::Storm) {
                self.storm_arc(mid, mx, my, dmg / 2 + 1);
            }
        } else {
            self.msg(format!("You miss the {}.", name), GREY);
            let delay = self.strike_delay;
            self.emit_after(Sfx::Miss, mx, my, 0.5, delay);
            self.fx.text(fx, fy, 0.9, "miss".to_string(), GREY);
        }
        let m = &mut self.level.monsters[mi];
        if m.alive && m.faction != Faction::Ally {
            m.awake = true;
            m.target = Some(Target::Player);
        }
    }

    /// Damage multiplier from boons and buffs against a foe.
    /// `elite`: champion, overlord, unique or boss. `low`: below 30% health.
    pub fn dmg_mult_vs(&self, elite: bool, low: bool) -> f32 {
        let mut k = 1.0;
        if self.p.fury > 0 {
            k *= 1.5;
        }
        if elite {
            k *= 1.0 + 0.15 * self.bonus.hunter as f32;
        }
        if low {
            k *= 1.0 + 0.15 * self.bonus.coup as f32;
        }
        if self.last_stand() {
            k *= 1.0 + 0.15 * self.bonus.laststand as f32;
        }
        k
    }

    /// Stormcaller: lightning leaps from (x,y) to the nearest other foe.
    fn storm_arc(&mut self, from_id: u32, x: i32, y: i32, dmg: i32) {
        let mut best: Option<(usize, i32)> = None;
        for (i, o) in self.level.monsters.iter().enumerate() {
            if !o.alive || o.id == from_id || o.faction == Faction::Ally {
                continue;
            }
            let d = cheb(x, y, o.x, o.y);
            if d <= 3 && best.map_or(true, |b| d < b.1) && self.level.los(x, y, o.x, o.y) {
                best = Some((i, d));
            }
        }
        if let Some((oi, _)) = best {
            let (ox, oy) = (self.level.monsters[oi].x, self.level.monsters[oi].y);
            self.fx.bolt(
                x as f32 + 0.5,
                y as f32 + 0.5,
                ox as f32 + 0.5,
                oy as f32 + 0.5,
                30.0,
                '~',
                Rgb(170, 200, 255),
                Some([0.8, 1.0, 2.2]),
                BoltEnd::Sparks(Rgb(200, 220, 255)),
            );
            self.emit(Sfx::Bolt, ox, oy, 0.6);
            self.fx.text(
                ox as f32 + 0.5,
                oy as f32 + 0.5,
                0.9,
                dmg.to_string(),
                Rgb(170, 200, 255),
            );
            self.damage_monster(oi, dmg, Src::Player);
        }
    }

    /// Returns true if the monster died.
    pub fn damage_monster(&mut self, mi: usize, dmg: i32, src: Src) -> bool {
        // Only you and your thralls can hurt the Warden.
        if self.level.monsters[mi].def().boss {
            let allowed = match src {
                Src::Player => true,
                Src::Mon(id) => self.mon_index(id).map_or(false, |k| self.may_harm_boss(k)),
                Src::Env => false,
            };
            if !allowed {
                return false;
            }
        }
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
            let m = &self.level.monsters[mi];
            if def.abil.contains(&MAbil::Berserk) && !m.raged && m.hp * 2 < m.max_hp {
                self.level.monsters[mi].raged = true;
                self.emit(Sfx::Voice(def.voice, Mood::Alert), x, y, 1.0);
                self.fx.ichor(fx, fy, 0.6, 16, Rgb(255, 40, 20));
                if self.level.is_visible(x, y) || cheb(x, y, self.p.x, self.p.y) <= 10 {
                    let who = cap(&self.level.monsters[mi].the());
                    self.msg(
                        format!("{} flies into a BERSERK RAGE!", who),
                        Rgb(255, 70, 50),
                    );
                }
            }
            if self.rng.gen_bool(0.4) {
                self.emit(Sfx::Voice(def.voice, Mood::Hurt), x, y, 0.7);
            }
            false
        }
    }

    fn kill_monster(&mut self, mi: usize, src: Src) {
        let (kind, x, y, faction, summoned, champion, overlord, label, thrall, raised) = {
            let m = &mut self.level.monsters[mi];
            m.alive = false;
            (
                m.kind,
                m.x,
                m.y,
                m.faction,
                m.summoned,
                m.champion,
                m.overlord,
                m.label(),
                m.thrall,
                m.raises,
            )
        };
        if kind == M_GUARDIAN {
            self.break_ward();
        }
        if thrall {
            let before = self.p.thrall_upkeep;
            self.sync_upkeep();
            let back = before - self.p.thrall_upkeep;
            if back > 0 {
                self.msg(
                    format!(
                        "The life you bound to it returns to you (+{} max HP).",
                        back
                    ),
                    Rgb(170, 230, 170),
                );
            }
        }
        let def = &MONSTERS[kind];
        let unique = def.unique > 0;
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let ally = faction == Faction::Ally;
        if self.level.is_visible(x, y) {
            let label = if ally {
                format!("Your {}", label)
            } else if def.boss || unique {
                cap(def.name)
            } else {
                format!("The {}", label)
            };
            let verb = if def.death == DeathKind::Undead {
                "is destroyed"
            } else {
                "dies"
            };
            let col = if ally {
                Rgb(255, 150, 100)
            } else {
                Rgb(255, 220, 140)
            };
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
                self.fx.burst(
                    fx,
                    fy,
                    0.4,
                    20,
                    Rgb(80, 70, 70),
                    &['o', '~', '.'],
                    0.6,
                    1.5,
                    -0.3,
                    false,
                );
                self.fx.light(fx, fy, [1.5, 0.6, 0.2], 2.0, 0.5);
            }
            _ => self.fx.blood(fx, fy, 0.5, 22),
        }
        if def.death != DeathKind::Undead && self.level.inb(x, y) {
            let i = self.level.idx(x, y);
            self.level.blood[i] = (self.level.blood[i] + 1.0).min(3.0);
            for &(dx, dy) in DIRS.iter() {
                if self.rng.gen_bool(0.3)
                    && self.level.inb(x + dx, y + dy)
                    && !self.level.tile(x + dx, y + dy).opaque()
                {
                    let j = self.level.idx(x + dx, y + dy);
                    self.level.blood[j] = (self.level.blood[j] + 0.5).min(3.0);
                }
            }
        }
        // Bosses and uniques leave no corpse: they never rise again and cannot be raised.
        if !def.boss && !unique {
            let revive_at = if def.revive.1 > 0.0 {
                let delay = self.rng.gen_range(def.revive.0..def.revive.1) * 100.0;
                self.time + delay * self.respawn_factor()
            } else {
                -1.0
            };
            // A fallen thrall's body is spent: it will not rise on its own.
            let (revive_at, raises) = if thrall {
                (-1.0, raised.max(1))
            } else {
                (revive_at, 0)
            };
            self.level.corpses.push(Corpse {
                kind,
                x,
                y,
                revive_at,
                raises,
            });
        }
        let killer_idx = match src {
            Src::Mon(id) => self
                .level
                .monsters
                .iter()
                .position(|m| m.id == id && m.alive),
            _ => None,
        };
        let by_ally = killer_idx.map_or(false, |k| self.level.monsters[k].faction == Faction::Ally);
        let by_player = src == Src::Player;
        if !ally && (by_player || by_ally) {
            let mut xp = if by_ally { (def.xp + 1) / 2 } else { def.xp };
            if overlord {
                xp *= 5;
            } else if champion {
                xp *= 3;
            }
            self.gain_xp(xp);
            *self.slain.entry(def.name.to_string()).or_insert(0) += 1;
            if by_player {
                self.kills += 1;
                if self.bonus.leech > 0 {
                    self.p.mp = (self.p.mp + self.bonus.leech).min(self.p.max_mp);
                }
                if self.bonus.bloodlust > 0
                    && self.rng.gen::<f32>() < 0.10 * self.bonus.bloodlust as f32
                {
                    self.free_turn = true;
                }
                if self.bonus.siphon > 0 {
                    self.p.hp = (self.p.hp + self.bonus.siphon).min(self.p.max_hp);
                }
                if self.has(UPower::Soulfeast) {
                    self.p.hp = (self.p.hp + 3).min(self.p.max_hp);
                    self.p.mp = (self.p.mp + 2).min(self.p.max_mp);
                }
                if !def.boss && !unique {
                    self.last_rites(kind, x, y);
                }
            }
        }

        // Infighting: the victor may grow into a champion.
        // A champion that slays another champion may become an overlord.
        if let Some(k) = killer_idx {
            let km = &self.level.monsters[k];
            if km.thrall {
                // Thralls grow by slaughter, faster as their master grows in power.
                if !ally {
                    let tier = (self.p.level / 5) as f64;
                    if !km.champion {
                        if self.rng.gen_bool((0.05 + 0.02 * tier).min(1.0)) {
                            self.make_champion(k);
                        }
                    } else if !km.overlord
                        && champion
                        && self.rng.gen_bool((0.015 + 0.005 * tier).min(1.0))
                    {
                        self.make_overlord(k);
                    }
                }
            } else if !km.champion
                && !km.summoned
                && !km.def().boss
                && !km.is_unique()
                && self.rng.gen_bool(0.35)
            {
                self.make_champion(k);
            } else if km.champion && !km.overlord && champion && self.rng.gen_bool(0.5) {
                self.make_overlord(k);
            }
        }

        let drop_at = self.drop_spot(x, y);
        let luck = self.luck();
        if !ally && !summoned && !unique {
            let chance =
                0.10 + luck as f32 * 0.04 + if self.has(UPower::Fortune) { 0.15 } else { 0.0 };
            if self.rng.gen::<f32>() < chance {
                let it = random_item(self.depth, &mut self.rng, luck);
                self.level.items.push(FloorItem {
                    x: drop_at.0,
                    y: drop_at.1,
                    item: it,
                });
            }
        }
        if champion && !ally && by_player {
            self.champion_slain(x, y, &label, drop_at, overlord);
        }
        if unique && !ally {
            let sig = def.sig;
            self.found_uniques.push(sig);
            let item = unique_item(sig);
            self.level.items.push(FloorItem {
                x: drop_at.0,
                y: drop_at.1,
                item,
            });
            self.msg(
                format!(
                    "{} is no more! {} clatters to the floor.",
                    cap(def.name),
                    item.name()
                ),
                Rgb(220, 150, 255),
            );
            self.fx
                .ring(fx, fy, 50, 3.0, Rgb(200, 120, 255), &['*', '+', '\''], 0.8);
            self.fx.light(fx, fy, [1.6, 0.8, 2.2], 4.0, 1.0);
            self.fx.shake = self.fx.shake.max(1.0);
            self.emit(Sfx::Amulet, x, y, 0.8);
        }
        if def.boss {
            self.msg(
                "VORTH, WARDEN OF GRAZZARK, FALLS! The Amulet lies unguarded!",
                Rgb(255, 80, 60),
            );
            self.fx.explosion(fx, fy, Rgb(255, 60, 30));
            self.fx.shake = 2.0;
            self.emit(Sfx::Explosion, x, y, 1.0);
        }
    }

    /// Where loot from a death at (x,y) lands: ghosts can die inside walls.
    fn drop_spot(&self, x: i32, y: i32) -> (i32, i32) {
        if self.level.tile(x, y).walkable() {
            return (x, y);
        }
        for r in 1..6 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if self.level.tile(x + dx, y + dy).walkable() {
                        return (x + dx, y + dy);
                    }
                }
            }
        }
        (self.p.x, self.p.y)
    }

    /// A monster that won a fight swells with power.
    pub fn make_champion(&mut self, k: usize) {
        let (x, y, name, voice) = {
            let m = &mut self.level.monsters[k];
            m.champion = true;
            m.max_hp = (m.max_hp as f32 * 1.6).round() as i32 + 4;
            m.hp = m.max_hp;
            (m.x, m.y, m.def().name, m.def().voice)
        };
        self.emit(Sfx::Voice(voice, Mood::Alert), x, y, 1.0);
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        self.fx
            .ring(fx, fy, 30, 1.5, Rgb(255, 190, 60), &['*', '+', '.'], 0.6);
        self.fx.light(fx, fy, [2.0, 1.4, 0.4], 2.5, 0.6);
        let ally = self.level.monsters[k].faction == Faction::Ally;
        if self.level.is_visible(x, y) {
            let who = if ally {
                format!("Your {}", name)
            } else {
                format!("The {}", name)
            };
            self.msg(
                format!(
                    "{} roars in triumph and swells with power: it has become a CHAMPION!",
                    who
                ),
                Rgb(255, 190, 70),
            );
        } else if cheb(x, y, self.p.x, self.p.y) <= 16 {
            self.msg("A triumphant cry is heard!", Rgb(220, 170, 90));
        }
    }

    /// A champion that slew another champion becomes an overlord.
    pub fn make_overlord(&mut self, k: usize) {
        let (x, y, name, voice) = {
            let m = &mut self.level.monsters[k];
            m.champion = true;
            m.overlord = true;
            m.max_hp = (m.max_hp as f32 * 1.5).round() as i32 + 10;
            m.hp = m.max_hp;
            (m.x, m.y, m.def().name, m.def().voice)
        };
        self.emit(Sfx::Voice(voice, Mood::Alert), x, y, 1.0);
        self.emit(Sfx::Rumble, x, y, 0.8);
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        self.fx
            .ring(fx, fy, 50, 2.5, Rgb(255, 40, 70), &['#', '*', '+'], 0.8);
        self.fx.light(fx, fy, [2.4, 0.3, 0.5], 3.5, 0.9);
        let thrall = self.level.monsters[k].thrall;
        if thrall {
            let t = self.time;
            let m = &mut self.level.monsters[k];
            m.abil_cd = t;
            m.roar_cd = t;
            self.fx.shake = self.fx.shake.max(0.8);
            self.msg(
                format!(
                    "Your champion {} devours its rival's power and rises as an OVERLORD THRALL!",
                    name
                ),
                Rgb(200, 120, 255),
            );
            self.msg(
                "It can now raise minions of its own and roar its enemies into flight.",
                Rgb(170, 140, 220),
            );
        } else if self.level.is_visible(x, y) {
            self.fx.shake = self.fx.shake.max(0.8);
            self.msg(
                format!(
                    "The champion {} devours its rival's power and rises as an OVERLORD!",
                    name
                ),
                Rgb(255, 60, 80),
            );
        } else if cheb(x, y, self.p.x, self.p.y) <= 20 {
            self.msg(
                "A terrible, triumphant bellow shakes the dungeon. Something has grown mightier...",
                Rgb(230, 80, 90),
            );
        }
    }

    /// Slaying an overlord grants a permanent boost for the rest of the run.
    fn steal_power(&mut self) {
        let txt = match self.rng.gen_range(0..5) {
            0 => {
                self.p.max_hp += 10;
                self.p.hp += 10;
                "+10 maximum health"
            }
            1 => {
                self.p.extra_dmg += 2;
                "+2 damage"
            }
            2 => {
                self.p.extra_ac += 1;
                "+1 armour class"
            }
            3 => {
                self.p.max_mp += 8;
                self.p.mp += 8;
                "+8 maximum mana"
            }
            _ => {
                self.p.extra_crit += 0.05;
                "+5% critical chance"
            }
        };
        self.msg(
            format!(
                "You steal the overlord's power: {} for the rest of this descent!",
                txt
            ),
            Rgb(255, 90, 110),
        );
        let (px, py) = (self.p.x, self.p.y);
        self.fx.levelup(px as f32 + 0.5, py as f32 + 0.5);
        self.emit(Sfx::LevelUp, px, py, 0.9);
    }

    /// Rewards and terror for slaying a champion at (x,y).
    fn champion_slain(&mut self, x: i32, y: i32, label: &str, drop_at: (i32, i32), overlord: bool) {
        let (px, py) = (self.p.x, self.p.y);
        let heal = if overlord {
            12 + self.p.max_hp * 25 / 100
        } else {
            8 + self.p.max_hp * 15 / 100
        };
        self.p.hp = (self.p.hp + heal).min(self.p.max_hp);
        self.fx.heal(px as f32 + 0.5, py as f32 + 0.5);
        self.msg(
            format!(
                "You slay the {}! Its strength flows into you (+{} HP).",
                label, heal
            ),
            Rgb(255, 200, 90),
        );
        if overlord {
            self.steal_power();
        }

        // Every monster that saw the kill, or heard the death scream within
        // 10 tiles, flees in terror.
        let mut scared = 0;
        for i in 0..self.level.monsters.len() {
            let (mx, my, ok, sight) = {
                let m = &self.level.monsters[i];
                let d = m.def();
                (
                    m.x,
                    m.y,
                    m.alive && m.faction != Faction::Ally && !m.fearless(),
                    d.sight as f32,
                )
            };
            if !ok {
                continue;
            }
            let heard = cheb(mx, my, x, y) <= 10;
            let saw = (dist(mx, my, x, y) <= sight && self.level.los(mx, my, x, y))
                || (dist(mx, my, px, py) <= sight && self.level.los(mx, my, px, py));
            if heard || saw {
                let turns = self.rng.gen_range(5..=20);
                let m = &mut self.level.monsters[i];
                m.awake = true;
                m.fear = m.fear.max(turns);
                m.target = None;
                scared += 1;
            }
        }
        if scared > 0 {
            self.msg(
                "Its death scream echoes through the halls. Nearby monsters flee in terror!",
                Rgb(255, 160, 90),
            );
        }

        // Champions carry treasure from deeper down.
        let luck = self.luck();
        let deeper = self.depth + 2;
        if self.rng.gen_bool(0.65) {
            let it = random_gear(deeper, &mut self.rng, luck);
            self.level.items.push(FloorItem {
                x: drop_at.0,
                y: drop_at.1,
                item: it,
            });
        }
        if self.rng.gen_bool(0.35) {
            let it = random_item(deeper, &mut self.rng, luck);
            self.level.items.push(FloorItem {
                x: drop_at.0,
                y: drop_at.1,
                item: it,
            });
        }
        // On the deepest floor, champions may carry a unique treasure.
        if self.depth == 10 && self.rng.gen_bool(0.25) {
            let pool: Vec<usize> = (N_SIGNATURE..UNIQUES.len())
                .filter(|u| !self.found_uniques.contains(u))
                .collect();
            if let Some(&u) = pool.choose(&mut self.rng) {
                self.found_uniques.push(u);
                let item = unique_item(u);
                self.level.items.push(FloorItem {
                    x: drop_at.0,
                    y: drop_at.1,
                    item,
                });
                self.msg(
                    format!(
                        "Something precious falls from the champion: {}!",
                        item.name()
                    ),
                    Rgb(220, 150, 255),
                );
                self.emit(Sfx::Amulet, x, y, 0.7);
            }
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
        self.msg(
            format!("You feel stronger! Welcome to experience level {}.", lvl),
            Rgb(255, 220, 80),
        );
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
                self.msg(
                    format!("Your ward absorbs {} damage.", a),
                    Rgb(140, 180, 255),
                );
                self.fx.flash(Rgb(120, 160, 255), 0.25);
            }
            if self.p.ward == 0 {
                self.msg("Your ward shatters!", Rgb(140, 180, 255));
            }
        }
        if dmg <= 0 {
            return;
        }
        if self.has(UPower::Stoneskin) {
            dmg = (dmg - 2).max(1);
        }
        self.p.hp -= dmg;
        if self.p.hp <= 0 && self.has(UPower::Undying) && !self.p.undying_used {
            self.p.undying_used = true;
            self.p.hp = 1;
            self.msg(
                "The Bride's locket weeps cold tears... death releases you! (1 HP)",
                Rgb(180, 220, 255),
            );
            self.fx.flash(Rgb(180, 220, 255), 0.7);
            let (px, py) = (self.p.x, self.p.y);
            self.fx
                .mist(px as f32 + 0.5, py as f32 + 0.5, Rgb(180, 220, 255));
            self.emit(Sfx::Voice(VoiceKind::Wail, Mood::Hurt), px, py, 0.7);
        }
        if self.p.hp <= 0 && self.bonus.anchor && !self.p.anchor_used {
            self.soul_anchor();
            return;
        }
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

    /// The Soul Anchor drags you back from death once per descent.
    fn soul_anchor(&mut self) {
        self.p.anchor_used = true;
        self.p.hp = (self.p.max_hp * 2 / 5).max(1);
        self.p.poison = 0;
        let (px, py) = (self.p.x, self.p.y);
        let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
        self.msg(
            "Your soul is anchored to this world! You tear yourself back from death!",
            Rgb(200, 160, 255),
        );
        self.fx.flash(Rgb(200, 150, 255), 0.9);
        self.fx
            .ring(fx, fy, 60, 4.0, Rgb(200, 140, 255), &['*', '+', '@'], 0.8);
        self.fx.light(fx, fy, [1.8, 1.2, 2.4], 5.0, 1.2);
        self.fx.shake = self.fx.shake.max(1.2);
        self.emit(Sfx::Amulet, px, py, 1.0);
        for m in self.level.monsters.iter_mut() {
            if m.alive && m.faction != Faction::Ally && !m.fearless() && cheb(m.x, m.y, px, py) <= 5
            {
                m.fear = m.fear.max(6);
                m.target = None;
            }
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

    fn explode(
        &mut self,
        cx: i32,
        cy: i32,
        dice: i32,
        bonus: i32,
        cause: DeathCause,
        hurts_player: bool,
    ) {
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
            self.fx.text(
                mx as f32 + 0.5,
                my as f32 + 0.5,
                0.9,
                dmg.to_string(),
                Rgb(255, 150, 40),
            );
            self.damage_monster(mi, dmg, Src::Player);
        }
        if hurts_player && cheb(self.p.x, self.p.y, cx, cy) <= 1 {
            let mut dmg = bonus;
            for _ in 0..dice {
                dmg += self.rng.gen_range(1..=6);
            }
            dmg /= 2;
            self.msg(
                format!("You are caught in the blast! ({} damage)", dmg),
                Rgb(255, 120, 40),
            );
            self.player_hit(dmg, cause, false);
        }
        self.noise(cx, cy, 12);
        self.combat_heat = 1.0;
    }

    fn use_skill(&mut self, s: Skill) -> Option<f64> {
        if self.cursed(Curse::Hush) {
            self.msg(
                "The walls swallow the words before they leave your lips.",
                Curse::Hush.color(),
            );
            return None;
        }
        let wait = self.skill_ready_in(s);
        if wait > 0.0 {
            self.msg(format!("{} is not ready ({} turns).", s.name(), wait), GREY);
            return None;
        }
        let cost = self.skill_cost(s);
        // Blood Magic: pay the shortfall in health, never fatally.
        let blood = if self.p.mp < cost {
            let need = (cost - self.p.mp) * 2;
            if !self.bonus.bloodmagic || self.p.hp <= need {
                self.msg(
                    format!("Not enough mana for {}.", s.name()),
                    Rgb(120, 140, 255),
                );
                return None;
            }
            need
        } else {
            0
        };
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
                    self.fx
                        .ring(fx, fy, 24, 1.5, Rgb(230, 230, 255), &['-', '=', '~'], 0.3);
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
                self.fx.ring(
                    fx,
                    fy,
                    40,
                    3.0,
                    Rgb(170, 140, 100),
                    &['.', ',', 'o', '~'],
                    0.6,
                );
                self.msg(
                    "You stomp the ground with tremendous force!",
                    Rgb(255, 200, 120),
                );
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
                    self.fx.bolt(
                        fx,
                        fy,
                        hx as f32 + 0.5,
                        hy as f32 + 0.5,
                        20.0,
                        '+',
                        Rgb(200, 210, 230),
                        None,
                        BoltEnd::Sparks(Rgb(220, 220, 240)),
                    );
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
                self.fx.bolt(
                    fx,
                    fy,
                    cx as f32 + 0.5,
                    cy as f32 + 0.5,
                    12.0,
                    '@',
                    Rgb(255, 140, 30),
                    Some([2.2, 1.0, 0.3]),
                    BoltEnd::Explosion(Rgb(255, 120, 20)),
                );
                self.emit(Sfx::Fireball, px, py, 0.8);
                self.emit(Sfx::Explosion, cx, cy, 1.0);
                self.msg("You hurl a roaring fireball!", Rgb(255, 160, 60));
                self.explode(cx, cy, 3, 4 + lvl, DeathCause::OwnFire, true);
                true
            }
            Skill::FrostNova => {
                self.fx
                    .ring(fx, fy, 48, 3.5, Rgb(160, 220, 255), &['*', '+', 'x'], 0.6);
                self.fx.ice(fx, fy);
                self.emit(Sfx::Frost, px, py, 1.0);
                self.msg(
                    "A wave of bitter frost explodes outward!",
                    Rgb(150, 210, 255),
                );
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
                if self
                    .level
                    .monsters
                    .iter()
                    .any(|m| m.alive && m.summoned && m.kind == M_WOLF)
                {
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
                    self.fx
                        .mist(sx as f32 + 0.5, sy as f32 + 0.5, Rgb(180, 200, 255));
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
                self.msg(
                    format!("Holy light mends your wounds (+{}).", amt),
                    Rgb(255, 240, 150),
                );
                true
            }
            Skill::TurnUndead => {
                let ids: Vec<usize> = self
                    .monsters_within(px, py, 6.5, true)
                    .into_iter()
                    .filter(|&i| self.level.monsters[i].def().faction == Faction::Undead)
                    .collect();
                self.fx
                    .ring(fx, fy, 60, 5.0, Rgb(255, 250, 200), &['+', '*', '\''], 0.8);
                self.fx.flash(Rgb(255, 250, 220), 0.4);
                self.fx.light(fx, fy, [2.5, 2.4, 1.8], 6.0, 0.8);
                self.emit(Sfx::Heal, px, py, 1.0);
                if ids.is_empty() {
                    self.msg(
                        "Holy light floods the area... but no undead are near.",
                        Rgb(255, 240, 180),
                    );
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
                let thralls = self
                    .level
                    .monsters
                    .iter()
                    .filter(|m| m.alive && m.thrall)
                    .count();
                if thralls >= 3 {
                    self.msg("You cannot bind any more thralls.", GREY);
                    false
                } else {
                    let opts = self.raise_options();
                    let pick = match self.raise_pick {
                        Some((x, y)) => opts.iter().find(|o| o.x == x && o.y == y).copied(),
                        None if opts.len() > 1 => {
                            // More than one body answers: let the player choose.
                            self.raise_menu = opts.clone();
                            None
                        }
                        None => opts.first().copied(),
                    };
                    match pick {
                        None => {
                            if self.raise_menu.is_empty() {
                                let spent = self
                                    .level
                                    .corpses
                                    .iter()
                                    .any(|c| dist(c.x, c.y, px, py) <= 5.5 && c.raises > 0);
                                if spent && opts.is_empty() {
                                    self.msg(
                                        "The fallen thralls nearby have nothing left to give you.",
                                        GREY,
                                    );
                                } else {
                                    self.msg("There are no corpses nearby to raise.", GREY);
                                }
                            }
                            false
                        }
                        Some(o) => {
                            let Some(ci) = self
                                .level
                                .corpses
                                .iter()
                                .position(|c| c.x == o.x && c.y == o.y && c.kind == o.kind)
                            else {
                                return None;
                            };
                            let c = self.level.corpses.remove(ci);
                            let id = self.new_id();
                            let mut m = Monster::new(id, c.kind, c.x, c.y, self.time + 50.0);
                            m.faction = Faction::Ally;
                            m.summoned = true;
                            m.thrall = true;
                            m.raises = c.raises + 1;
                            m.awake = true;
                            self.level.monsters.push(m);
                            self.fx
                                .mist(c.x as f32 + 0.5, c.y as f32 + 0.5, Rgb(120, 255, 120));
                            self.emit(Sfx::Revive, c.x, c.y, 1.0);
                            let again = if o.again { " once more" } else { "" };
                            self.msg(
                                format!(
                                    "The {} rises{} to serve you! (-{} max HP while it lives)",
                                    MONSTERS[c.kind].name, again, o.cost
                                ),
                                Rgb(150, 255, 150),
                            );
                            self.sync_upkeep();
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
                        self.fx.bolt(
                            hx as f32 + 0.5,
                            hy as f32 + 0.5,
                            fx,
                            fy,
                            10.0,
                            '*',
                            Rgb(150, 255, 120),
                            Some([0.4, 1.4, 0.5]),
                            BoltEnd::None,
                        );
                        self.emit(Sfx::Bolt, px, py, 0.8);
                        let heal = dmg / 2 + 1;
                        self.p.hp = (self.p.hp + heal).min(self.p.max_hp);
                        let name = self.level.monsters[mi].def().name;
                        self.msg(
                            format!("You drain {} life from the {}.", dmg, name),
                            Rgb(150, 255, 150),
                        );
                        self.fx.text(
                            hx as f32 + 0.5,
                            hy as f32 + 0.5,
                            0.9,
                            dmg.to_string(),
                            Rgb(150, 255, 120),
                        );
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
                        self.fx
                            .rise(fx, fy, 20, Rgb(180, 120, 255), &['*', '.', '+'], 0.8);
                        self.p.x = x;
                        self.p.y = y;
                        self.fx.rise(
                            x as f32 + 0.5,
                            y as f32 + 0.5,
                            20,
                            Rgb(180, 120, 255),
                            &['*', '.', '+'],
                            0.8,
                        );
                        self.emit(Sfx::Blink, x, y, 0.8);
                        self.after_player_move();
                        true
                    }
                }
            }
            Skill::SoulBurst => {
                self.fx
                    .ring(fx, fy, 60, 4.0, Rgb(200, 120, 255), &['*', '@', '%'], 0.7);
                self.fx.light(fx, fy, [1.5, 0.6, 2.2], 4.0, 0.7);
                self.fx.shake = 1.0;
                self.emit(Sfx::Explosion, px, py, 0.9);
                self.msg("Soulfire bursts from your body!", Rgb(210, 150, 255));
                let ids = self.monsters_within(px, py, 2.9, true);
                let heal = ids.len() as i32 * 2;
                for mi in ids {
                    let dmg = (0..3).map(|_| self.rng.gen_range(1..=6)).sum::<i32>() + lvl;
                    self.damage_monster(mi, dmg, Src::Player);
                }
                if heal > 0 {
                    self.p.hp = (self.p.hp + heal).min(self.p.max_hp);
                }
                true
            }
            Skill::PhaseWard => {
                self.p.ward = 20 + lvl * 2;
                self.fx
                    .ring(fx, fy, 30, 1.0, Rgb(120, 170, 255), &['o', 'O', '0'], 0.6);
                self.emit(Sfx::Frost, px, py, 0.7);
                let w = self.p.ward;
                self.msg(
                    format!("A shimmering ward surrounds you ({}).", w),
                    Rgb(140, 180, 255),
                );
                true
            }
            Skill::SecondWind => {
                let amt = self.p.max_hp / 2;
                self.p.hp = (self.p.hp + amt).min(self.p.max_hp);
                self.p.poison = 0;
                self.fx.heal(fx, fy);
                self.emit(Sfx::Heal, px, py, 0.9);
                self.msg(
                    format!("You catch your second wind (+{}).", amt),
                    Rgb(150, 255, 150),
                );
                true
            }
            Skill::BattleCry => {
                self.p.fury = 8;
                self.fx.shake = self.fx.shake.max(1.0);
                self.fx.ring(
                    fx,
                    fy,
                    50,
                    5.0,
                    Rgb(255, 120, 60),
                    &['!', '>', '<', '~'],
                    0.6,
                );
                self.fx.flash(Rgb(255, 90, 40), 0.3);
                self.emit(Sfx::Voice(VoiceKind::Roar, Mood::Alert), px, py, 1.0);
                self.emit(Sfx::Stomp, px, py, 0.8);
                let ids = self.monsters_within(px, py, 5.5, true);
                let mut fled = 0;
                for mi in ids {
                    let m = &mut self.level.monsters[mi];
                    if m.fearless() {
                        continue;
                    }
                    m.fear = m.fear.max(4 + (mi as i32 % 3));
                    m.target = None;
                    fled += 1;
                }
                self.msg(
                    if fled > 0 {
                        "You bellow a terrible battle cry! Your foes quail, and fury fills you!"
                    } else {
                        "You bellow a terrible battle cry! Fury fills you!"
                    },
                    Rgb(255, 140, 80),
                );
                self.noise(px, py, 14);
                true
            }
            Skill::Shadowstep => {
                let sight = self.sight() as f32;
                let mut best: Option<(usize, f32)> = None;
                for (i, m) in self.level.monsters.iter().enumerate() {
                    if !m.alive || m.faction == Faction::Ally || !self.level.is_visible(m.x, m.y) {
                        continue;
                    }
                    let d = dist(px, py, m.x, m.y);
                    if d <= 6.5f32.min(sight) && d > 1.5 && best.map_or(true, |b| d < b.1) {
                        best = Some((i, d));
                    }
                }
                let spot = best.and_then(|(mi, _)| {
                    let (mx, my) = (self.level.monsters[mi].x, self.level.monsters[mi].y);
                    let (sx, sy) = ((mx - px).signum(), (my - py).signum());
                    let behind = (mx + sx, my + sy);
                    let ok = |g: &Game, x: i32, y: i32| {
                        g.level.tile(x, y).walkable()
                            && g.level.monster_at(x, y).is_none()
                            && !(x == px && y == py)
                    };
                    if ok(self, behind.0, behind.1) {
                        Some((mi, behind))
                    } else {
                        DIRS.iter()
                            .map(|&(dx, dy)| (mx + dx, my + dy))
                            .find(|&(x, y)| ok(self, x, y))
                            .map(|p| (mi, p))
                    }
                });
                match spot {
                    None => {
                        self.msg("There is no foe in sight to step behind.", GREY);
                        false
                    }
                    Some((mi, (x, y))) => {
                        self.fx.mist(fx, fy, Rgb(70, 70, 110));
                        self.p.x = x;
                        self.p.y = y;
                        self.cam.x = x as f32 + 0.5;
                        self.cam.y = y as f32 + 0.5;
                        let (mx, my) = (self.level.monsters[mi].x, self.level.monsters[mi].y);
                        if let Some(d) = DIRS
                            .iter()
                            .position(|&d| d == ((mx - x).signum(), (my - y).signum()))
                        {
                            self.p.facing = d as i32;
                        }
                        self.fx
                            .mist(x as f32 + 0.5, y as f32 + 0.5, Rgb(70, 70, 110));
                        self.emit(Sfx::Blink, x, y, 0.6);
                        self.msg("You step through the shadows!", Rgb(160, 160, 220));
                        // It never saw you coming.
                        self.level.monsters[mi].target = None;
                        self.refresh_view();
                        self.cam.swing = 1.0;
                        self.player_strike(mi, 1.0, 4, None);
                        true
                    }
                }
            }
            Skill::SmokeBomb => {
                self.fx.mist(fx, fy, Rgb(120, 120, 130));
                self.fx.burst(
                    fx,
                    fy,
                    0.3,
                    50,
                    Rgb(110, 110, 120),
                    &['%', '@', 'o', '.'],
                    1.2,
                    2.5,
                    -0.2,
                    false,
                );
                self.emit(Sfx::Explosion, px, py, 0.4);
                let ids: Vec<usize> = self
                    .level
                    .monsters
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| {
                        m.alive && m.faction != Faction::Ally && cheb(m.x, m.y, px, py) <= 4
                    })
                    .map(|(i, _)| i)
                    .collect();
                for mi in ids {
                    let m = &mut self.level.monsters[mi];
                    m.target = None;
                    m.last_known = None;
                    if !m.def().boss {
                        m.stun = m.stun.max(2);
                    }
                }
                self.p.invis = self.p.invis.max(5);
                self.msg(
                    "You hurl a smoke bomb and vanish in the choking cloud!",
                    Rgb(170, 170, 190),
                );
                true
            }
            Skill::ChainLightning => {
                let first = self.fire_line(f, 8).2.or_else(|| {
                    self.level
                        .monsters
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| {
                            m.alive
                                && m.faction != Faction::Ally
                                && self.level.is_visible(m.x, m.y)
                                && dist(px, py, m.x, m.y) <= 6.5
                        })
                        .min_by(|a, b| {
                            dist(px, py, a.1.x, a.1.y)
                                .partial_cmp(&dist(px, py, b.1.x, b.1.y))
                                .unwrap()
                        })
                        .map(|(i, _)| i)
                });
                match first {
                    None => {
                        self.msg("There is nothing for the lightning to strike.", GREY);
                        false
                    }
                    Some(mi) => {
                        self.msg("Lightning leaps from your fingertips!", Rgb(170, 200, 255));
                        let mut dmg = (0..3).map(|_| self.rng.gen_range(1..=6)).sum::<i32>() + lvl;
                        let mut hit: Vec<u32> = Vec::new();
                        let (mut fx0, mut fy0) = (px, py);
                        let mut cur = Some(mi);
                        while let Some(ci) = cur {
                            let (cx, cy, cid) = {
                                let m = &self.level.monsters[ci];
                                (m.x, m.y, m.id)
                            };
                            hit.push(cid);
                            self.fx.bolt(
                                fx0 as f32 + 0.5,
                                fy0 as f32 + 0.5,
                                cx as f32 + 0.5,
                                cy as f32 + 0.5,
                                30.0,
                                '~',
                                Rgb(170, 200, 255),
                                Some([0.8, 1.0, 2.2]),
                                BoltEnd::Sparks(Rgb(200, 220, 255)),
                            );
                            self.emit(Sfx::Bolt, cx, cy, 0.7);
                            self.fx.text(
                                cx as f32 + 0.5,
                                cy as f32 + 0.5,
                                0.9,
                                dmg.to_string(),
                                Rgb(170, 200, 255),
                            );
                            self.damage_monster(ci, dmg, Src::Player);
                            if hit.len() >= 5 {
                                break;
                            }
                            dmg = (dmg * 4 / 5).max(1);
                            fx0 = cx;
                            fy0 = cy;
                            cur = self
                                .level
                                .monsters
                                .iter()
                                .enumerate()
                                .filter(|(_, o)| {
                                    o.alive
                                        && o.faction != Faction::Ally
                                        && !hit.contains(&o.id)
                                        && cheb(o.x, o.y, cx, cy) <= 4
                                        && self.level.los(cx, cy, o.x, o.y)
                                })
                                .min_by_key(|(_, o)| cheb(o.x, o.y, cx, cy))
                                .map(|(i, _)| i);
                        }
                        self.noise(px, py, 8);
                        true
                    }
                }
            }
            Skill::Smite => {
                let (hx, hy, mi) = self.fire_line(f, 8);
                match mi {
                    None => {
                        self.msg("There is nothing ahead to smite.", GREY);
                        false
                    }
                    Some(mi) => {
                        let unholy = matches!(
                            self.level.monsters[mi].def().faction,
                            Faction::Undead | Faction::Demon
                        );
                        let mut dmg =
                            (0..2).map(|_| self.rng.gen_range(1..=8)).sum::<i32>() + lvl + 2;
                        if unholy {
                            dmg *= 2;
                        }
                        let (tx, ty) = (hx as f32 + 0.5, hy as f32 + 0.5);
                        self.fx
                            .ring(tx, ty, 30, 1.2, Rgb(255, 245, 180), &['+', '|', '*'], 0.6);
                        self.fx.light(tx, ty, [2.6, 2.4, 1.6], 3.0, 0.6);
                        self.fx.flash(Rgb(255, 250, 210), 0.25);
                        self.emit(Sfx::Heal, hx, hy, 1.0);
                        self.fx
                            .text(tx, ty, 0.9, dmg.to_string(), Rgb(255, 240, 150));
                        let name = self.level.monsters[mi].label();
                        self.msg(
                            if unholy {
                                format!("Holy fire SEARS the unholy {} ({})!", name, dmg)
                            } else {
                                format!("Holy fire smites the {} ({}).", name, dmg)
                            },
                            Rgb(255, 240, 150),
                        );
                        self.damage_monster(mi, dmg, Src::Player);
                        self.p.hp = (self.p.hp + dmg / 4).min(self.p.max_hp);
                        true
                    }
                }
            }
            Skill::PiercingShot => {
                let (dx, dy) = DIRS[f as usize];
                let custom = if self.wdef().range > 0 {
                    None
                } else {
                    Some((2, 8))
                };
                let mut end = (px, py);
                let mut targets = Vec::new();
                for k in 1..=10 {
                    let (x, y) = (px + dx * k, py + dy * k);
                    if self.level.tile(x, y).opaque() {
                        break;
                    }
                    end = (x, y);
                    if let Some(mi) = self.level.monster_at(x, y) {
                        if self.level.monsters[mi].faction != Faction::Ally {
                            targets.push(mi);
                        }
                    }
                }
                let ch = ['|', '/', '-', '\\', '|', '/', '-', '\\'][f as usize];
                self.fx.bolt(
                    fx,
                    fy,
                    end.0 as f32 + 0.5,
                    end.1 as f32 + 0.5,
                    30.0,
                    ch,
                    Rgb(255, 230, 160),
                    Some([1.2, 1.0, 0.5]),
                    BoltEnd::Sparks(Rgb(255, 230, 160)),
                );
                self.emit(Sfx::Arrow, px, py, 0.9);
                self.msg(
                    "You loose an arrow that tears through everything in its path!",
                    Rgb(230, 230, 150),
                );
                for mi in targets {
                    self.player_strike(mi, 1.25, 3, custom);
                }
                self.noise(px, py, 5);
                true
            }
            Skill::RainOfArrows => {
                let (dx, dy) = DIRS[f as usize];
                let mut c = (px, py);
                for k in 1..=5 {
                    let (x, y) = (px + dx * k, py + dy * k);
                    if self.level.tile(x, y).opaque() {
                        break;
                    }
                    c = (x, y);
                    if self.level.monster_at(x, y).is_some() {
                        break;
                    }
                }
                let (cx, cy) = (c.0 as f32 + 0.5, c.1 as f32 + 0.5);
                for _ in 0..8 {
                    let ox = self.rng.gen_range(-1.2..1.2);
                    let oy = self.rng.gen_range(-1.2..1.2);
                    self.fx.bolt(
                        fx,
                        fy,
                        cx + ox,
                        cy + oy,
                        18.0 + self.rng.gen_range(0.0..8.0),
                        '\'',
                        Rgb(225, 205, 160),
                        None,
                        BoltEnd::Sparks(Rgb(200, 190, 160)),
                    );
                }
                self.emit(Sfx::Arrow, px, py, 1.0);
                self.emit(Sfx::Arrow, c.0, c.1, 0.8);
                self.msg("A rain of arrows falls upon your foes!", Rgb(200, 230, 160));
                let ids: Vec<usize> = self
                    .level
                    .monsters
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| {
                        m.alive && m.faction != Faction::Ally && cheb(m.x, m.y, c.0, c.1) <= 1
                    })
                    .map(|(i, _)| i)
                    .collect();
                for mi in ids {
                    let dmg = (0..2).map(|_| self.rng.gen_range(1..=6)).sum::<i32>() + lvl / 2 + 1;
                    let (mx, my) = (self.level.monsters[mi].x, self.level.monsters[mi].y);
                    self.fx.text(
                        mx as f32 + 0.5,
                        my as f32 + 0.5,
                        0.9,
                        dmg.to_string(),
                        Rgb(230, 220, 170),
                    );
                    self.damage_monster(mi, dmg, Src::Player);
                }
                self.noise(px, py, 6);
                true
            }
            Skill::CorpseExplosion => {
                let mut picks: Vec<usize> = self
                    .level
                    .corpses
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        dist(c.x, c.y, px, py) <= 6.5 && self.level.los(px, py, c.x, c.y)
                    })
                    .map(|(i, _)| i)
                    .collect();
                picks.truncate(6);
                if picks.is_empty() {
                    self.msg("There are no corpses in reach to detonate.", GREY);
                    false
                } else {
                    self.msg(
                        format!(
                            "You detonate {} corpse{}!",
                            picks.len(),
                            if picks.len() == 1 { "" } else { "s" }
                        ),
                        Rgb(170, 255, 140),
                    );
                    let spots: Vec<(i32, i32)> = picks
                        .iter()
                        .map(|&i| (self.level.corpses[i].x, self.level.corpses[i].y))
                        .collect();
                    for &i in picks.iter().rev() {
                        self.level.corpses.remove(i);
                    }
                    for (x, y) in spots {
                        self.fx
                            .explosion(x as f32 + 0.5, y as f32 + 0.5, Rgb(140, 255, 90));
                        self.fx.blood(x as f32 + 0.5, y as f32 + 0.5, 0.6, 16);
                        self.emit(Sfx::Explosion, x, y, 0.8);
                        self.explode(x, y, 2, 2 + lvl / 2, DeathCause::OwnFire, false);
                    }
                    self.fx.shake = self.fx.shake.max(0.8);
                    true
                }
            }
        };
        if ok {
            if blood > 0 {
                self.p.hp -= blood;
                self.p.mp = 0;
                self.msg(
                    format!("Your blood fuels the spell (-{} HP).", blood),
                    Rgb(200, 40, 60),
                );
                self.fx.blood(fx, fy, 0.3, 8);
            } else {
                self.p.mp -= cost;
            }
            self.p.cds.insert(s, self.time + s.cooldown() * 100.0);
            self.combat_heat = self.combat_heat.max(0.5);
            Some(100.0)
        } else {
            None
        }
    }

    /// Gather everything here for one turn.
    fn pickup_all(&mut self) -> Option<f64> {
        let mut took = false;
        loop {
            let here = self.items_here();
            // Pick from the back so earlier indices stay valid; stop when full.
            let Some(&ii) = here.iter().rev().find(|&&i| {
                let it = self.level.items[i].item;
                it == Item::Amulet || self.can_carry(&it)
            }) else {
                break;
            };
            if self.pickup_index(ii).is_none() {
                break;
            }
            took = true;
        }
        if !self.items_here().is_empty() && took {
            self.msg("Your pack is full; some items remain.", GREY);
        }
        if took {
            Some(100.0)
        } else {
            if self.items_here().is_empty() {
                self.msg("There is nothing here to pick up.", GREY);
            } else {
                self.msg("Your pack is full.", GREY);
            }
            None
        }
    }

    fn pickup_index(&mut self, ii: usize) -> Option<f64> {
        let (px, py) = (self.p.x, self.p.y);
        let item = self.level.items[ii].item;
        if item == Item::Amulet {
            self.level.items.remove(ii);
            self.p.has_amulet = true;
            let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
            self.fx
                .ring(fx, fy, 60, 4.0, Rgb(255, 210, 60), &['*', '+', '$'], 1.0);
            self.fx.flash(Rgb(255, 200, 40), 0.8);
            self.fx.light(fx, fy, [2.5, 2.0, 0.6], 6.0, 2.0);
            self.fx.shake = 1.5;
            self.emit(Sfx::Amulet, px, py, 1.0);
            self.msg(
                "You seize the AMULET OF GRAZZARK! His heart beats against your palm.",
                Rgb(255, 215, 60),
            );
            self.msg("The whole dungeon shudders awake. Carry his heart into the SUNLIGHT and it will burn!", Rgb(255, 150, 60));
            self.msg(
                "Every creature in GRAZZARK's halls now knows where you are...",
                Rgb(255, 90, 60),
            );
            self.msg("The faces in the walls begin to shriek in torment. Some of them fall silent forever.", Rgb(220, 160, 160));
            self.crumble_all_wards();
            self.rouse_all();
            self.combat_heat = 1.0;
            return Some(100.0);
        }
        if !self.can_carry(&item) {
            self.msg("Your pack is full.", GREY);
            return None;
        }
        self.level.items.remove(ii);
        self.msg(format!("You pick up {}.", item.name()), Rgb(220, 220, 150));
        if let Some(u) = item.unique() {
            let d = &UNIQUES[u];
            self.msg(
                format!(
                    "A unique treasure! {} (equip it from the inventory)",
                    d.desc
                ),
                d.color,
            );
        }
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
        self.level.items.push(FloorItem {
            x: px,
            y: py,
            item: it,
        });
        self.msg(format!("You drop {}.", it.name()), GREY);
        Some(50.0)
    }

    /// Alchemy: potions are 25% stronger per rank.
    fn potency(&self, amt: i32) -> i32 {
        ((amt as f32) * (1.0 + 0.25 * self.bonus.alchemy as f32)).round() as i32
    }

    /// The effect of drinking a potion of kind `k`.
    fn drink(&mut self, k: Potion) {
        let (px, py) = (self.p.x, self.p.y);
        let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
        match k {
            Potion::Healing => {
                let amt = 12 + self.depth as i32 * 2 + self.p.max_hp / 4;
                let amt = self.potency(amt);
                self.p.hp = (self.p.hp + amt).min(self.p.max_hp);
                self.p.poison = 0;
                self.msg(
                    format!("You feel much better (+{} HP).", amt),
                    Rgb(255, 120, 120),
                );
            }
            Potion::Mana => {
                let amt = self.potency(10 + self.p.max_mp / 2);
                self.p.mp = (self.p.mp + amt).min(self.p.max_mp);
                self.msg(
                    format!("Arcane energy surges through you (+{} MP).", amt),
                    Rgb(120, 150, 255),
                );
            }
            Potion::Might => {
                let n = if self.bonus.alchemy >= 3 { 2 } else { 1 };
                self.p.extra_dmg += n;
                self.msg(
                    format!("Your muscles swell with power! (+{} damage)", n),
                    Rgb(255, 170, 60),
                );
            }
            Potion::Vigor => {
                let n = self.potency(5);
                self.p.max_hp += n;
                self.p.hp += n;
                self.msg(
                    format!("You feel hardier! (+{} max HP)", n),
                    Rgb(120, 255, 120),
                );
            }
            Potion::Cure => {
                self.p.poison = 0;
                let n = self.potency(5);
                self.p.hp = (self.p.hp + n).min(self.p.max_hp);
                self.msg("The poison is purged from your blood.", Rgb(200, 255, 200));
            }
        }
        self.fx.heal(fx, fy);
        self.emit(Sfx::Heal, px, py, 0.6);
    }

    /// Eager Gulp: drink any Potions of Might or Vigor lying where you stand.
    fn gulp_potions(&mut self) {
        if !self.bonus.gulp {
            return;
        }
        let (px, py) = (self.p.x, self.p.y);
        while let Some(i) = self.level.items.iter().position(|it| {
            it.x == px
                && it.y == py
                && matches!(it.item, Item::Potion(Potion::Might) | Item::Potion(Potion::Vigor))
        }) {
            let Item::Potion(k) = self.level.items.remove(i).item else {
                unreachable!()
            };
            self.msg(format!("You snatch up the {} and gulp it down.", Item::Potion(k).name()), Rgb(220, 210, 120));
            self.drink(k);
        }
    }

    /// Appraiser's Eye: how much better `it` is than what you wear, as
    /// (min damage, max damage) for a weapon or (AC, 0) for armour. None if it
    /// is not gear, or is no better.
    pub fn upgrade_over_worn(&self, it: &Item) -> Option<(i32, i32)> {
        match it {
            Item::Weapon(gr) => {
                let (n, w) = (&WEAPONS[gr.def], &WEAPONS[self.p.wpn.def]);
                let dmin = (n.dmg.0 + gr.bonus) - (w.dmg.0 + self.p.wpn.bonus);
                let dmax = (n.dmg.1 + gr.bonus) - (w.dmg.1 + self.p.wpn.bonus);
                (dmin + dmax > 0).then_some((dmin, dmax))
            }
            Item::Armor(gr) => {
                let d = (ARMORS[gr.def].ac + gr.bonus) - (ARMORS[self.p.arm.def].ac + self.p.arm.bonus);
                (d > 0).then_some((d, 0))
            }
            _ => None,
        }
    }

    pub fn use_item(&mut self, i: usize) -> Option<f64> {
        if i >= self.p.inv.len() {
            return None;
        }
        let it = self.p.inv[i];
        if matches!(it, Item::Scroll(_)) && self.cursed(Curse::Hush) {
            self.msg(
                "You open the scroll, but no sound will leave your throat.",
                Curse::Hush.color(),
            );
            return None;
        }
        let (px, py) = (self.p.x, self.p.y);
        let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
        match it {
            Item::Potion(k) => {
                self.p.inv.remove(i);
                self.drink(k);
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
                        self.fx
                            .rise(fx, fy, 25, Rgb(180, 120, 255), &['*', '.', '+'], 0.8);
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
                                let near_open = DIRS
                                    .iter()
                                    .any(|&(dx, dy)| !self.level.tile(x + dx, y + dy).opaque())
                                    || !self.level.tile(x, y).opaque();
                                if near_open {
                                    let i = self.level.idx(x, y);
                                    self.level.explored[i] = true;
                                }
                            }
                        }
                        self.emit(Sfx::Amulet, px, py, 0.4);
                        self.msg(
                            "A map of the level burns itself into your mind.",
                            Rgb(230, 220, 160),
                        );
                    }
                    Scroll::Fire => {
                        self.fx.explosion(fx, fy, Rgb(255, 120, 20));
                        self.fx
                            .ring(fx, fy, 50, 4.0, Rgb(255, 140, 30), &['*', '#', '%'], 0.6);
                        self.emit(Sfx::Explosion, px, py, 1.0);
                        self.msg("A ring of fire erupts around you!", Rgb(255, 150, 60));
                        let ids = self.monsters_within(px, py, 2.9, true);
                        for mi in ids {
                            let dmg = (0..4).map(|_| self.rng.gen_range(1..=6)).sum::<i32>();
                            self.damage_monster(mi, dmg, Src::Player);
                        }
                        self.noise(px, py, 12);
                    }
                    Scroll::Vision => {
                        self.p.vision = 10;
                        self.fx
                            .ring(fx, fy, 40, 5.0, Rgb(255, 120, 90), &['o', '.', '*'], 0.7);
                        self.emit(Sfx::Whisper, px, py, 0.8);
                        let n = self
                            .level
                            .monsters
                            .iter()
                            .filter(|m| self.vision_shows(m) && m.faction != Faction::Ally)
                            .count();
                        self.msg(
                            format!(
                                "Your eyes burn red. You sense {} creature{} nearby.",
                                n,
                                if n == 1 { "" } else { "s" }
                            ),
                            Rgb(255, 140, 110),
                        );
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
            Item::Trinket(u) => {
                let d = &UNIQUES[u as usize];
                let slot = if d.slot == USlot::Ring {
                    &mut self.p.ring
                } else {
                    &mut self.p.neck
                };
                let old = slot.replace(u);
                match old {
                    Some(o) => self.p.inv[i] = Item::Trinket(o),
                    None => {
                        self.p.inv.remove(i);
                    }
                }
                let verb = if d.slot == USlot::Ring {
                    "slip on"
                } else {
                    "fasten"
                };
                self.msg(format!("You {} {}. {}", verb, d.name, d.desc), d.color);
                self.emit(Sfx::Amulet, px, py, 0.4);
                Some(100.0)
            }
            Item::Amulet => None,
        }
    }

    fn use_stairs(&mut self) -> Option<f64> {
        let (px, py) = (self.p.x, self.p.y);
        match self.level.tile(px, py) {
            Tile::StairsDown => {
                if self.depth >= 10 {
                    self.msg(
                        "The stair plunges into a wall of black flame. Nothing lies deeper.",
                        Rgb(255, 120, 80),
                    );
                    None
                } else {
                    self.change_level(true);
                    Some(100.0)
                }
            }
            Tile::StairsUp => {
                if self.depth == 1 {
                    if self.p.has_amulet {
                        self.msg(
                            "You climb into the light of day, the Amulet blazing in your fist!",
                            Rgb(255, 230, 120),
                        );
                        self.over = Some(Outcome::Won);
                        Some(0.0)
                    } else {
                        self.msg(
                            "You cannot leave without the Amulet of GRAZZARK!",
                            Rgb(255, 160, 80),
                        );
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

        // Hand back stolen memories before the floor is put away.
        self.restore_memory();
        self.figments.clear();
        let new_level = match self.stash.remove(&new_depth) {
            Some(l) => l,
            None => {
                let mut l = generate(new_depth, &mut self.rng);
                l.style = self.style_for(new_depth);
                let luck = self.bonus.luck;
                let chance = self.unique_chance[(new_depth as usize - 1).min(9)];
                let unique = self.rng.gen::<f32>() < chance;
                let hp_mult = self.diff().hp;
                populate(
                    &mut l,
                    &mut self.rng,
                    &mut self.next_id,
                    luck,
                    self.time,
                    unique,
                    hp_mult,
                );
                if self.ward_depths.contains(&new_depth) && !self.p.has_amulet {
                    place_ward(&mut l, &mut self.rng);
                }
                l.theme_doors();
                chart_level(&mut l, self.bonus.carto);
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
        self.p.undying_used = false;
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

        self.sync_upkeep();
        self.emit(Sfx::Stairs, sx, sy, 0.8);
        let name = self.floor_name();
        self.msg(
            format!(
                "You {} to depth {}: {}.",
                if down { "descend" } else { "climb" },
                new_depth,
                name
            ),
            Rgb(200, 180, 255),
        );
        if self.p.has_amulet && !down {
            let t = match new_depth {
                7..=9 => "Thin red veins creep through the mortar here. The stones are... breathing.",
                4..=6 => "Flesh bulges between the stones and eyes blink open in the walls. The dungeon is ALIVE.",
                _ => "The halls are a living throat of meat and sinew. It does not want to let you go!",
            };
            self.msg(t, Rgb(230, 90, 90));
        }
        if self.cursed(Curse::Forgetting) {
            self.forget_floor();
        }
        if self.cursed(Curse::Figments) {
            self.spawn_figments();
        }
        if new_depth == 10 && down {
            self.msg(
                "An overwhelming malice fills these halls. The Warden is near.",
                Rgb(255, 80, 80),
            );
            self.weaken_wardens();
            let n = self.wards_destroyed.min(4);
            let t = match n {
                0 => "Four ward prisms still feed the Warden's power. It is at its full, terrible strength.".to_string(),
                4 => "All four wards are broken. The Warden is a hollow shell of what it was.".to_string(),
                _ => format!("{} of the 4 wards lie broken. The Warden's power is diminished.", n),
            };
            self.msg(t, Rgb(220, 170, 255));
        }
        self.announce_unique();

        if self.p.has_amulet && !self.level.hunted {
            self.level.hunted = true;
            self.infest_level();
            // The dead on this floor stir faster the nearer you are to the surface.
            let f = self.respawn_factor();
            let now = self.time;
            for c in self.level.corpses.iter_mut() {
                if c.revive_at > now {
                    c.revive_at = now + (c.revive_at - now) * f;
                }
            }
            let kinds: Vec<usize> = (0..MONSTERS.len())
                .filter(|&k| {
                    let d = &MONSTERS[k];
                    !d.boss && d.depth.1 >= new_depth && d.depth.0 <= new_depth + 3
                })
                .collect();
            // The Amulet's power draws about 10% more hunters.
            let n = ((2 + new_depth / 3) as f32 * 1.1).round() as u32;
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
                    m.scale_hp(self.diff().hp);
                    self.level.monsters.push(m);
                }
            }
            self.msg(
                "The Amulet's pulse echoes through the halls. Things are hunting you...",
                Rgb(255, 120, 80),
            );
            self.emit(Sfx::Rumble, sx, sy, 1.0);
        }
        if self.amulet_rage() {
            self.rouse_all();
        }
    }

    /// The Amulet of GRAZZARK: every hostile monster on the floor wakes and
    /// comes for the bearer.
    fn rouse_all(&mut self) {
        let (px, py) = (self.p.x, self.p.y);
        for m in self.level.monsters.iter_mut() {
            if m.alive && m.faction != Faction::Ally {
                m.awake = true;
                m.target = Some(Target::Player);
                m.last_known = Some((px, py));
            }
        }
    }

    /// A message when you step onto a floor its unique monster walks.
    fn announce_unique(&mut self) {
        if !self.level.has_unique {
            return;
        }
        let kind = M_UNIQUE0 + self.depth as usize - 1;
        if self
            .level
            .monsters
            .iter()
            .any(|m| m.alive && m.kind == kind)
        {
            self.msg(
                format!(
                    "A dreadful presence haunts this floor: {}.",
                    MONSTERS[kind].name
                ),
                Rgb(210, 140, 255),
            );
        }
    }

    // ------------------------------------------------------------ monsters

    fn monster_act(&mut self, mi: usize) {
        let kind = self.level.monsters[mi].kind;
        let def = &MONSTERS[kind];
        {
            let m = &mut self.level.monsters[mi];
            m.next_act += m.speed();
            if m.stun > 0 {
                m.stun -= 1;
                return;
            }
        }
        let (mx, my, fac) = {
            let m = &self.level.monsters[mi];
            (m.x, m.y, m.faction)
        };
        let (px, py) = (self.p.x, self.p.y);
        let pd = cheb(mx, my, px, py);
        let hunts_player = hostile(fac, Faction::Player);
        // The Amulet of GRAZZARK: hostile monsters never sleep, always know
        // where the bearer is, and ignore everything else.
        let amulet = hunts_player && self.amulet_rage();
        if amulet {
            self.level.monsters[mi].awake = true;
        }
        let awake = self.level.monsters[mi].awake;
        let in_range = dist(mx, my, px, py) <= def.sight as f32 && (self.p.invis == 0 || pd <= 1);
        let los_player = self.level.los(mx, my, px, py);
        // Ghosts sense the living through solid stone once they are hunting.
        let sees_player =
            amulet || (hunts_player && in_range && (los_player || (def.ghost && awake)));

        if !awake {
            if hunts_player && in_range && los_player {
                let stealth = self.stealth();
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
                        let who = self.level.monsters[mi].the();
                        self.msg(format!("{} notices you!", cap(&who)), Rgb(255, 170, 90));
                    }
                }
            } else if self.rng.gen_bool(0.003) {
                self.emit(Sfx::Voice(def.voice, Mood::Idle), mx, my, 0.5);
            }
            return;
        }

        if self.level.monsters[mi].fear > 0 && self.level.monsters[mi].fearless() {
            self.level.monsters[mi].fear = 0;
        }
        if self.level.monsters[mi].fear > 0 {
            self.level.monsters[mi].fear -= 1;
            self.monster_flee(mi);
            return;
        }

        // Choose a target: the player, or any hostile monster in view.
        let mut tgt = self.level.monsters[mi].target;
        if amulet {
            tgt = Some(Target::Player);
        } else {
            tgt = match tgt {
                Some(Target::Player) => {
                    if sees_player {
                        tgt
                    } else {
                        None
                    }
                }
                Some(Target::Mon(id)) => match self.mon_index(id) {
                    Some(oi)
                        if cheb(self.level.monsters[oi].x, self.level.monsters[oi].y, mx, my)
                            <= 10 =>
                    {
                        tgt
                    }
                    _ => None,
                },
                None => None,
            };
            if tgt.is_none() || self.rng.gen_bool(0.15) {
                let mut best: Option<(Target, i32)> = if sees_player {
                    Some((Target::Player, pd))
                } else {
                    None
                };
                let can_hit_boss = self.may_harm_boss(mi);
                for (oi, o) in self.level.monsters.iter().enumerate() {
                    if oi == mi
                        || !o.alive
                        || !hostile(fac, o.faction)
                        || (o.def().boss && !can_hit_boss)
                    {
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
            // The Warden guards the Amulet and nothing else: it only ever
            // hunts you, and no monster but your thralls will touch it.
            if def.boss {
                tgt = if sees_player {
                    Some(Target::Player)
                } else {
                    None
                };
            }
            if let Some(Target::Mon(id)) = tgt {
                if self
                    .mon_index(id)
                    .map_or(false, |oi| self.level.monsters[oi].def().boss)
                    && !self.may_harm_boss(mi)
                {
                    tgt = None;
                }
            }
            // Champions cannot abide a rival: when two meet, they fight at once.
            if self.level.monsters[mi].champion && !def.boss {
                let fighting_rival = match tgt {
                    Some(Target::Mon(id)) => self
                        .mon_index(id)
                        .map_or(false, |oi| self.level.monsters[oi].champion),
                    _ => false,
                };
                if !fighting_rival {
                    if let Some(rid) = self.rival_in_view(mi) {
                        tgt = Some(Target::Mon(rid));
                        if self.level.is_visible(mx, my) && self.rng.gen_bool(0.5) {
                            let who = self.level.monsters[mi].the();
                            self.msg(
                                format!("{} spots a rival champion and charges!", cap(&who)),
                                Rgb(255, 170, 70),
                            );
                        }
                    }
                }
            }
        }
        self.level.monsters[mi].target = tgt;

        // An overlord thrall's own powers.
        if let Some(Target::Mon(_)) = tgt {
            if self.level.monsters[mi].thrall
                && self.level.monsters[mi].overlord
                && self.thrall_power(mi)
            {
                return;
            }
        }

        // Unique powers.
        if let Some(t) = tgt {
            if !def.abil.is_empty() && self.unique_power(mi, t) {
                return;
            }
        }
        if def.abil.contains(&MAbil::FireAura) {
            self.fire_aura(mi);
            if self.over.is_some() || !self.level.monsters[mi].alive {
                return;
            }
        }

        match tgt {
            Some(t) => {
                let (tx, ty) = self.target_pos(t);
                if t == Target::Player {
                    self.level.monsters[mi].last_known = Some((tx, ty));
                }
                let d = cheb(mx, my, tx, ty);
                if d == 1 && (def.ghost || self.level.can_step(mx, my, tx - mx, ty - my)) {
                    self.monster_melee(mi, t);
                    return;
                }
                if let Some(r) = &def.ranged {
                    if d > 1
                        && d <= r.range
                        && self.level.los(mx, my, tx, ty)
                        && self.rng.gen::<f32>() < r.chance
                    {
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
                        // Uniques prowl their halls; others mostly loiter.
                        let roam = if def.unique > 0 { 0.7 } else { 0.3 };
                        if self.rng.gen_bool(roam) {
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

    /// Another champion that champion `mi` can see.
    fn rival_in_view(&self, mi: usize) -> Option<u32> {
        let m = &self.level.monsters[mi];
        let sight = m.def().sight as f32;
        self.level
            .monsters
            .iter()
            .enumerate()
            .filter(|(oi, o)| {
                *oi != mi
                    && o.alive
                    && o.champion
                    && !(o.faction == Faction::Ally && m.faction == Faction::Ally)
                    && dist(m.x, m.y, o.x, o.y) <= sight
                    && self.level.los(m.x, m.y, o.x, o.y)
            })
            .min_by_key(|(_, o)| cheb(m.x, m.y, o.x, o.y))
            .map(|(_, o)| o.id)
    }

    /// Try a unique monster's special power against target `t`. Returns true
    /// if it used its action.
    fn unique_power(&mut self, mi: usize, t: Target) -> bool {
        let (kind, mx, my, fac, cd) = {
            let m = &self.level.monsters[mi];
            (m.kind, m.x, m.y, m.faction, m.abil_cd)
        };
        if cd > self.time {
            return false;
        }
        let def = &MONSTERS[kind];
        let (tx, ty) = self.target_pos(t);
        let d = cheb(mx, my, tx, ty);
        let vis = self.level.is_visible(mx, my);
        let name = cap(&self.level.monsters[mi].the());
        let weak = self.level.monsters[mi].ward_weaken;
        for &ab in def.abil {
            // Broken wards strip the Warden of its powers one by one.
            if def.boss
                && (weak >= 4
                    || (weak >= 2 && matches!(ab, MAbil::Summon(..)))
                    || (weak >= 3 && ab == MAbil::Quake))
            {
                continue;
            }
            match ab {
                MAbil::Summon(k, n) => {
                    let minions = self
                        .level
                        .monsters
                        .iter()
                        .filter(|m| m.alive && m.summoned && m.faction == fac && m.kind == k)
                        .count();
                    if minions >= 6 || !self.rng.gen_bool(0.3) {
                        continue;
                    }
                    let mut made = 0;
                    for _ in 0..n {
                        if let Some((sx, sy)) = self.free_adjacent(mx, my) {
                            let id = self.new_id();
                            let mut m = Monster::new(id, k, sx, sy, self.time + 60.0);
                            m.faction = fac;
                            m.summoned = true;
                            m.awake = true;
                            if fac != Faction::Ally {
                                m.scale_hp(self.diff().hp);
                            }
                            m.target = Some(t);
                            self.level.monsters.push(m);
                            self.fx
                                .mist(sx as f32 + 0.5, sy as f32 + 0.5, Rgb(160, 120, 200));
                            made += 1;
                        }
                    }
                    if made > 0 {
                        self.emit(Sfx::Voice(def.voice, Mood::Alert), mx, my, 1.0);
                        if vis {
                            self.msg(
                                format!("{} calls its brood to its side!", name),
                                Rgb(220, 150, 255),
                            );
                        }
                        self.level.monsters[mi].abil_cd = self.time + 900.0;
                        return true;
                    }
                }
                MAbil::Raise => {
                    let mut raised = 0;
                    let mut i = 0;
                    while i < self.level.corpses.len() && raised < 2 {
                        let c = &self.level.corpses[i];
                        if cheb(c.x, c.y, mx, my) <= 6
                            && self.level.monster_at(c.x, c.y).is_none()
                            && !(c.x == self.p.x && c.y == self.p.y)
                        {
                            let c = self.level.corpses.remove(i);
                            let id = self.new_id();
                            let mut m = Monster::new(id, c.kind, c.x, c.y, self.time + 80.0);
                            m.faction = fac;
                            m.awake = true;
                            if fac != Faction::Ally {
                                m.scale_hp(self.diff().hp);
                            }
                            m.target = Some(t);
                            self.level.monsters.push(m);
                            self.fx
                                .mist(c.x as f32 + 0.5, c.y as f32 + 0.5, Rgb(200, 120, 255));
                            self.emit(Sfx::Revive, c.x, c.y, 1.0);
                            raised += 1;
                        } else {
                            i += 1;
                        }
                    }
                    if raised > 0 {
                        if vis {
                            self.msg(
                                format!("{} raises the dead to serve it!", name),
                                Rgb(200, 140, 255),
                            );
                        }
                        self.level.monsters[mi].abil_cd = self.time + 700.0;
                        return true;
                    }
                }
                MAbil::Blink | MAbil::Charge | MAbil::Pounce => {
                    let pounce = ab == MAbil::Pounce;
                    let charge = ab == MAbil::Charge || pounce;
                    let ok_range = if pounce {
                        (2..=4).contains(&d)
                    } else if charge {
                        (2..=6).contains(&d)
                    } else {
                        (3..=8).contains(&d)
                    };
                    if !ok_range || !self.level.los(mx, my, tx, ty) || !self.rng.gen_bool(0.35) {
                        continue;
                    }
                    // Land on the free tile next to the target that is closest
                    // to us. Seeing the prey is not enough: the way there must
                    // be open, so a closed door or gate (whose bars you can
                    // see through) has to be opened first.
                    let ghost = self.level.monsters[mi].def().ghost;
                    let mut best: Option<(i32, i32, i32)> = None;
                    for &(dx, dy) in DIRS.iter() {
                        let (nx, ny) = (tx + dx, ty + dy);
                        if self.level.tile(nx, ny).walkable()
                            && self.level.monster_at(nx, ny).is_none()
                            && !(nx == self.p.x && ny == self.p.y)
                            && self.level.can_step(nx, ny, -dx, -dy)
                            && (ghost || self.level.clear_walk(mx, my, nx, ny))
                        {
                            let sc = cheb(nx, ny, mx, my);
                            if best.map_or(true, |b| sc < b.2) {
                                best = Some((nx, ny, sc));
                            }
                        }
                    }
                    let Some((nx, ny, _)) = best else { continue };
                    let (ofx, ofy) = (mx as f32 + 0.5, my as f32 + 0.5);
                    if charge {
                        self.fx.burst(
                            ofx,
                            ofy,
                            0.1,
                            14,
                            Rgb(150, 130, 110),
                            &['.', ',', 'o'],
                            0.5,
                            1.5,
                            -0.2,
                            false,
                        );
                        self.emit(Sfx::Stomp, mx, my, 0.9);
                        self.fx.shake = self.fx.shake.max(0.6);
                    } else {
                        self.fx
                            .rise(ofx, ofy, 18, Rgb(160, 200, 255), &['*', '.', '+'], 0.7);
                        self.emit(Sfx::Blink, mx, my, 0.8);
                    }
                    {
                        let m = &mut self.level.monsters[mi];
                        m.x = nx;
                        m.y = ny;
                        if !charge {
                            m.rx = nx as f32;
                            m.ry = ny as f32;
                        }
                        m.abil_cd = self.time + if charge { 500.0 } else { 600.0 };
                    }
                    if vis || self.level.is_visible(nx, ny) {
                        let txt = if pounce {
                            format!("{} pounces!", name)
                        } else if charge {
                            format!("{} CHARGES!", name)
                        } else {
                            format!("{} vanishes and reappears beside you!", name)
                        };
                        let txt = if t == Target::Player {
                            txt
                        } else {
                            format!("{} lunges across the room!", name)
                        };
                        self.msg(txt, Rgb(255, 150, 120));
                    }
                    if pounce {
                        self.monster_melee(mi, t);
                    } else if charge {
                        // The charge carries extra force into the blow, and hurls the target back.
                        let before = self.target_hp(t);
                        self.level.monsters[mi].charging = true;
                        self.monster_melee(mi, t);
                        if mi < self.level.monsters.len() {
                            self.level.monsters[mi].charging = false;
                        }
                        if self.over.is_none() && self.target_hp(t) < before {
                            let n = self.rng.gen_range(1..=2);
                            self.knock_back(mi, t, n);
                        }
                    }
                    return true;
                }
                MAbil::Rally => {
                    if fac == Faction::Ally || !self.rng.gen_bool(0.35) {
                        continue;
                    }
                    let mut n = 0;
                    for (oi, o) in self.level.monsters.iter_mut().enumerate() {
                        if oi != mi
                            && o.alive
                            && o.faction == fac
                            && cheb(o.x, o.y, mx, my) <= 10
                            && (!o.awake || o.target.is_none())
                        {
                            o.awake = true;
                            o.target = Some(t);
                            o.last_known = Some((tx, ty));
                            n += 1;
                        }
                    }
                    if n == 0 {
                        continue;
                    }
                    self.emit(Sfx::Voice(def.voice, Mood::Alert), mx, my, 1.0);
                    if vis || cheb(mx, my, self.p.x, self.p.y) <= 12 {
                        self.msg(
                            format!("{} calls out, and its kin answer!", name),
                            Rgb(255, 170, 90),
                        );
                    }
                    self.level.monsters[mi].abil_cd = self.time + 1500.0;
                    return true;
                }
                MAbil::Web => {
                    if !(2..=5).contains(&d)
                        || !self.level.los(mx, my, tx, ty)
                        || !self.rng.gen_bool(0.35)
                    {
                        continue;
                    }
                    self.fx.bolt(
                        mx as f32 + 0.5,
                        my as f32 + 0.5,
                        tx as f32 + 0.5,
                        ty as f32 + 0.5,
                        12.0,
                        '#',
                        Rgb(230, 230, 230),
                        None,
                        BoltEnd::None,
                    );
                    self.emit(Sfx::Arrow, mx, my, 0.6);
                    let turns = self.rng.gen_range(2..=3);
                    self.root(t, turns);
                    if t == Target::Player {
                        self.msg(
                            format!("{} spits a sticky web over you. You are stuck fast!", name),
                            Rgb(230, 230, 200),
                        );
                    }
                    self.level.monsters[mi].abil_cd = self.time + 800.0;
                    return true;
                }
                MAbil::Grab => {
                    if d != 1
                        || !self.level.can_step(mx, my, tx - mx, ty - my)
                        || !self.rng.gen_bool(0.3)
                    {
                        continue;
                    }
                    self.root(t, 2);
                    if t == Target::Player {
                        self.msg(
                            format!("{} seizes you in a cold, crushing grip!", name),
                            Rgb(200, 220, 160),
                        );
                    }
                    self.level.monsters[mi].lunge = ((tx - mx) as f32, (ty - my) as f32, 1.0);
                    self.level.monsters[mi].abil_cd = self.time + 700.0;
                    return true;
                }
                MAbil::Guard => {
                    let m = &self.level.monsters[mi];
                    if m.guard > 0 || d > 3 || !self.rng.gen_bool(0.3) {
                        continue;
                    }
                    self.level.monsters[mi].guard = 5;
                    self.emit(Sfx::Clang, mx, my, 0.6);
                    if vis {
                        let txt = if def.name == "gargoyle" {
                            format!("{}'s stone hide hardens!", name)
                        } else {
                            format!("{} raises its guard!", name)
                        };
                        self.msg(txt, Rgb(190, 190, 210));
                        self.fx.ring(
                            mx as f32 + 0.5,
                            my as f32 + 0.5,
                            12,
                            0.8,
                            Rgb(180, 180, 220),
                            &['+', '.'],
                            0.5,
                        );
                    }
                    self.level.monsters[mi].abil_cd = self.time + 1200.0;
                    return true;
                }
                MAbil::Flurry => {
                    if d != 1
                        || !self.level.can_step(mx, my, tx - mx, ty - my)
                        || !self.rng.gen_bool(0.35)
                    {
                        continue;
                    }
                    if vis {
                        self.msg(format!("{} strikes in a frenzy!", name), Rgb(255, 160, 100));
                    }
                    self.level.monsters[mi].abil_cd = self.time + 500.0;
                    for _ in 0..2 {
                        if self.over.is_some()
                            || !self.level.monsters[mi].alive
                            || self.target_hp(t) <= 0
                        {
                            break;
                        }
                        self.monster_melee(mi, t);
                    }
                    return true;
                }
                MAbil::Blind => {
                    if d > 4 || !self.level.los(mx, my, tx, ty) || !self.rng.gen_bool(0.3) {
                        continue;
                    }
                    match t {
                        Target::Player => {
                            self.p.blind = self.p.blind.max(self.rng.gen_range(3..=4));
                            let txt = if def.faction == Faction::Dark {
                                format!("{} conjures a globe of darkness around you! You can barely see!", name)
                            } else {
                                format!("{} flings grit into your eyes! You can barely see!", name)
                            };
                            self.msg(txt, Rgb(200, 180, 140));
                            self.fx.flash(Rgb(40, 30, 20), 0.5);
                            self.refresh_view();
                        }
                        Target::Mon(_) => {
                            self.stagger(t, 100.0);
                        }
                    }
                    self.level.monsters[mi].abil_cd = self.time + 1000.0;
                    return true;
                }
                MAbil::LifeSap => {
                    if d > 5 || !self.level.los(mx, my, tx, ty) || !self.rng.gen_bool(0.3) {
                        continue;
                    }
                    let dmg = self.mon_damage(mi, (2, 5)) + self.depth as i32 / 3;
                    self.fx.bolt(
                        tx as f32 + 0.5,
                        ty as f32 + 0.5,
                        mx as f32 + 0.5,
                        my as f32 + 0.5,
                        9.0,
                        'o',
                        Rgb(170, 255, 200),
                        Some([0.4, 1.2, 0.8]),
                        BoltEnd::None,
                    );
                    self.emit(Sfx::Whisper, mx, my, 0.9);
                    {
                        let m = &mut self.level.monsters[mi];
                        m.hp = (m.hp + dmg).min(m.max_hp);
                    }
                    if t == Target::Player {
                        self.msg(
                            format!(
                                "{} saps your life! ({} damage, and it grows stronger)",
                                name, dmg
                            ),
                            Rgb(170, 255, 200),
                        );
                    }
                    self.level.monsters[mi].abil_cd = self.time + 700.0;
                    self.hurt(mi, t, dmg, false);
                    return true;
                }
                MAbil::Quake => {
                    if d > 2 || !self.rng.gen_bool(0.3) {
                        continue;
                    }
                    let mid = self.level.monsters[mi].id;
                    self.level.monsters[mi].abil_cd = self.time + 900.0;
                    self.fx.ring(
                        mx as f32 + 0.5,
                        my as f32 + 0.5,
                        40,
                        2.5,
                        Rgb(150, 130, 100),
                        &['.', ',', 'o', '#'],
                        0.7,
                    );
                    self.fx.shake = self.fx.shake.max(1.0);
                    self.emit(Sfx::Stomp, mx, my, 1.0);
                    self.emit(Sfx::Rumble, mx, my, 0.8);
                    if vis || cheb(mx, my, self.p.x, self.p.y) <= 8 {
                        self.msg(
                            format!("{} slams the ground! The floor bucks and heaves!", name),
                            Rgb(220, 180, 120),
                        );
                    }
                    let victims: Vec<u32> = self
                        .level
                        .monsters
                        .iter()
                        .enumerate()
                        .filter(|(oi, o)| {
                            *oi != mi
                                && o.alive
                                && cheb(o.x, o.y, mx, my) <= 2
                                && !(fac == Faction::Ally && o.faction == Faction::Ally)
                                && !(def.boss && o.faction != Faction::Ally)
                        })
                        .map(|(_, o)| o.id)
                        .collect();
                    if fac != Faction::Ally && cheb(self.p.x, self.p.y, mx, my) <= 2 {
                        let dmg = self.mon_damage(mi, (2, 6));
                        self.msg(
                            format!("The shockwave hits you for {}!", dmg),
                            Rgb(255, 90, 90),
                        );
                        self.player_hit(dmg, DeathCause::Monster(kind), true);
                        if self.over.is_none() {
                            self.stagger(Target::Player, 100.0);
                        }
                    }
                    for id in victims {
                        if self.over.is_some() {
                            break;
                        }
                        if let Some(oi) = self.mon_index(id) {
                            let dmg = self.mon_damage(mi, (2, 6));
                            self.level.monsters[oi].stun = self.level.monsters[oi].stun.max(1);
                            self.damage_monster(oi, dmg, Src::Mon(mid));
                        }
                    }
                    return true;
                }
                MAbil::Throw => {
                    if !(2..=7).contains(&d)
                        || !self.level.los(mx, my, tx, ty)
                        || !self.rng.gen_bool(0.3)
                    {
                        continue;
                    }
                    let mid = self.level.monsters[mi].id;
                    let (hx, hy, hit) = self.trace(mx, my, tx, ty, d + 2, Some(mid));
                    self.fx.bolt(
                        mx as f32 + 0.5,
                        my as f32 + 0.5,
                        hx as f32 + 0.5,
                        hy as f32 + 0.5,
                        9.0,
                        'O',
                        Rgb(150, 140, 130),
                        None,
                        BoltEnd::Sparks(Rgb(150, 140, 130)),
                    );
                    self.emit(Sfx::Stomp, hx, hy, 0.8);
                    if vis {
                        self.msg(format!("{} hurls a boulder!", name), Rgb(220, 180, 140));
                    }
                    self.level.monsters[mi].abil_cd = self.time + 700.0;
                    match hit {
                        Some(Target::Player) => {
                            let roll = self.rng.gen_range(1..=20);
                            if roll == 20 || (roll != 1 && roll + def.hit >= 10 + self.player_ac())
                            {
                                let dmg = self.mon_damage(mi, (4, 10));
                                self.msg(
                                    format!("The boulder smashes into you for {}!", dmg),
                                    Rgb(255, 90, 90),
                                );
                                self.player_hit(dmg, DeathCause::Monster(kind), true);
                            } else {
                                self.msg("The boulder thunders past you.", GREY);
                            }
                        }
                        Some(Target::Mon(oid)) => {
                            if let Some(oi) = self.mon_index(oid) {
                                if self.rng.gen_bool(0.75) {
                                    let dmg = self.mon_damage(mi, (4, 10));
                                    self.damage_monster(oi, dmg, Src::Mon(mid));
                                }
                            }
                        }
                        None => {}
                    }
                    return true;
                }
                MAbil::Spores => {
                    if d > 3 || !self.rng.gen_bool(0.35) {
                        continue;
                    }
                    let until = self.time + 500.0;
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let (cx, cy) = (mx + dx, my + dy);
                            if self.level.tile(cx, cy).walkable() && !self.level.cloud_at(cx, cy) {
                                self.level.clouds.push(Cloud {
                                    x: cx,
                                    y: cy,
                                    until,
                                });
                            }
                        }
                    }
                    self.fx.burst(
                        mx as f32 + 0.5,
                        my as f32 + 0.5,
                        0.4,
                        30,
                        Rgb(220, 160, 210),
                        &['*', '.', 'o', ':'],
                        1.2,
                        1.4,
                        -0.1,
                        false,
                    );
                    self.emit(Sfx::Whisper, mx, my, 0.8);
                    if vis {
                        self.msg(
                            format!("{} bursts, releasing a cloud of choking spores!", name),
                            Rgb(230, 160, 220),
                        );
                    }
                    self.level.monsters[mi].abil_cd = self.time + 800.0;
                    if fac != Faction::Ally && cheb(self.p.x, self.p.y, mx, my) <= 1 {
                        self.spore_player();
                    }
                    return true;
                }
                MAbil::Split => {
                    let (hp, max_hp) = (self.level.monsters[mi].hp, self.level.monsters[mi].max_hp);
                    let kin = self
                        .level
                        .monsters
                        .iter()
                        .filter(|o| o.alive && o.kind == kind)
                        .count();
                    if hp < 8 || hp * 10 >= max_hp * 8 || kin >= 10 || !self.rng.gen_bool(0.35) {
                        continue;
                    }
                    let Some((sx, sy)) = self.free_adjacent(mx, my) else {
                        continue;
                    };
                    let half = hp / 2;
                    let id = self.new_id();
                    let mut m = Monster::new(id, kind, sx, sy, self.time + 60.0);
                    m.faction = fac;
                    m.summoned = true;
                    m.awake = true;
                    m.target = Some(t);
                    m.hp = half;
                    m.max_hp = half;
                    self.level.monsters.push(m);
                    {
                        let me = &mut self.level.monsters[mi];
                        me.hp -= half;
                        me.max_hp = (me.max_hp - half).max(me.hp);
                        me.abil_cd = self.time + 600.0;
                    }
                    self.fx.burst(
                        mx as f32 + 0.5,
                        my as f32 + 0.5,
                        0.2,
                        14,
                        def.color,
                        &['o', '.', '~'],
                        0.8,
                        0.8,
                        0.3,
                        false,
                    );
                    if vis {
                        self.msg(
                            format!("{} quivers and splits in two!", name),
                            Rgb(150, 220, 120),
                        );
                    }
                    return true;
                }
                MAbil::Mend => {
                    if !self.rng.gen_bool(0.4) {
                        continue;
                    }
                    let pick = (0..self.level.monsters.len()).find(|&j| {
                        let o = &self.level.monsters[j];
                        o.alive
                            && o.faction == fac
                            && o.hp * 10 < o.max_hp * 7
                            && cheb(o.x, o.y, mx, my) <= 6
                            && self.level.los(mx, my, o.x, o.y)
                    });
                    let Some(j) = pick else { continue };
                    let heal = 6 + self.depth as i32;
                    let (ox, oy) = (self.level.monsters[j].x, self.level.monsters[j].y);
                    {
                        let o = &mut self.level.monsters[j];
                        o.hp = (o.hp + heal).min(o.max_hp);
                    }
                    self.fx.heal(ox as f32 + 0.5, oy as f32 + 0.5);
                    self.emit(Sfx::Heal, mx, my, 0.7);
                    if vis || self.level.is_visible(ox, oy) {
                        let who = if j == mi {
                            "its own".to_string()
                        } else {
                            format!("{}'s", self.level.monsters[j].the())
                        };
                        self.msg(
                            format!("{} chants, and {} wounds close!", name, who),
                            Rgb(150, 230, 150),
                        );
                    }
                    self.level.monsters[mi].abil_cd = self.time + 600.0;
                    return true;
                }
                MAbil::Bash => {
                    if d != 1
                        || !self.level.can_step(mx, my, tx - mx, ty - my)
                        || !self.rng.gen_bool(0.3)
                    {
                        continue;
                    }
                    let before = self.target_hp(t);
                    self.level.monsters[mi].abil_cd = self.time + 600.0;
                    self.monster_melee(mi, t);
                    if self.over.is_none() && self.target_hp(t) < before && self.stagger(t, 100.0) {
                        if t == Target::Player {
                            self.msg(
                                format!(
                                    "{} slams its shield into you, knocking you off balance!",
                                    name
                                ),
                                Rgb(230, 190, 120),
                            );
                        }
                        self.emit(Sfx::Clang, tx, ty, 0.8);
                    }
                    return true;
                }
                MAbil::Gaze => {
                    if d > 5 || !self.level.los(mx, my, tx, ty) || !self.rng.gen_bool(0.25) {
                        continue;
                    }
                    self.level.monsters[mi].abil_cd = self.time + 1000.0;
                    if self.stagger(t, 200.0) {
                        if t == Target::Player {
                            self.msg(
                                format!("{} meets your eyes... your limbs turn to stone!", name),
                                Rgb(200, 255, 120),
                            );
                            self.fx.flash(Rgb(150, 160, 120), 0.4);
                        }
                        self.fx.bolt(
                            mx as f32 + 0.5,
                            my as f32 + 0.5,
                            tx as f32 + 0.5,
                            ty as f32 + 0.5,
                            20.0,
                            '*',
                            Rgb(220, 255, 100),
                            Some([1.0, 1.2, 0.3]),
                            BoltEnd::None,
                        );
                        self.emit(Sfx::Frost, tx, ty, 0.7);
                        return true;
                    }
                }
                MAbil::Shriek => {
                    if d > 5 || !self.rng.gen_bool(0.25) {
                        continue;
                    }
                    self.level.monsters[mi].abil_cd = self.time + 900.0;
                    self.emit(Sfx::Voice(def.voice, Mood::Alert), mx, my, 1.0);
                    self.fx.ring(
                        mx as f32 + 0.5,
                        my as f32 + 0.5,
                        30,
                        3.0,
                        Rgb(180, 230, 255),
                        &['~', ')', '('],
                        0.6,
                    );
                    if vis {
                        self.msg(
                            format!(
                                "{} lets out a keening shriek that freezes your blood!",
                                name
                            ),
                            Rgb(180, 230, 255),
                        );
                    }
                    let dmg = self.mon_damage(mi, (2, 5));
                    self.hurt(mi, t, dmg, false);
                    if self.over.is_none() {
                        self.stagger(t, 100.0);
                    }
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Current health of a target (0 if it is gone).
    fn target_hp(&self, t: Target) -> i32 {
        match t {
            Target::Player => self.p.hp + self.p.ward,
            Target::Mon(id) => self.mon_index(id).map_or(0, |i| self.level.monsters[i].hp),
        }
    }

    /// Damage a target with a monster's special power.
    fn hurt(&mut self, mi: usize, t: Target, dmg: i32, crush: bool) {
        let kind = self.level.monsters[mi].kind;
        let mid = self.level.monsters[mi].id;
        match t {
            Target::Player => self.player_hit(dmg, DeathCause::Spell(kind), crush),
            Target::Mon(id) => {
                if let Some(oi) = self.mon_index(id) {
                    let (ox, oy) = (self.level.monsters[oi].x, self.level.monsters[oi].y);
                    self.fx.text(
                        ox as f32 + 0.5,
                        oy as f32 + 0.5,
                        0.9,
                        dmg.to_string(),
                        Rgb(255, 160, 80),
                    );
                    self.damage_monster(oi, dmg, Src::Mon(mid));
                }
            }
        }
    }

    /// Stagger a target for `ticks`. The player gets a grace period afterwards
    /// so paralysis can never chain into a lock; a monster is stunned.
    pub fn stagger(&mut self, t: Target, ticks: f64) -> bool {
        match t {
            Target::Player => {
                if self.time < self.p.para_guard {
                    return false;
                }
                self.p.para_guard = self.time + ticks + 300.0;
                self.p.slow += ticks;
                if self.realtime {
                    self.p.next_act = self.p.next_act.max(self.time) + ticks;
                    self.p.slow = 0.0;
                }
                true
            }
            Target::Mon(id) => match self.mon_index(id) {
                Some(i) => {
                    let m = &mut self.level.monsters[i];
                    m.stun = m.stun.max((ticks / 100.0).ceil() as i32);
                    true
                }
                None => false,
            },
        }
    }

    /// Hold a target in place: the player cannot step away, a monster is stunned.
    fn root(&mut self, t: Target, turns: i32) {
        match t {
            Target::Player => self.p.rooted = self.p.rooted.max(turns),
            Target::Mon(id) => {
                if let Some(i) = self.mon_index(id) {
                    let m = &mut self.level.monsters[i];
                    m.stun = m.stun.max(turns - 1);
                }
            }
        }
    }

    /// Hurl a target 1-2 tiles away from monster `mi`; walls hurt.
    fn knock_back(&mut self, mi: usize, t: Target, n: i32) {
        let (mx, my, kind, mid) = {
            let m = &self.level.monsters[mi];
            (m.x, m.y, m.kind, m.id)
        };
        let (tx, ty) = self.target_pos(t);
        let (dx, dy) = ((tx - mx).signum(), (ty - my).signum());
        if (dx, dy) == (0, 0) {
            return;
        }
        let mut moved = false;
        for _ in 0..n {
            let (cx, cy) = self.target_pos(t);
            let (nx, ny) = (cx + dx, cy + dy);
            let blocked = !self.level.can_step(cx, cy, dx, dy)
                || !self.level.tile(nx, ny).walkable()
                || self.level.monster_at(nx, ny).is_some()
                || (nx == self.p.x && ny == self.p.y);
            if blocked {
                let dmg = self.rng.gen_range(1..=4);
                match t {
                    Target::Player => {
                        self.msg(
                            format!("You are slammed into the wall for {}!", dmg),
                            Rgb(255, 110, 90),
                        );
                        self.player_hit(dmg, DeathCause::Monster(kind), true);
                    }
                    Target::Mon(id) => {
                        if let Some(oi) = self.mon_index(id) {
                            self.damage_monster(oi, dmg, Src::Mon(mid));
                        }
                    }
                }
                break;
            }
            match t {
                Target::Player => {
                    self.p.x = nx;
                    self.p.y = ny;
                }
                Target::Mon(id) => {
                    if let Some(oi) = self.mon_index(id) {
                        self.level.monsters[oi].x = nx;
                        self.level.monsters[oi].y = ny;
                    }
                }
            }
            moved = true;
        }
        if moved && t == Target::Player && self.over.is_none() {
            self.msg("You are hurled backwards!", Rgb(255, 150, 120));
            self.fx.shake = self.fx.shake.max(0.8);
            self.after_player_move();
        }
    }

    /// Spores take hold of the player: a short paralysis.
    fn spore_player(&mut self) {
        if self.stagger(Target::Player, 150.0) {
            self.msg(
                "You breathe in the spores! Your muscles seize up!",
                Rgb(230, 160, 220),
            );
            self.fx.flash(Rgb(160, 90, 150), 0.3);
        }
    }

    /// Spore clouds drift away in time; anything caught in one is seized.
    fn tick_clouds(&mut self) {
        let now = self.time;
        self.level.clouds.retain(|c| c.until > now);
        if self.level.clouds.is_empty() {
            return;
        }
        let (px, py) = (self.p.x, self.p.y);
        if self.level.cloud_at(px, py) {
            self.spore_player();
        }
        for i in 0..self.level.monsters.len() {
            let (x, y, alive, immune) = {
                let m = &self.level.monsters[i];
                (
                    m.x,
                    m.y,
                    m.alive,
                    m.def().abil.contains(&MAbil::Spores) || m.def().death == DeathKind::Undead,
                )
            };
            if alive && !immune && self.level.cloud_at(x, y) && self.rng.gen_bool(0.5) {
                let m = &mut self.level.monsters[i];
                m.stun = m.stun.max(1);
            }
        }
    }

    /// Ignar's searing aura burns everything standing next to him.
    fn fire_aura(&mut self, mi: usize) {
        if !self.rng.gen_bool(0.5) {
            return;
        }
        let (mx, my, mid, kind) = {
            let m = &self.level.monsters[mi];
            (m.x, m.y, m.id, m.kind)
        };
        let fx = mx as f32 + 0.5;
        let fy = my as f32 + 0.5;
        let d = self.depth as i32;
        let mut any = false;
        if cheb(mx, my, self.p.x, self.p.y) <= 1 {
            let dmg = ((self.rng.gen_range(2..=5) + d / 2) as f32 * self.diff().dmg).round() as i32;
            let who = self.level.monsters[mi].the();
            self.msg(
                format!("Searing heat from {} scorches you! ({} damage)", who, dmg),
                Rgb(255, 130, 40),
            );
            self.player_hit(dmg, DeathCause::Spell(kind), false);
            any = true;
        }
        let near: Vec<usize> = self
            .level
            .monsters
            .iter()
            .enumerate()
            .filter(|(i, o)| {
                *i != mi
                    && o.alive
                    && o.id != mid
                    && cheb(o.x, o.y, mx, my) <= 1
                    && hostile(self.level.monsters[mi].faction, o.faction)
            })
            .map(|(i, _)| i)
            .collect();
        for oi in near {
            let dmg = self.rng.gen_range(2..=5) + d / 2;
            self.damage_monster(oi, dmg, Src::Mon(mid));
            any = true;
        }
        if any {
            self.fx
                .ring(fx, fy, 16, 1.2, Rgb(255, 120, 30), &['^', '*', '~'], 0.4);
            self.fx.light(fx, fy, [2.0, 0.8, 0.2], 2.0, 0.3);
            self.emit(Sfx::Fireball, mx, my, 0.5);
        }
    }

    /// Ghosts drift through walls (but not the dungeon's outer edge).
    fn ghost_can_enter(&self, x: i32, y: i32) -> bool {
        x >= 1 && y >= 1 && x < self.level.w - 1 && y < self.level.h - 1
    }

    fn monster_free(&self, mx: i32, my: i32, dx: i32, dy: i32) -> bool {
        let (nx, ny) = (mx + dx, my + dy);
        let t = self.level.tile(nx, ny);
        (t.walkable() || t.is_closed_door())
            && self.level.can_step(mx, my, dx, dy)
            && self.level.monster_at(nx, ny).is_none()
            && !(nx == self.p.x && ny == self.p.y)
    }

    /// Like monster_free, but ghosts may enter walls.
    fn monster_free_for(&self, mi: usize, dx: i32, dy: i32) -> bool {
        let m = &self.level.monsters[mi];
        if m.def().ghost {
            let (nx, ny) = (m.x + dx, m.y + dy);
            self.ghost_can_enter(nx, ny)
                && self.level.monster_at(nx, ny).is_none()
                && !(nx == self.p.x && ny == self.p.y)
        } else {
            self.monster_free(m.x, m.y, dx, dy)
        }
    }

    fn monster_wander(&mut self, mi: usize) {
        let (mx, my) = (self.level.monsters[mi].x, self.level.monsters[mi].y);
        let (dx, dy) = DIRS[self.rng.gen_range(0..8)];
        if self.monster_free_for(mi, dx, dy) {
            self.move_monster(mi, mx + dx, my + dy);
        }
    }

    fn monster_flee(&mut self, mi: usize) {
        let (mx, my, ghost) = {
            let m = &self.level.monsters[mi];
            (m.x, m.y, m.def().ghost)
        };
        let (px, py) = (self.p.x, self.p.y);
        // Ghosts flee in a straight line; others follow the corridors.
        let score = |g: &Game, x: i32, y: i32| -> i32 {
            if ghost {
                cheb(x, y, px, py) * 10 + (x - px).abs() + (y - py).abs()
            } else {
                g.dmap[g.level.idx(x, y)]
            }
        };
        let cur = score(self, mx, my);
        let mut best: Option<(i32, i32, i32)> = None;
        for &(dx, dy) in DIRS.iter() {
            if !self.monster_free_for(mi, dx, dy) {
                continue;
            }
            let v = score(self, mx + dx, my + dy);
            if v < UNREACH && v > cur && best.map_or(true, |b| v > b.0) {
                best = Some((v, dx, dy));
            }
        }
        if let Some((_, dx, dy)) = best {
            self.move_monster(mi, mx + dx, my + dy);
        }
    }

    fn move_monster(&mut self, mi: usize, nx: i32, ny: i32) {
        let ghost = self.level.monsters[mi].def().ghost;
        let t = self.level.tile(nx, ny);
        if !ghost && t.is_closed_door() {
            let txt = self.open_door(nx, ny);
            if self.level.is_visible(nx, ny) {
                self.msg(txt.replacen("The ", "A ", 1), GREY);
            }
            return;
        }
        {
            let m = &mut self.level.monsters[mi];
            m.x = nx;
            m.y = ny;
        }
        // Distant, unseen footsteps give you something to listen for.
        if !ghost
            && !self.level.is_visible(nx, ny)
            && self.dmap[self.level.idx(nx, ny)] <= 12
            && self.rng.gen_bool(0.25)
        {
            self.emit(Sfx::Step, nx, ny, 0.45);
        }
    }

    /// Step toward (tx,ty). Uses the player distance map when chasing the
    /// player so monsters path around corners; ghosts go straight through
    /// walls. Returns false if stuck.
    fn monster_step_toward(&mut self, mi: usize, tx: i32, ty: i32, use_dmap: bool) -> bool {
        let (mx, my, fac, ghost) = {
            let m = &self.level.monsters[mi];
            (m.x, m.y, m.faction, m.def().ghost)
        };
        let use_dmap = use_dmap && !ghost;
        let (px, py) = (self.p.x, self.p.y);
        let amulet = hostile(fac, Faction::Player) && self.amulet_rage();
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
            if ghost {
                if !self.ghost_can_enter(nx, ny) {
                    continue;
                }
            } else {
                if !self.level.can_step(mx, my, dx, dy) {
                    continue;
                }
                let t = self.level.tile(nx, ny);
                if !(t.walkable() || t.is_closed_door()) {
                    continue;
                }
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
                // Under the Amulet's spell, monsters only cut through the
                // player's companions; they ignore their usual enemies.
                let oid = self.level.monsters[oi].id;
                let is_target = self.level.monsters[mi].target == Some(Target::Mon(oid));
                if (hostile(fac, ofac) || is_target) && (!amulet || ofac == Faction::Ally) {
                    let oid = self.level.monsters[oi].id;
                    if !amulet {
                        self.level.monsters[mi].target = Some(Target::Mon(oid));
                    }
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

    /// Roll a monster's damage, including champion and rage bonuses.
    fn mon_damage(&mut self, mi: usize, range: (i32, i32)) -> i32 {
        let base = self.rng.gen_range(range.0..=range.1);
        let m = &self.level.monsters[mi];
        let diff = if m.faction == Faction::Ally {
            1.0
        } else {
            self.diff().dmg
        };
        ((base as f32) * m.dmg_mult() * diff).round() as i32
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
        let label = self.level.monsters[mi].the();
        let aname = if ally {
            format!("your {}", self.level.monsters[mi].label())
        } else {
            label
        };
        // A ghost lurking inside the stone.
        let in_wall = def.ghost && self.level.tile(mx, my).opaque();
        match t {
            Target::Player => {
                if in_wall || self.rng.gen_bool(0.5) {
                    self.emit(Sfx::Voice(def.voice, Mood::Attack), mx, my, 0.8);
                } else {
                    self.emit(Sfx::Swing, mx, my, 0.5);
                }
                let roll = self.rng.gen_range(1..=20);
                let ac = 10 + self.player_ac();
                if roll == 20 || (roll != 1 && roll + def.hit >= ac) {
                    let dmg = self.mon_damage(mi, def.dmg);
                    if in_wall {
                        self.msg(
                            format!(
                                "An unseen power hits you from a nearby wall for {} damage!",
                                dmg
                            ),
                            Rgb(170, 200, 255),
                        );
                        self.fx.flash(Rgb(120, 160, 255), 0.35);
                    } else {
                        self.msg(
                            format!("{} hits you for {}!", cap(&aname), dmg),
                            Rgb(255, 90, 90),
                        );
                    }
                    let (px, py) = (self.p.x, self.p.y);
                    self.emit(Sfx::Hit, px, py, 0.8);
                    self.player_hit(dmg, DeathCause::Monster(kind), def.crush);
                    if def.poison && self.over.is_none() && self.rng.gen_bool(0.35) {
                        let n = 4 + self.depth as i32 / 2;
                        if self.add_poison(n, a_name(kind)) {
                            self.msg("You are poisoned!", Rgb(120, 255, 80));
                            self.fx.flash(Rgb(80, 200, 40), 0.3);
                        }
                    }
                    if self.over.is_none() {
                        self.on_hit_powers(mi, dmg);
                    }
                    // Thorned Hide (altar boon).
                    if self.over.is_none()
                        && self.bonus.thorns > 0
                        && !in_wall
                        && self.level.monsters[mi].alive
                    {
                        let th: i32 = (0..self.bonus.thorns)
                            .map(|_| self.rng.gen_range(1..=3))
                            .sum();
                        self.fx.text(
                            mx as f32 + 0.5,
                            my as f32 + 0.5,
                            0.9,
                            th.to_string(),
                            Rgb(170, 220, 110),
                        );
                        self.damage_monster(mi, th, Src::Player);
                    }
                    // Briarmail: thorns tear at anything that strikes you.
                    if self.over.is_none()
                        && self.has(UPower::Thorns)
                        && self.level.monsters[mi].alive
                    {
                        let th = 2 + self.depth as i32 / 2;
                        self.fx.text(
                            mx as f32 + 0.5,
                            my as f32 + 0.5,
                            0.9,
                            th.to_string(),
                            Rgb(140, 200, 90),
                        );
                        self.damage_monster(mi, th, Src::Player);
                    }
                } else if in_wall {
                    self.msg(
                        "Something cold brushes past you from within the wall.",
                        Rgb(150, 160, 200),
                    );
                } else {
                    self.msg(format!("{} misses you.", cap(&aname)), GREY);
                }
            }
            Target::Mon(oid) => {
                let Some(oi) = self.mon_index(oid) else {
                    return;
                };
                let (ox, oy, ofac) = {
                    let o = &self.level.monsters[oi];
                    (o.x, o.y, o.faction)
                };
                let ovis = self.level.is_visible(ox, oy);
                let oname = if ofac == Faction::Ally {
                    format!("your {}", self.level.monsters[oi].label())
                } else {
                    self.level.monsters[oi].the()
                };
                let roll = self.rng.gen_range(1..=20);
                if roll + def.hit >= 10 + self.level.monsters[oi].ac() {
                    let dmg = self.mon_damage(mi, def.dmg);
                    if vis || ovis {
                        self.msg(
                            format!("{} hits {}.", cap(&aname), oname),
                            Rgb(200, 170, 140),
                        );
                    }
                    let mat = self.level.monsters[oi].def().material();
                    self.emit(Sfx::Impact(mat), ox, oy, 0.8);
                    if self.rng.gen_bool(0.4) {
                        self.emit(Sfx::Voice(def.voice, Mood::Attack), mx, my, 0.8);
                    }
                    self.fx.text(
                        ox as f32 + 0.5,
                        oy as f32 + 0.5,
                        0.9,
                        dmg.to_string(),
                        Rgb(255, 160, 80),
                    );
                    if def.abil.contains(&MAbil::Vampiric) {
                        let m = &mut self.level.monsters[mi];
                        m.hp = (m.hp + dmg / 2).min(m.max_hp);
                    }
                    self.damage_monster(oi, dmg, Src::Mon(mid));
                } else if vis || ovis {
                    self.msg(format!("{} misses {}.", cap(&aname), oname), GREY);
                }
                if !(vis || ovis) && self.turns > self.last_fight_msg + 20 {
                    // Out of sight you only hear it: no names, no place, only
                    // a rough direction, and only if it is close enough.
                    if let Some(d) = self.heard_from(mx, my) {
                        self.last_fight_msg = self.turns;
                        let (ax, ay) = self.sound_corner(mx, my);
                        let how = if d * 3 > self.hear_radius() * 2 { "faint " } else { "" };
                        self.msg(
                            format!(
                                "You hear the {}sounds of a fight {}.",
                                how,
                                compass(ax - self.p.x as f32 - 0.5, ay - self.p.y as f32 - 0.5)
                            ),
                            Rgb(180, 160, 140),
                        );
                    }
                }
                self.noise(mx, my, 6);
            }
        }
    }

    /// Unique monster powers that trigger when its blow lands on the player.
    fn on_hit_powers(&mut self, mi: usize, dmg: i32) {
        let def = self.level.monsters[mi].def();
        if def.abil.is_empty() {
            return;
        }
        let name = cap(&self.level.monsters[mi].the());
        if def.abil.contains(&MAbil::Vampiric) {
            let m = &mut self.level.monsters[mi];
            m.hp = (m.hp + dmg / 2 + 1).min(m.max_hp);
            self.msg(format!("{} drinks your blood!", name), Rgb(220, 60, 90));
        }
        if def.abil.contains(&MAbil::FrostTouch) && self.rng.gen_bool(0.35) {
            self.p.slow += 100.0;
            if self.realtime {
                self.p.next_act = self.p.next_act.max(self.time) + 100.0;
                self.p.slow = 0.0;
            }
            let (px, py) = (self.p.x, self.p.y);
            self.fx.ice(px as f32 + 0.5, py as f32 + 0.5);
            self.emit(Sfx::Frost, px, py, 0.8);
            self.msg("A deathly chill freezes you in place!", Rgb(150, 210, 255));
        }
        if def.abil.contains(&MAbil::Paralyze)
            && self.level.monsters[mi].abil_cd <= self.time
            && self.rng.gen_bool(0.3)
        {
            if self.stagger(Target::Player, 150.0) {
                self.level.monsters[mi].abil_cd = self.time + 800.0;
                self.msg(
                    format!("{}'s venom locks your limbs: you are PARALYSED!", name),
                    Rgb(200, 255, 120),
                );
                self.fx.flash(Rgb(150, 220, 80), 0.3);
            }
        }
        if def.abil.contains(&MAbil::ManaDrain) && self.p.mp > 0 {
            let drain = self.rng.gen_range(3..=6).min(self.p.mp);
            self.p.mp -= drain;
            self.msg(
                format!("{} tears {} mana from your soul!", name, drain),
                Rgb(180, 120, 255),
            );
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
        self.fx.bolt(
            mx as f32 + 0.5,
            my as f32 + 0.5,
            hx as f32 + 0.5,
            hy as f32 + 0.5,
            14.0,
            r.ch,
            r.color,
            light,
            end,
        );
        let sfx = match r.element {
            Element::Fire => Sfx::Fireball,
            Element::Magic => Sfx::Bolt,
            Element::Frost => Sfx::Frost,
            Element::Phys => Sfx::Arrow,
        };
        self.emit(sfx, mx, my, 0.8);
        if self.level.is_visible(mx, my) {
            let who = self.level.monsters[mi].the();
            self.msg(format!("{} {}!", cap(&who), r.verb), Rgb(255, 180, 120));
        }
        self.level.monsters[mi].lunge = (
            (tx - mx).signum() as f32 * 0.5,
            (ty - my).signum() as f32 * 0.5,
            0.6,
        );
        let phys = r.element == Element::Phys;
        let flight = (((hx - mx).pow(2) + (hy - my).pow(2)) as f32).sqrt() / 14.0;
        let (sx, sy) = ((tx - mx).signum(), (ty - my).signum());
        if phys && hit.is_none() && self.level.tile(hx + sx, hy + sy).opaque() {
            self.emit_after(Sfx::Impact(Material::Wall), hx, hy, 0.5, flight);
        }
        match hit {
            Some(Target::Player) => {
                let roll = self.rng.gen_range(1..=20);
                if roll == 20 || (roll != 1 && roll + def.hit >= 10 + self.player_ac()) {
                    if phys {
                        let mat = if self.player_ac() >= 7 { Material::Armor } else { Material::Flesh };
                        self.emit_after(Sfx::Impact(mat), hx, hy, 0.9, flight);
                    }
                    let dmg = self.mon_damage(mi, r.dmg);
                    self.msg(format!("You are struck for {}!", dmg), Rgb(255, 90, 90));
                    let cause = if r.element == Element::Phys {
                        DeathCause::Monster(kind)
                    } else {
                        DeathCause::Spell(kind)
                    };
                    self.player_hit(dmg, cause, false);
                } else {
                    self.msg("It misses you.", GREY);
                }
            }
            Some(Target::Mon(oid)) => {
                if let Some(oi) = self.mon_index(oid) {
                    if self.rng.gen_bool(0.75) {
                        let dmg = self.mon_damage(mi, r.dmg);
                        let (ox, oy) = (self.level.monsters[oi].x, self.level.monsters[oi].y);
                        if phys {
                            let mat = self.level.monsters[oi].def().material();
                            self.emit_after(Sfx::Impact(mat), ox, oy, 0.8, flight);
                        }
                        self.fx.text(
                            ox as f32 + 0.5,
                            oy as f32 + 0.5,
                            0.9,
                            dmg.to_string(),
                            Rgb(255, 160, 80),
                        );
                        self.damage_monster(oi, dmg, Src::Mon(mid));
                    }
                }
            }
            None => {}
        }
        self.combat_heat = self
            .combat_heat
            .max(if t == Target::Player { 1.0 } else { 0.3 });
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
        let path = if self.level.inb(tx, ty) {
            self.dmap[self.level.idx(tx, ty)]
        } else {
            UNREACH
        };
        let path_f = if path >= UNREACH {
            straight * 2.5 + 12.0
        } else {
            path as f32
        };

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
        let ear_ang = if self.overhead {
            -PI / 2.0
        } else {
            self.cam.ang
        };
        let rel = (ay - ly).atan2(ax - lx) - ear_ang;
        let dist_here = ((ax - lx).powi(2) + (ay - ly).powi(2)).sqrt();
        let pan = if dist_here < 0.3 {
            0.0
        } else {
            rel.sin() * 0.9
        };
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
        let reverb =
            (0.15 + 0.35 * room + if los { 0.0 } else { 0.25 } + (eff / 30.0).min(0.3)).min(0.95);
        Some(Spatial {
            gain,
            pan,
            cutoff,
            echoes,
            reverb,
            room,
        })
    }

    /// `spatial`, plus what this sound does in the space around you:
    /// footsteps ring back off the far end of a long hallway or a big hall.
    pub fn spatial_for(&self, sfx: Sfx, sx: f32, sy: f32, vol: f32) -> Option<Spatial> {
        let mut sp = self.spatial(sx, sy, vol)?;
        if sfx == Sfx::Step {
            // Slap-back off the far wall: there and back, ~1.5 m a tile.
            let k = ((self.echo_len - 3.0) / 12.0).clamp(0.0, 1.0);
            let open = self.room_size;
            if k > 0.0 {
                let d = (2.0 * self.echo_len * 1.5 / 343.0).clamp(0.03, 0.22);
                let g = 0.15 + 0.35 * k;
                sp.echoes.push((d, g, -sp.pan * 0.5 - 0.3));
                sp.echoes.push((d * 2.0, g * 0.55, sp.pan * 0.5 + 0.3));
                sp.echoes.push((d * 3.0, g * 0.3, -0.2));
            }
            sp.reverb = (sp.reverb + 0.25 * k + 0.2 * open).min(0.95);
            sp.room = (sp.room + 0.4 * k).min(1.0);
        }
        Some(sp)
    }

    /// How far, along the corridors, you can hear a fight or a footstep.
    /// Doors muffle sound (they count as three tiles).
    pub fn hear_radius(&self) -> i32 {
        12 + if self.bonus.hearing >= 1 { 6 } else { 0 }
    }

    /// How far a sound at (x,y) effectively travels to reach you, in tiles,
    /// if you can hear it. Corners and doors muffle it (see `sound_field`).
    pub fn heard_from(&self, x: i32, y: i32) -> Option<i32> {
        if !self.level.inb(x, y) || self.smap.is_empty() {
            return None;
        }
        let d = self.smap[self.level.idx(x, y)];
        if d == SOUND_FAR {
            return None;
        }
        let tiles = (d as i32 + 1) / 2;
        (tiles <= self.hear_radius()).then_some(tiles)
    }

    /// Where a sound at (x,y) seems to come from: itself if you can see it,
    /// otherwise the last corner it bends around on its way to you.
    pub fn sound_corner(&self, x: i32, y: i32) -> (f32, f32) {
        let (px, py) = (self.p.x, self.p.y);
        let (mut cx, mut cy) = (x, y);
        let walkable = |x: i32, y: i32| {
            let t = self.level.tile(x, y);
            t.walkable() || t.is_closed_door()
        };
        if self.level.inb(x, y) && !self.smap.is_empty() && self.smap[self.level.idx(x, y)] != SOUND_FAR {
            for _ in 0..64 {
                if self.level.los(px, py, cx, cy) {
                    break;
                }
                let mut best = self.smap[self.level.idx(cx, cy)];
                let mut next = None;
                for &(dx, dy) in DIRS.iter() {
                    let (nx, ny) = (cx + dx, cy + dy);
                    if self.level.inb(nx, ny)
                        && walkable(nx, ny)
                        && self.level.can_step(cx, cy, dx, dy)
                        && self.smap[self.level.idx(nx, ny)] < best
                    {
                        best = self.smap[self.level.idx(nx, ny)];
                        next = Some((nx, ny));
                    }
                }
                match next {
                    Some(n) => (cx, cy) = n,
                    None => break,
                }
            }
        }
        (cx as f32 + 0.5, cy as f32 + 0.5)
    }

    /// Keen Hearing rank 2: hold still for a turn and listen. Everything that
    /// stirs within earshot (and anything snoring close by) is picked out:
    /// where its sound comes from, and roughly how far.
    fn listen(&mut self) -> Option<f64> {
        if self.bonus.hearing < 2 {
            self.msg("You strain your ears, but hear nothing useful. (Keen Hearing, Soul Altar)", GREY);
            return None;
        }
        if self.time < self.listen_ready {
            let left = ((self.listen_ready - self.time) / 100.0).ceil() as i32;
            self.msg(format!("Your ears are still ringing ({} more turns).", left), GREY);
            return None;
        }
        self.listen_ready = self.time + 1000.0;
        let r = self.hear_radius();
        let mut heard = Vec::new();
        for m in self.level.monsters.iter() {
            if !m.alive || m.faction == Faction::Ally || self.level.is_visible(m.x, m.y) {
                continue;
            }
            let Some(d) = self.heard_from(m.x, m.y) else { continue };
            // The sleeping make little noise: only a snore from close by.
            if !m.awake && d > r / 2 {
                continue;
            }
            heard.push((m.x, m.y, d));
        }
        self.pings.clear();
        for (x, y, d) in heard {
            let (ax, ay) = self.sound_corner(x, y);
            let guess = (d + self.rng.gen_range(-1..=1)).max(1);
            let (wave, reach) = self.sound_wave(x, y);
            self.pings.push(Ping { x, y, ax, ay, dist: guess, age: 0.0, wave, reach });
        }
        let n = self.pings.len();
        let col = Rgb(120, 170, 255);
        if n == 0 {
            self.msg("You hold your breath and listen... Silence.", col);
        } else {
            self.msg(
                format!(
                    "You hold your breath and listen... {} {} nearby.",
                    n,
                    if n == 1 { "sound" } else { "sounds" }
                ),
                col,
            );
        }
        Some(100.0)
    }

    /// How a sound made at (x,y) reaches you, for drawing: it fills the space
    /// around where it was made, rings off those walls, and the part of it
    /// that finds a way toward you runs down the corridors to where you stand.
    /// Each tile carries how far (in half-tiles, muffling included) the sound
    /// has come to get there. Returns the tiles and the furthest of them.
    fn sound_wave(&self, x: i32, y: i32) -> (Vec<(i32, i32, u16)>, u16) {
        let lv = &self.level;
        let total = self.smap[lv.idx(x, y)];
        let field = lv.sound_field(x, y, total.saturating_add(12));
        // Allowed detour: the whole room it starts in, narrowing to the way to you.
        let slack = 10u16;
        let mut out = Vec::new();
        let mut reach = 0;
        for ty in 0..lv.h {
            for tx in 0..lv.w {
                let i = lv.idx(tx, ty);
                let (a, b) = (field[i], self.smap[i]);
                if a == SOUND_FAR {
                    continue;
                }
                let near_source = a <= 12;
                let on_the_way = b != SOUND_FAR && a as u32 + b as u32 <= total as u32 + slack as u32;
                // Walls only show where the sound strikes them on its way.
                let wall = lv.tile(tx, ty).opaque();
                let wall_ok = wall
                    && DIRS.iter().any(|&(dx, dy)| {
                        let j = (tx + dx, ty + dy);
                        lv.inb(j.0, j.1) && {
                            let k = lv.idx(j.0, j.1);
                            field[k] != SOUND_FAR
                                && !lv.tile(j.0, j.1).opaque()
                                && (field[k] <= 12
                                    || (self.smap[k] != SOUND_FAR
                                        && field[k] as u32 + self.smap[k] as u32 <= total as u32 + slack as u32))
                        }
                    });
                if (wall && wall_ok) || (!wall && (near_source || on_the_way)) {
                    out.push((tx, ty, a));
                    if a > reach && a != SOUND_FAR {
                        reach = a;
                    }
                }
            }
        }
        (out, reach)
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
                .filter(|(_, m)| {
                    m.alive && m.faction != Faction::Ally && !self.level.is_visible(m.x, m.y)
                })
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

/// A rough compass direction for an offset on the map (north is up).
pub fn compass(dx: f32, dy: f32) -> &'static str {
    if dx.abs() < 0.5 && dy.abs() < 0.5 {
        return "right beside you";
    }
    let a = dy.atan2(dx); // 0 = east, +y is south
    let k = ((a / (PI / 4.0)).round() as i32).rem_euclid(8);
    ["to the east", "to the south-east", "to the south", "to the south-west", "to the west", "to the north-west", "to the north", "to the north-east"][k as usize]
}

/// Group a pack into slots; see `Game::inv_slots`.
pub fn slots_of(inv: &[Item], stack: usize) -> Vec<(usize, usize)> {
    let mut slots: Vec<(usize, usize)> = Vec::new();
    for (i, it) in inv.iter().enumerate() {
        if matches!(it, Item::Potion(_)) {
            if let Some(s) = slots
                .iter_mut()
                .find(|s| inv[s.0] == *it && s.1 < stack)
            {
                s.1 += 1;
                continue;
            }
        }
        slots.push((i, 1));
    }
    slots
}

#[cfg(test)]
mod tests {
    use super::*;

    const M_WRAITH: usize = 13;
    const M_ORC: usize = 7;
    const M_GOBLIN: usize = 3;
    const M_SKELETON: usize = 4;

    fn game() -> Game {
        let mut g = Game::new(
            0,
            Bonuses {
                xp_mult: 1.0,
                ..Default::default()
            },
            0,
        );
        g.level.monsters.clear();
        g.p.max_hp = 10_000;
        g.p.hp = 10_000;
        g
    }

    fn spawn(g: &mut Game, kind: usize, x: i32, y: i32) -> usize {
        let id = g.new_id();
        let mut m = Monster::new(id, kind, x, y, g.time);
        m.awake = true;
        g.level.monsters.push(m);
        g.level.monsters.len() - 1
    }

    /// A free floor tile at least `d` steps from the player.
    fn floor_away(g: &mut Game, d: i32) -> (i32, i32) {
        loop {
            let c = g.level.random_floor(&mut g.rng);
            if cheb(c.0, c.1, g.p.x, g.p.y) >= d {
                return c;
            }
        }
    }

    #[test]
    fn ghost_strikes_from_inside_a_wall() {
        let mut g = game();
        let (px, py) = (g.p.x, g.p.y);
        let (wx, wy) = DIRS
            .iter()
            .step_by(2)
            .map(|&(dx, dy)| (px + dx, py + dy))
            .find(|&(x, y)| g.ghost_can_enter(x, y))
            .expect("an interior tile next to the start");
        g.level.set(wx, wy, Tile::Wall);
        let mi = spawn(&mut g, M_WRAITH, wx, wy);
        g.level.monsters[mi].target = Some(Target::Player);
        for _ in 0..60 {
            g.monster_melee(mi, Target::Player);
        }
        assert!(g.msgs.iter().any(|m| m
            .text
            .starts_with("An unseen power hits you from a nearby wall for")));
        assert!(g.p.hp < 10_000);
    }

    #[test]
    fn ghost_drifts_through_walls_toward_the_player() {
        let mut g = game();
        let (gx, gy) = floor_away(&mut g, 12);
        let mi = spawn(&mut g, M_WRAITH, gx, gy);
        let (px, py) = (g.p.x, g.p.y);
        for _ in 0..80 {
            g.monster_step_toward(mi, px, py, true);
            let m = &g.level.monsters[mi];
            if cheb(m.x, m.y, px, py) <= 1 {
                break;
            }
        }
        let m = &g.level.monsters[mi];
        assert!(
            cheb(m.x, m.y, px, py) <= 1,
            "ghost should reach the player in a straight line"
        );
    }

    #[test]
    fn slaying_a_champion_rewards_and_terrifies() {
        let mut g = game();
        let (px, py) = (g.p.x, g.p.y);
        let (cx, cy) = (px + 1, py);
        g.level.set(cx, cy, Tile::Floor);
        let ci = spawn(&mut g, M_ORC, cx, cy);
        g.make_champion(ci);
        assert!(g.level.monsters[ci].champion);
        assert!(g.level.monsters[ci].max_hp > MONSTERS[M_ORC].hp);
        let (wx, wy) = floor_away(&mut g, 4);
        let wi = spawn(&mut g, M_GOBLIN, wx, wy);
        // A witness within earshot.
        g.level.monsters[wi].x = cx;
        g.level.monsters[wi].y = cy + 3;
        g.level.set(cx, cy + 3, Tile::Floor);
        g.p.hp = 100;
        let xp0 = g.p.xp;
        let hp = g.level.monsters[ci].hp;
        g.damage_monster(ci, hp + 1, Src::Player);
        assert!(
            g.p.xp - xp0 >= MONSTERS[M_ORC].xp * 3,
            "champions give triple experience"
        );
        assert!(g.p.hp > 100 + g.bonus.siphon, "champion kills heal more");
        let w = &g.level.monsters[wi];
        assert!((5..=20).contains(&w.fear), "witness fear {}", w.fear);
    }

    #[test]
    fn infighting_victors_become_champions() {
        let mut g = game();
        let (x, y) = floor_away(&mut g, 3);
        let ki = spawn(&mut g, M_ORC, x, y);
        let kid = g.level.monsters[ki].id;
        let mut promoted = false;
        for _ in 0..60 {
            let (vx, vy) = floor_away(&mut g, 3);
            let vi = spawn(&mut g, M_SKELETON, vx, vy);
            g.damage_monster(vi, 999, Src::Mon(kid));
            if g.level.monsters[ki].champion {
                promoted = true;
                break;
            }
        }
        assert!(promoted);
        let m = &g.level.monsters[ki];
        assert!(m.speed() < MONSTERS[M_ORC].speed && m.dmg_mult() > 1.0);
    }

    #[test]
    fn uniques_drop_their_signature_and_stay_dead() {
        for floor in 1..=10usize {
            let mut g = game();
            g.depth = floor as u32;
            let (x, y) = floor_away(&mut g, 2);
            let kind = M_UNIQUE0 + floor - 1;
            assert_eq!(MONSTERS[kind].unique as usize, floor);
            let mi = spawn(&mut g, kind, x, y);
            g.damage_monster(mi, 9999, Src::Player);
            let sig = MONSTERS[kind].sig;
            assert!(
                g.level.items.iter().any(|i| i.item.unique() == Some(sig)),
                "floor {} drop",
                floor
            );
            assert!(
                g.level.corpses.iter().all(|c| c.kind != kind),
                "uniques leave no corpse to raise"
            );
        }
    }

    #[test]
    fn the_amulet_turns_every_monster_on_the_player() {
        let mut g = game();
        g.p.has_amulet = true;
        let (x, y) = floor_away(&mut g, 6);
        let a = spawn(&mut g, M_ORC, x, y);
        g.level.monsters[a].awake = false;
        let b = spawn(&mut g, M_SKELETON, x, y);
        g.level.monsters[b].x = x;
        let (bx, by) = g.free_adjacent(x, y).expect("room");
        g.level.monsters[b].x = bx;
        g.level.monsters[b].y = by;
        g.level.monsters[b].target = Some(Target::Mon(g.level.monsters[a].id));
        g.monster_act(a);
        g.monster_act(b);
        for i in [a, b] {
            let m = &g.level.monsters[i];
            assert!(m.awake);
            assert_eq!(m.target, Some(Target::Player));
        }
    }

    #[test]
    fn unique_gear_powers_apply() {
        let mut g = game();
        let ring = UNIQUES
            .iter()
            .position(|u| u.power == UPower::Wrath)
            .unwrap();
        let before = g.dmg_bonus();
        g.p.inv.push(unique_item(ring));
        let i = g.p.inv.len() - 1;
        g.use_item(i);
        assert_eq!(g.p.ring, Some(ring as u8));
        assert_eq!(g.dmg_bonus(), before + 3);
        let neck = UNIQUES
            .iter()
            .position(|u| u.power == UPower::Undying)
            .unwrap();
        g.p.neck = Some(neck as u8);
        g.p.hp = 5;
        g.player_hit(50, DeathCause::Abandoned, false);
        assert_eq!(g.p.hp, 1);
        assert!(g.over.is_none());
    }

    #[test]
    fn a_run_survives_a_save_round_trip() {
        let mut g = game();
        for _ in 0..30 {
            g.do_action(Action::Wait);
        }
        let (x, y) = floor_away(&mut g, 3);
        let ci = spawn(&mut g, M_ORC, x, y);
        g.make_overlord(ci);
        g.p.ring = Some(13);
        let json = serde_json::to_string(&g).expect("serialize");
        let mut h: Game = serde_json::from_str(&json).expect("deserialize");
        h.enter_level_keep_facing();
        assert_eq!(
            (h.p.x, h.p.y, h.depth, h.turns, h.p.hp),
            (g.p.x, g.p.y, g.depth, g.turns, g.p.hp)
        );
        assert_eq!(h.level.monsters.len(), g.level.monsters.len());
        assert!(h.level.monsters.iter().any(|m| m.overlord));
        assert_eq!(h.p.ring, Some(13));
        assert_eq!(h.unique_chance, g.unique_chance);
        assert!(!h.dmap.is_empty());
        h.do_action(Action::Wait);
    }

    #[test]
    fn rival_champions_fight_and_overlords_grant_power() {
        let mut g = game();
        let (x, y) = floor_away(&mut g, 6);
        let a = spawn(&mut g, M_ORC, x, y);
        let (bx, by) = g.free_adjacent(x, y).expect("room");
        // Same faction: normally friends, but champions are rivals.
        let b = spawn(&mut g, M_ORC, bx, by);
        g.make_champion(a);
        g.make_champion(b);
        g.monster_act(a);
        let bid = g.level.monsters[b].id;
        assert_eq!(g.level.monsters[a].target, Some(Target::Mon(bid)));

        let o = spawn(&mut g, M_GOBLIN, x, y);
        g.level.monsters[o].x = 1;
        g.make_overlord(o);
        assert!(g.level.monsters[o].overlord && g.level.monsters[o].dmg_mult() > 1.4);
        let before = (
            g.p.max_hp,
            g.p.extra_dmg,
            g.p.extra_ac,
            g.p.max_mp,
            g.p.extra_crit,
        );
        g.p.hp = 100;
        g.damage_monster(o, 99_999, Src::Player);
        let after = (
            g.p.max_hp,
            g.p.extra_dmg,
            g.p.extra_ac,
            g.p.max_mp,
            g.p.extra_crit,
        );
        assert_ne!(
            before, after,
            "slaying an overlord grants a permanent boost"
        );
        assert!(g
            .msgs
            .iter()
            .any(|m| m.text.contains("steal the overlord's power")));
    }

    #[test]
    fn pick_one_or_everything() {
        let mut g = game();
        let (px, py) = (g.p.x, g.p.y);
        g.level.items.retain(|i| !(i.x == px && i.y == py));
        g.p.inv.clear();
        for it in [
            Item::Potion(Potion::Mana),
            Item::Scroll(Scroll::Vision),
            Item::Potion(Potion::Cure),
        ] {
            g.level.items.push(FloorItem {
                x: px,
                y: py,
                item: it,
            });
        }
        assert_eq!(g.items_here().len(), 3);
        g.do_action(Action::PickupOne(1));
        assert_eq!(g.p.inv, vec![Item::Scroll(Scroll::Vision)]);
        let t0 = g.time;
        g.do_action(Action::PickupAll);
        assert!(g.items_here().is_empty());
        assert_eq!(g.p.inv.len(), 3);
        assert!(
            (g.time - t0 - 100.0).abs() < 1e-6,
            "grab-all takes one turn"
        );
        // Read the scroll of monster vision.
        let i =
            g.p.inv
                .iter()
                .position(|i| *i == Item::Scroll(Scroll::Vision))
                .unwrap();
        g.use_item(i);
        assert_eq!(g.p.vision, 10);
    }

    #[test]
    fn the_dead_rise_faster_nearer_the_surface() {
        let mut g = game();
        assert_eq!(g.respawn_factor(), 1.0);
        g.p.has_amulet = true;
        g.depth = 10;
        assert!((g.respawn_factor() - 1.0).abs() < 1e-9);
        let mut last = 1.0;
        for d in (1..10).rev() {
            g.depth = d;
            let f = g.respawn_factor();
            assert!(f < last);
            last = f;
        }
    }
    #[test]
    fn monster_closets_fill_a_room() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut found = 0;
        let mut elites = 0;
        for depth in 2..=10 {
            for _ in 0..12 {
                let mut lv = generate(depth, &mut rng);
                let kinds: Vec<usize> = (0..MONSTERS.len())
                    .filter(|&i| {
                        !MONSTERS[i].boss
                            && depth >= MONSTERS[i].depth.0
                            && depth <= MONSTERS[i].depth.1
                    })
                    .collect();
                let mut id = 1;
                place_closet(&mut lv, &mut rng, &mut id, &kinds, 0.0, 1.0);
                if let Some(r) = lv.closet {
                    found += 1;
                    let inside = lv.monsters.iter().filter(|m| r.contains(m.x, m.y)).count();
                    assert!(inside >= 8, "a closet holds lots of monsters ({})", inside);
                    assert!(!r.contains(lv.up.0, lv.up.1) && !r.contains(lv.down.0, lv.down.1));
                    for m in &lv.monsters {
                        assert!(lv.tile(m.x, m.y).walkable());
                    }
                    elites += lv.monsters.iter().filter(|m| m.champion).count();
                }
            }
        }
        assert!(found > 50, "most levels have room for a closet ({})", found);
        assert!(elites > 0, "closets sometimes hold champions");
    }

    #[test]
    fn closet_discovery_message() {
        let mut g = game();
        let r = g.level.rooms[0];
        g.level.closet = Some(r);
        for _ in 0..5 {
            let (x, y) = g.level.random_floor_in(&r, &mut g.rng).unwrap();
            if g.level.monster_at(x, y).is_none() && (x, y) != (g.p.x, g.p.y) {
                spawn(&mut g, M_GOBLIN, x, y);
            }
        }
        g.refresh_view();
        assert!(g.level.closet_found);
    }

    #[test]
    fn difficulty_scales_monsters() {
        let mut g = game();
        g.difficulty = 3;
        let (x, y) = floor_away(&mut g, 3);
        let mi = spawn(&mut g, M_ORC, x, y);
        let k = g.diff().hp;
        g.level.monsters[mi].scale_hp(k);
        assert_eq!(g.level.monsters[mi].max_hp, MONSTERS[M_ORC].hp * 2);
        let mut tot_hard = 0;
        let mut tot_norm = 0;
        for _ in 0..400 {
            g.difficulty = 3;
            tot_hard += g.mon_damage(mi, (10, 10));
            g.difficulty = 0;
            tot_norm += g.mon_damage(mi, (10, 10));
        }
        assert_eq!(tot_norm, 4000);
        assert_eq!(tot_hard, 6800);
        // A fresh Chaotic run builds its floors with tougher monsters.
        let g2 = Game::new(
            0,
            Bonuses {
                xp_mult: 1.0,
                ..Default::default()
            },
            3,
        );
        assert!(g2
            .level
            .monsters
            .iter()
            .any(|m| m.max_hp >= MONSTERS[m.kind].hp * 2));
    }

    #[test]
    fn kills_are_tallied_for_the_bestiary() {
        let mut g = game();
        let (x, y) = floor_away(&mut g, 3);
        let mi = spawn(&mut g, M_GOBLIN, x, y);
        g.damage_monster(mi, 999, Src::Player);
        let (x, y) = floor_away(&mut g, 3);
        let mi = spawn(&mut g, M_GOBLIN, x, y);
        g.damage_monster(mi, 999, Src::Player);
        assert_eq!(g.slain.get("goblin"), Some(&2));
        // Monsters killing each other teach you nothing.
        let (x, y) = floor_away(&mut g, 3);
        let a = spawn(&mut g, M_ORC, x, y);
        let aid = g.level.monsters[a].id;
        let (x, y) = floor_away(&mut g, 3);
        let b = spawn(&mut g, M_SKELETON, x, y);
        g.damage_monster(b, 999, Src::Mon(aid));
        assert_eq!(g.slain.get("skeleton"), None);

        let mut prof = crate::profile::Profile::default();
        prof.bestiary.insert("goblin".into(), 199);
        let newly = prof.record_kills(&g.slain);
        assert_eq!(newly, vec!["goblin".to_string()]);
        assert!(prof.bonuses().mastered.contains(&"goblin".to_string()));
    }

    #[test]
    fn difficulty_unlocks_in_order() {
        let mut prof = crate::profile::Profile::default();
        assert!(prof.diff_unlocked(0) && !prof.diff_unlocked(1));
        assert_eq!(prof.record_win(1), None, "can't skip ahead");
        assert_eq!(prof.record_win(0), Some("Hard"));
        assert_eq!(prof.record_win(0), None);
        assert_eq!(prof.record_win(1), Some("Difficult"));
        assert_eq!(prof.record_win(2), Some("Chaotic"));
        assert_eq!(prof.record_win(3), None);
        assert!(prof.diff_unlocked(3));
    }

    #[test]
    fn legacy_skills_are_refunded() {
        let mut prof = crate::profile::Profile::default();
        prof.upgrades.insert("blink".into(), 1);
        prof.upgrades.insert("wind".into(), 1);
        prof.upgrades.insert("might".into(), 2);
        assert_eq!(prof.migrate(), 650);
        assert_eq!(prof.points, 650);
        assert_eq!(prof.rank("might"), 2);
        assert!(prof.upgrades.get("blink").is_none());
    }

    #[test]
    fn class_skills_stay_with_their_class() {
        let all: Vec<Skill> = UPGRADES.iter().filter_map(|u| u.skill).collect();
        for class in 0..CLASSES.len() {
            let g = Game::new(
                class,
                Bonuses {
                    xp_mult: 1.0,
                    skills: all.clone(),
                    ..Default::default()
                },
                0,
            );
            let sk = g.skills();
            assert_eq!(sk.len(), 4, "two class skills plus two altar skills");
            for s in &sk[2..] {
                assert_eq!(s.class(), Some(class));
            }
        }
    }

    #[test]
    fn new_skills_work() {
        // Warrior: Battle Cry terrifies and enrages.
        let mut g = game();
        g.p.mp = 100;
        let (px, py) = (g.p.x, g.p.y);
        let (x, y) = g.free_adjacent(px, py).unwrap();
        let mi = spawn(&mut g, M_ORC, x, y);
        assert!(g.use_skill(Skill::BattleCry).is_some());
        assert!(g.p.fury > 0 && g.level.monsters[mi].fear > 0);
        assert!(g.dmg_mult_vs(false, false) > 1.4);

        // Rogue: Shadowstep lands behind a visible foe and backstabs it.
        let mut g = game();
        g.p.class = C_ROGUE;
        g.p.mp = 100;
        let (px, py) = (g.p.x, g.p.y);
        let f = (0..8).find(|&d| {
            let (dx, dy) = DIRS[d];
            (1..=4).all(|k| g.level.tile(px + dx * k, py + dy * k).walkable())
        });
        if let Some(d) = f {
            let (dx, dy) = DIRS[d];
            let mi = spawn(&mut g, M_ORC, px + dx * 3, py + dy * 3);
            g.level.monsters[mi].max_hp = 5000;
            g.level.monsters[mi].hp = 5000;
            g.refresh_view();
            assert!(g.use_skill(Skill::Shadowstep).is_some());
            assert!(
                cheb(g.p.x, g.p.y, px + dx * 3, py + dy * 3) == 1,
                "lands next to the foe"
            );
        }

        // Necromancer: corpses explode and never rise.
        let mut g = game();
        g.p.mp = 100;
        let (px, py) = (g.p.x, g.p.y);
        let (cx, cy) = g.free_adjacent(px, py).unwrap();
        g.level.corpses.push(Corpse {
            kind: M_SKELETON,
            x: cx,
            y: cy,
            revive_at: 1.0,
            raises: 0,
        });
        let mi = spawn(&mut g, M_GOBLIN, cx, cy);
        assert!(g.use_skill(Skill::CorpseExplosion).is_some());
        assert!(
            !g.level.corpses.iter().any(|c| c.kind == M_SKELETON),
            "the skeleton is gone for good"
        );
        let gob = &g.level.monsters[mi];
        assert!(!gob.alive || gob.hp < MONSTERS[M_GOBLIN].hp);

        // Blood Magic pays in health.
        let mut g = game();
        g.bonus.bloodmagic = true;
        g.p.mp = 0;
        let hp = g.p.hp;
        assert!(g.use_skill(Skill::SmokeBomb).is_some());
        assert_eq!(g.p.hp, hp - Skill::SmokeBomb.mp() * 2);
        assert!(g.p.invis > 0);
    }

    #[test]
    fn soul_anchor_cheats_death_once() {
        let mut g = game();
        g.bonus.anchor = true;
        g.p.max_hp = 50;
        g.p.hp = 5;
        g.player_hit(100, DeathCause::OwnFire, false);
        assert!(g.over.is_none());
        assert_eq!(g.p.hp, 20);
        g.player_hit(100, DeathCause::OwnFire, false);
        assert!(g.over.is_some());
    }

    #[test]
    fn boons_apply() {
        let mut g = game();
        g.bonus.pockets = 2;
        assert_eq!(g.inv_cap(), MAX_INV + 6);
        g.bonus.ironwill = 2;
        assert!(!g.add_poison(10, "x".into()));
        assert_eq!(g.p.poison, 0);
        g.bonus.coup = 2;
        g.bonus.hunter = 1;
        assert!((g.dmg_mult_vs(true, true) - 1.15 * 1.3).abs() < 1e-4);
        g.bonus.carto = 1;
        let mut rng = StdRng::seed_from_u64(3);
        let mut lv = generate(4, &mut rng);
        chart_level(&mut lv, 1);
        assert!(lv.explored[lv.idx(lv.down.0, lv.down.1)]);
    }

    #[test]
    fn themes_are_dealt_in_bands() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut firsts = std::collections::HashSet::new();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let th = roll_themes(&mut rng);
            assert_eq!(th.len(), 9);
            // Near the surface: cellars, crypts, gaols and sewers. The strange places wait below.
            assert!(
                th[..3].iter().all(|s| SHALLOW_THEMES.contains(s)),
                "{:?}",
                th
            );
            assert!(
                th[3..6].iter().all(|s| MIDDLE_THEMES.contains(s)),
                "{:?}",
                th
            );
            assert!(th[6..].iter().all(|s| DEEP_THEMES.contains(s)), "{:?}", th);
            for tier in th.chunks(3) {
                let distinct: std::collections::HashSet<_> = tier.iter().collect();
                assert!(distinct.len() <= 2);
                // A theme never comes back after it has given way.
                assert!(!(tier[0] == tier[2] && tier[1] != tier[0]));
            }
            seen.extend(th.iter().copied());
            firsts.insert(th[3]);
        }
        assert_eq!(seen.len(), FLOOR_THEMES.len(), "every theme turns up");
        assert!(firsts.len() > 2, "depth 4 is not always the same theme");
        // Older saves keep the classic layout and names.
        let c = classic_themes();
        assert_eq!(floor_name(&c, 1), "The Old Cellars");
        assert_eq!(floor_name(&c, 4), "The Mossy Halls");
        assert_eq!(floor_name(&c, 9), "The Halls of Torment");
        assert_eq!(floor_name(&c, 10), "The Sanctum of GRAZZARK");
        // New floors take the run's styles.
        let mut g = game();
        assert_eq!(g.level.style, g.themes[0]);
        for _ in 0..9 {
            let (dx, dy) = g.level.down;
            g.p.x = dx;
            g.p.y = dy;
            g.use_stairs();
            assert_eq!(g.level.style, g.style_for(g.depth));
        }
        assert_eq!(g.depth, 10);
        assert_eq!(g.level.style, SANCTUM_STYLE);
    }

    #[test]
    fn rare_monsters_have_short_lore() {
        let warden = MONSTERS.iter().find(|m| m.boss).unwrap();
        let lt = warden.lore();
        assert_eq!(
            (lt.name, lt.basic, lt.desc, lt.adv, lt.master),
            (1, 3, 5, 7, 10)
        );
        let mut prof = crate::profile::Profile::default();
        prof.bestiary.insert(warden.name.into(), 9);
        let mut slain = HashMap::new();
        slain.insert(warden.name.to_string(), 1);
        assert_eq!(prof.record_kills(&slain), vec![warden.name.to_string()]);
        assert!(prof.bonuses().mastered.contains(&warden.name.to_string()));
        assert_eq!(MONSTERS[M_GOBLIN].lore().master, 200);
    }

    #[test]
    fn potion_belt_stacks_potions_in_one_slot() {
        let mut prof = crate::profile::Profile::default();
        let i = UPGRADES.iter().position(|u| u.id == "belt").unwrap();
        assert_eq!(prof.next_cost(i), Some(150));
        prof.upgrades.insert("belt".into(), 1);
        assert_eq!(prof.next_cost(i), Some(300));
        prof.upgrades.insert("belt".into(), 2);
        assert_eq!(prof.next_cost(i), None);
        assert_eq!(prof.bonuses().belt, 2);

        let heal = Item::Potion(Potion::Healing);
        let mana = Item::Potion(Potion::Mana);
        let scroll = Item::Scroll(Scroll::Vision);
        let mut g = game();
        g.p.inv = vec![heal, scroll, heal, mana, heal];
        // No belt: one potion a slot.
        assert_eq!(g.inv_slots().len(), 5);
        g.bonus.belt = 1;
        assert_eq!(g.inv_slots(), vec![(0, 2), (1, 1), (3, 1), (4, 1)]);
        g.bonus.belt = 2;
        assert_eq!(g.inv_slots(), vec![(0, 3), (1, 1), (3, 1)]);
        assert_eq!(g.slot_item(2), Some(3));
        assert_eq!(g.slot_item(3), None);

        // A full pack still takes a potion that fits on a stack.
        g.bonus.belt = 1;
        g.p.inv = vec![scroll; g.inv_cap() - 1];
        g.p.inv.push(heal);
        assert_eq!(g.free_slots(), 0);
        assert!(g.can_carry(&heal));
        assert!(!g.can_carry(&mana));
        assert!(!g.can_carry(&scroll));
        g.p.inv.push(heal);
        assert!(!g.can_carry(&heal), "the stack is full");
        g.bonus.belt = 0;
        g.p.inv.pop();
        assert!(!g.can_carry(&heal), "no belt, no stacking");

        // Picking up from the floor follows the same rule.
        g.bonus.belt = 1;
        let (px, py) = (g.p.x, g.p.y);
        g.level.items.retain(|it| (it.x, it.y) != (px, py));
        for it in [mana, heal, scroll] {
            g.level.items.push(FloorItem { x: px, y: py, item: it });
        }
        g.do_action(Action::PickupAll);
        assert_eq!(g.p.inv.iter().filter(|&&i| i == heal).count(), 2);
        assert_eq!(g.items_here().len(), 2, "mana and scroll stay behind");
    }

    #[test]
    fn backtick_sorts_the_pack_by_type() {
        let mut g = game();
        let gear = Gear { def: 0, bonus: 0, uniq: None };
        g.p.inv = vec![
            Item::Scroll(Scroll::Vision),
            Item::Potion(Potion::Mana),
            Item::Armor(gear),
            Item::Potion(Potion::Healing),
            Item::Weapon(gear),
            Item::Potion(Potion::Mana),
        ];
        g.sort_inv();
        assert_eq!(
            g.p.inv,
            vec![
                Item::Weapon(gear),
                Item::Armor(gear),
                Item::Potion(Potion::Healing),
                Item::Potion(Potion::Mana),
                Item::Potion(Potion::Mana),
                Item::Scroll(Scroll::Vision),
            ]
        );
    }

    #[test]
    fn appraisers_eye_compares_with_worn_gear() {
        let mut prof = crate::profile::Profile::default();
        for id in ["appraise", "gulp"] {
            let i = UPGRADES.iter().position(|u| u.id == id).unwrap();
            assert_eq!(prof.next_cost(i), Some(500));
            prof.upgrades.insert(id.into(), 1);
            assert_eq!(prof.next_cost(i), None);
        }
        let b = prof.bonuses();
        assert!(b.appraise && b.gulp);

        let mut g = game();
        let w = |name: &str| WEAPONS.iter().position(|d| d.name == name).unwrap();
        let a = |name: &str| ARMORS.iter().position(|d| d.name == name).unwrap();
        let gear = |def| Gear { def, bonus: 0, uniq: None };
        g.p.wpn = gear(w("Mace"));
        let (mace, sword) = (&WEAPONS[w("Mace")], &WEAPONS[w("Long Sword")]);
        let up = g.upgrade_over_worn(&Item::Weapon(gear(w("Long Sword"))));
        if sword.dmg.0 + sword.dmg.1 > mace.dmg.0 + mace.dmg.1 {
            assert_eq!(up, Some((sword.dmg.0 - mace.dmg.0, sword.dmg.1 - mace.dmg.1)));
        } else {
            assert_eq!(up, None);
        }
        // The same weapon, better enchanted, is better by exactly its bonus.
        let plus2 = Item::Weapon(Gear { def: w("Mace"), bonus: 2, uniq: None });
        assert_eq!(g.upgrade_over_worn(&plus2), Some((2, 2)));
        assert_eq!(g.upgrade_over_worn(&Item::Weapon(gear(w("Mace")))), None);
        // Armour: how much AC it adds.
        g.p.arm = gear(a("Leather Armor"));
        let d = ARMORS[a("Chain Mail")].ac - ARMORS[a("Leather Armor")].ac;
        assert_eq!(g.upgrade_over_worn(&Item::Armor(gear(a("Chain Mail")))), Some((d, 0)));
        assert_eq!(g.upgrade_over_worn(&Item::Armor(gear(a("Leather Armor")))), None);
        assert_eq!(g.upgrade_over_worn(&Item::Potion(Potion::Healing)), None);
    }

    #[test]
    fn eager_gulp_drinks_might_and_vigor_underfoot() {
        let mut g = game();
        let (px, py) = (g.p.x, g.p.y);
        let put = |g: &mut Game, k| g.level.items.push(FloorItem { x: px, y: py, item: Item::Potion(k) });
        g.level.items.retain(|it| (it.x, it.y) != (px, py));
        put(&mut g, Potion::Might);
        put(&mut g, Potion::Vigor);
        put(&mut g, Potion::Healing);
        // Without the boon, nothing happens.
        g.after_player_move();
        assert_eq!(g.items_here().len(), 3);
        g.bonus.gulp = true;
        let (dmg, max_hp) = (g.p.extra_dmg, g.p.max_hp);
        let inv = g.p.inv.len();
        g.after_player_move();
        assert_eq!(g.p.extra_dmg, dmg + 1);
        assert_eq!(g.p.max_hp, max_hp + 5);
        assert_eq!(g.p.inv.len(), inv, "drunk, not carried");
        // Other potions stay on the floor for you to pick up.
        let here = g.items_here();
        assert_eq!(here.len(), 1);
        assert_eq!(g.level.items[here[0]].item, Item::Potion(Potion::Healing));
    }

    #[test]
    fn blows_sound_like_what_they_strike() {
        let mat = |n: &str| MONSTERS.iter().find(|d| d.name == n).unwrap().material();
        assert_eq!(mat("green slime"), Material::Ooze);
        assert_eq!(mat("skeleton"), Material::Bone);
        assert_eq!(mat("lich"), Material::Bone);
        assert_eq!(mat("Prism Guardian"), Material::Crystal);
        assert_eq!(mat("gargoyle"), Material::Stone);
        assert_eq!(mat("giant spider"), Material::Shell);
        assert_eq!(mat("wraith"), Material::Spirit);
        assert_eq!(mat("fire elemental"), Material::Flame);
        assert_eq!(mat("hobgoblin"), Material::Armor);
        assert_eq!(mat("giant rat"), Material::Flesh);
        assert_eq!(mat("zombie"), Material::Flesh);

        // A strike makes the target's sound (or a miss), never the old clang.
        let mut g = game();
        let (x, y) = floor_away(&mut g, 2);
        let mi = spawn(&mut g, M_SKELETON, x, y);
        g.level.monsters[mi].hp = 10_000;
        g.level.monsters[mi].max_hp = 10_000;
        for _ in 0..30 {
            g.sounds.clear();
            g.strike_delay = 0.25;
            g.player_strike(mi, 1.0, 50, None);
            let s = g.sounds.iter().find(|s| matches!(s.sfx, Sfx::Impact(_) | Sfx::Miss)).unwrap();
            assert!(matches!(s.sfx, Sfx::Impact(Material::Bone) | Sfx::Miss));
            assert!((s.delay - 0.25).abs() < 1e-6, "lands when the arrow does");
        }
        assert!(!g.sounds.iter().any(|s| s.sfx == Sfx::Clang));
    }

    #[test]
    fn arrows_fly_before_they_land() {
        let mut g = game();
        g.level.monsters.clear();
        let mut thunks = 0;
        for dir in 0..8 {
            g.sounds.clear();
            g.shoot_arrow(dir);
            assert!(g.sounds.iter().any(|s| s.sfx == Sfx::Arrow && s.delay == 0.0));
            for s in g.sounds.iter().filter(|s| s.sfx == Sfx::Impact(Material::Wall)) {
                assert!(s.delay > 0.0);
                thunks += 1;
            }
        }
        assert!(thunks > 0, "some arrow should hit a wall in range");
    }

    #[test]
    fn footsteps_ring_down_long_halls() {
        let mut g = game();
        let (px, py) = (g.p.x as f32 + 0.5, g.p.y as f32 + 0.5);
        g.echo_len = 2.0;
        let small = g.spatial_for(Sfx::Step, px, py, 0.5).unwrap();
        g.echo_len = 20.0;
        let big = g.spatial_for(Sfx::Step, px, py, 0.5).unwrap();
        assert!(big.echoes.len() > small.echoes.len());
        assert!(big.reverb > small.reverb);
        let far = big.echoes.iter().map(|e| e.0).fold(0.0f32, f32::max);
        assert!(far > 0.15, "a long hall echoes late ({})", far);
        // Other sounds are left as they were.
        let swing = g.spatial_for(Sfx::Swing, px, py, 0.5).unwrap();
        assert_eq!(swing.echoes.len(), g.spatial(px, py, 0.5).unwrap().echoes.len());
    }

    #[test]
    fn new_altar_boons_cost_what_they_should() {
        let mut prof = crate::profile::Profile::default();
        let gl = UPGRADES.iter().position(|u| u.id == "glint").unwrap();
        assert_eq!(prof.next_cost(gl), Some(1000));
        prof.upgrades.insert("glint".into(), 1);
        assert_eq!(prof.next_cost(gl), None);
        assert!(prof.bonuses().glint);
        let py = UPGRADES.iter().position(|u| u.id == "pyre").unwrap();
        let mut paid = Vec::new();
        for r in 0..5 {
            prof.upgrades.insert("pyre".into(), r);
            paid.push(prof.next_cost(py).unwrap());
        }
        assert_eq!(paid, vec![500, 750, 1000, 1250, 1500]);
        prof.upgrades.insert("pyre".into(), 5);
        assert_eq!(prof.next_cost(py), None);
        assert_eq!(prof.bonuses().pyre, 5);
    }

    #[test]
    fn last_rites_destroys_corpses_so_they_never_rise() {
        // Count corpses left by player kills of `kind` out of `n`.
        fn left(g: &mut Game, kind: usize, n: usize) -> usize {
            g.level.corpses.clear();
            for _ in 0..n {
                let (x, y) = floor_away(g, 3);
                let mi = spawn(g, kind, x, y);
                g.damage_monster(mi, 99_999, Src::Player);
                g.level.monsters.clear();
            }
            g.level.corpses.iter().filter(|c| c.kind == kind).count()
        }
        let mut g = game();
        // Without the boon every body stays.
        assert_eq!(left(&mut g, M_GOBLIN, 200), 200);
        // Maxed out: about a quarter are burned to ash.
        g.bonus.pyre = 5;
        let kept = left(&mut g, M_GOBLIN, 2000);
        assert!((1350..1650).contains(&kept), "kept {} of 2000", kept);
        // Skeletons are kicked apart, even with the torch out.
        g.p.torch_out = true;
        let kept = left(&mut g, M_SKELETON, 2000);
        assert!((1350..1650).contains(&kept), "kept {} of 2000", kept);
        // No flame, no pyre.
        assert_eq!(left(&mut g, M_GOBLIN, 200), 200);
        g.p.torch_out = false;
        // Shift+C turns it off and on again.
        g.toggle_rites();
        assert!(g.rites_off);
        assert_eq!(left(&mut g, M_GOBLIN, 200), 200);
        g.toggle_rites();
        assert!(!g.rites_off);
        // Kills by other monsters are not yours to burn.
        g.level.corpses.clear();
        for _ in 0..200 {
            let (x, y) = floor_away(&mut g, 3);
            let a = spawn(&mut g, M_ORC, x, y);
            let aid = g.level.monsters[a].id;
            let (x, y) = floor_away(&mut g, 3);
            let b = spawn(&mut g, M_GOBLIN, x, y);
            g.damage_monster(b, 99_999, Src::Mon(aid));
            g.level.monsters.clear();
        }
        assert_eq!(g.level.corpses.iter().filter(|c| c.kind == M_GOBLIN).count(), 200);
    }

    #[test]
    fn cartographer_is_dear_and_normal_pays_less() {
        let mut prof = crate::profile::Profile::default();
        let i = UPGRADES.iter().position(|u| u.id == "carto").unwrap();
        assert_eq!(prof.next_cost(i), Some(1500));
        prof.upgrades.insert("carto".into(), 1);
        assert_eq!(prof.next_cost(i), Some(5000));
        prof.upgrades.insert("carto".into(), 2);
        assert_eq!(prof.next_cost(i), None);
        // Other upgrades keep the squared price.
        let v = UPGRADES.iter().position(|u| u.id == "vitality").unwrap();
        prof.upgrades.insert("vitality".into(), 1);
        assert_eq!(prof.next_cost(v), Some(160));
        assert!((DIFFICULTIES[0].souls - 0.8).abs() < 1e-6);
    }

    #[test]
    fn the_dungeon_rots_toward_the_surface() {
        let mut g = game();
        g.depth = 5;
        assert_eq!(g.corruption(), 0.0);
        g.p.has_amulet = true;
        let mut last = -1.0;
        for d in (1..=9).rev() {
            g.depth = d;
            let c = g.corruption();
            assert!(c > last);
            last = c;
        }
        g.depth = 10;
        assert_eq!(g.corruption(), 0.0, "the Sanctum has faces instead");
    }

    #[test]
    fn curses_torment_the_amulet_bearer() {
        // Forgetting wipes the map, then gives it back.
        let mut g = game();
        g.p.has_amulet = true;
        let n = g.level.explored.len();
        for i in 0..n / 2 {
            g.level.explored[i] = true;
        }
        let before = g.level.explored.clone();
        g.start_curse(Curse::Forgetting, 5);
        let left = g.level.explored.iter().filter(|&&e| e).count();
        assert!(left < before.iter().filter(|&&e| e).count());
        assert!(g
            .level
            .explored
            .iter()
            .zip(g.level.visible.iter())
            .all(|(&e, &v)| !e || v));
        for _ in 0..5 {
            g.tick_curses();
        }
        assert!(!g.cursed(Curse::Forgetting));
        assert!(before
            .iter()
            .zip(g.level.explored.iter())
            .all(|(&b, &e)| !b || e));

        // Figments can be walked straight through.
        let mut g = game();
        let (fx, fy) = g.free_adjacent(g.p.x, g.p.y).unwrap();
        g.curses.push(ActiveCurse {
            kind: Curse::Figments,
            turns: 10,
        });
        g.figments.push(Figment {
            id: 999,
            kind: M_ORC,
            x: fx,
            y: fy,
            rx: 0.0,
            ry: 0.0,
        });
        let dir = DIRS
            .iter()
            .position(|&(dx, dy)| (g.p.x + dx, g.p.y + dy) == (fx, fy))
            .unwrap() as i32;
        let hp = g.p.hp;
        assert!(g.player_move(dir).is_some());
        assert_eq!((g.p.x, g.p.y), (fx, fy));
        assert!(g.figments.is_empty());
        assert!(g
            .msgs
            .iter()
            .any(|m| m.text == "An evil figment of the mind!"));
        assert_eq!(g.p.hp, hp);
        // ...and they never hurt you.
        g.spawn_figments();
        assert!(!g.figments.is_empty());
        for _ in 0..40 {
            g.move_figments();
        }
        assert_eq!(g.p.hp, hp);

        // Hush silences skills and scrolls; Darkness smothers your sight.
        let mut g = game();
        g.p.mp = 100;
        g.start_curse(Curse::Hush, 5);
        assert!(g.use_skill(g.skills()[0]).is_none());
        g.p.inv.push(Item::Scroll(Scroll::Mapping));
        let k = g.p.inv.len() - 1;
        assert!(g.use_item(k).is_none());
        let s = g.sight();
        g.start_curse(Curse::Darkness, 5);
        assert!(g.sight() < s && g.sight() <= 2);

        // Leaden limbs slow you down.
        let mut g = game();
        g.curses.push(ActiveCurse {
            kind: Curse::Leaden,
            turns: 5,
        });
        assert!(g.action_cost(100.0) > 100.0);

        // Curses only strike while you hold the Amulet.
        let mut g = game();
        for _ in 0..3000 {
            g.tick_curses();
        }
        assert!(g.curses.is_empty());
        g.p.has_amulet = true;
        g.depth = 1;
        for _ in 0..3000 {
            g.tick_curses();
            if g.over.is_some() {
                break;
            }
        }
        assert!(g
            .msgs
            .iter()
            .any(|m| Curse::ALL.iter().any(|c| m.text == c.onset())));
    }

    fn thrall(g: &mut Game, kind: usize) -> usize {
        let (x, y) = g.free_adjacent(g.p.x, g.p.y).unwrap();
        let i = spawn(g, kind, x, y);
        let m = &mut g.level.monsters[i];
        m.faction = Faction::Ally;
        m.summoned = true;
        m.thrall = true;
        g.sync_upkeep();
        i
    }

    #[test]
    fn thralls_cost_max_health_while_they_live() {
        let mut g = game();
        let base = g.p.max_hp;
        let a = thrall(&mut g, M_SKELETON);
        let _b = thrall(&mut g, M_SKELETON);
        assert_eq!(g.p.max_hp, base - 4);
        g.damage_monster(a, 9999, Src::Env);
        assert_eq!(g.p.max_hp, base - 2, "a dead thrall gives its share back");
        // Stronger monsters hold more of your life.
        assert!(g.thrall_cost(M_ORC, false) > g.thrall_cost(0, false));
        let ogre = MONSTERS.iter().position(|m| m.name == "ogre").unwrap();
        assert_eq!(g.thrall_cost(ogre, false), 5);
        assert_eq!(g.thrall_cost(ogre, true), 7, "raised again costs more");
        // The cost shrinks every 10 levels.
        g.p.level = 10;
        assert_eq!(g.thrall_cost(ogre, false), 4);
        g.p.level = 20;
        assert_eq!(g.thrall_cost(ogre, false), 3);
        g.p.level = 25;
        g.sync_upkeep();
        assert_eq!(g.p.max_hp, base - 1);
        // Never fatal.
        g.p.max_hp = 2;
        g.p.hp = 2;
        g.p.thrall_upkeep = 0;
        g.p.level = 1;
        thrall(&mut g, M_SKELETON);
        assert!(g.p.max_hp >= 1 && g.p.hp >= 1);
    }

    #[test]
    fn thralls_rise_to_champion_and_overlord() {
        let mut champs = 0;
        let trials = 1000;
        for _ in 0..trials {
            let mut g = game();
            let t = thrall(&mut g, M_SKELETON);
            let tid = g.level.monsters[t].id;
            let (x, y) = floor_away(&mut g, 3);
            let v = spawn(&mut g, M_GOBLIN, x, y);
            g.damage_monster(v, 999, Src::Mon(tid));
            let t = g.mon_index(tid).unwrap();
            if g.level.monsters[t].champion {
                champs += 1;
            }
        }
        // 5% at level 1.
        assert!((20..=90).contains(&champs), "{} champions", champs);

        // A champion thrall that slays a champion may become an overlord.
        let mut lords = 0;
        for _ in 0..1000 {
            let mut g = game();
            g.p.level = 20; // 1.5% + 4 x 0.5% = 3.5%
            let t = thrall(&mut g, M_ORC);
            g.make_champion(t);
            let tid = g.level.monsters[t].id;
            let (x, y) = floor_away(&mut g, 3);
            let v = spawn(&mut g, M_ORC, x, y);
            g.make_champion(v);
            g.damage_monster(v, 9999, Src::Mon(tid));
            let t = g.mon_index(tid).unwrap();
            if g.level.monsters[t].overlord {
                lords += 1;
            }
        }
        assert!((10..=70).contains(&lords), "{} overlords", lords);
    }

    #[test]
    fn overlord_thralls_roar_and_raise_minions() {
        let mut g = game();
        let t = thrall(&mut g, M_ORC);
        g.make_overlord(t);
        assert!(g.level.monsters[t].overlord);
        let (tx, ty) = (g.level.monsters[t].x, g.level.monsters[t].y);
        let mut foes = Vec::new();
        for _ in 0..2 {
            let (x, y) = g.free_adjacent(tx, ty).unwrap();
            let f = spawn(&mut g, M_GOBLIN, x, y);
            foes.push(g.level.monsters[f].id);
        }
        // First the roar.
        assert!(g.thrall_power(t));
        for id in &foes {
            let f = g.mon_index(*id).unwrap();
            assert!((5..=20).contains(&g.level.monsters[f].fear));
        }
        let cd = (g.level.monsters[t].roar_cd - g.time) / 100.0;
        assert!((25.0..=50.0).contains(&cd));
        // Then the minions, which crumble after 10 turns.
        let before = g.level.monsters.len();
        assert!(g.thrall_power(t));
        let minions: Vec<u32> = g.level.monsters[before..].iter().map(|m| m.id).collect();
        assert_eq!(minions.len(), 2);
        assert!(g.level.monsters[before..]
            .iter()
            .all(|m| m.faction == Faction::Ally && !m.thrall && m.expires > 0.0));
        let upkeep = g.p.thrall_upkeep;
        g.sync_upkeep();
        assert_eq!(g.p.thrall_upkeep, upkeep, "minions cost nothing");
        assert!(!g.thrall_power(t), "both powers are cooling down");
        g.time += 1001.0;
        g.expire_minions();
        assert!(minions.iter().all(|id| g.mon_index(*id).is_none()));

        // An overlord thrall carries no treasure.
        let items = g.level.items.len();
        let t = g.mon_index(g.level.monsters[t].id).unwrap();
        g.damage_monster(t, 99999, Src::Player);
        assert_eq!(g.level.items.len(), items);
    }

    #[test]
    fn thralls_heal_slower_than_you() {
        let mut g = game();
        let t = thrall(&mut g, M_ORC);
        let tid = g.level.monsters[t].id;
        g.level.monsters[t].hp = 1;
        g.p.hp = 1;
        g.p.max_hp = 1000;
        g.p.thrall_upkeep = 0;
        g.sync_upkeep();
        for _ in 0..60 {
            g.turn_tick();
        }
        let t = g.mon_index(tid).unwrap();
        let healed = g.level.monsters[t].hp - 1;
        assert!(
            healed > 0 && healed < g.p.hp - 1,
            "thrall {} vs you {}",
            healed,
            g.p.hp - 1
        );
    }

    #[test]
    fn new_monsters_are_complete_and_brawl() {
        assert_eq!(
            MONSTER_LORE.len(),
            MONSTERS.len(),
            "every monster has a bestiary entry"
        );
        let mut order = bestiary_order();
        order.sort();
        assert_eq!(order, (0..MONSTERS.len()).collect::<Vec<_>>());
        let first_new = M_UNIQUE0 + 10;
        assert!(MONSTERS.len() >= first_new + 13);
        for k in first_new..MONSTERS.len() {
            let d = &MONSTERS[k];
            assert!(!d.art.is_empty() && d.unique == 0 && !d.boss, "{}", d.name);
            // Find something it hates and drop the two side by side, out of your sight.
            let foe = (0..first_new)
                .find(|&o| {
                    MONSTERS[o].unique == 0
                        && !MONSTERS[o].boss
                        && hostile(d.faction, MONSTERS[o].faction)
                })
                .unwrap_or_else(|| panic!("{} has no enemies", d.name));
            let mut g = game();
            g.p.invis = 1000;
            let (x, y) = floor_away(&mut g, 12);
            let a = spawn(&mut g, k, x, y);
            let (bx, by) = g.free_adjacent(x, y).expect("room");
            let b = spawn(&mut g, foe, bx, by);
            let bid = g.level.monsters[b].id;
            g.monster_act(a);
            assert_eq!(
                g.level.monsters[a].target,
                Some(Target::Mon(bid)),
                "{} ignores a {}",
                d.name,
                MONSTERS[foe].name
            );
        }
    }

    #[test]
    fn weak_monsters_crowd_the_upper_floors() {
        let mut rng = StdRng::seed_from_u64(3);
        let avg_xp = |depth: u32, rng: &mut StdRng| {
            let kinds: Vec<usize> = (0..MONSTERS.len())
                .filter(|&i| spawn_weight(&MONSTERS[i], depth) > 0.0)
                .collect();
            let n = 2000;
            (0..n)
                .map(|_| MONSTERS[pick_kind(&kinds, depth, rng)].xp as f32)
                .sum::<f32>()
                / n as f32
        };
        let mut last = 0.0;
        for depth in [1, 3, 5, 7, 9] {
            let a = avg_xp(depth, &mut rng);
            assert!(a > last, "depth {} average xp {} after {}", depth, a, last);
            last = a;
        }
        // Within a floor, a monster at the end of its range is rarer than at home.
        let skel = &MONSTERS[M_SKELETON];
        assert!(spawn_weight(skel, 7) < spawn_weight(skel, 4));
        // The new monsters really turn up.
        let mut seen = std::collections::HashSet::new();
        for depth in 1..=10 {
            for _ in 0..6 {
                let mut lv = generate(depth, &mut rng);
                let mut id = 1;
                populate(&mut lv, &mut rng, &mut id, 0, 0.0, false, 1.0);
                seen.extend(lv.monsters.iter().map(|m| m.kind));
            }
        }
        for k in (M_UNIQUE0 + 10)..MONSTERS.len() {
            if k == M_GUARDIAN {
                continue; // only ever torn out of a ward prism
            }
            assert!(seen.contains(&k), "{} never spawned", MONSTERS[k].name);
        }
    }

    #[test]
    fn the_log_keeps_a_hundred_turns() {
        let mut g = game();
        g.msgs.clear();
        for t in 0..250u64 {
            g.turns = t;
            g.msg(format!("turn {}", t), GREY);
            g.msg(format!("again {}", t), GREY);
        }
        let oldest = g.msgs.first().unwrap().turn;
        assert!(oldest >= 149 && oldest <= 150, "oldest kept {}", oldest);
        assert_eq!(g.msgs.last().unwrap().text, "again 249");
        assert!(g.msgs.windows(2).all(|w| w[0].turn <= w[1].turn));
    }

    #[test]
    fn a_gale_snuffs_the_torch() {
        let mut g = game();
        g.p.has_amulet = true;
        g.start_curse(Curse::Gale, 3);
        // No other curse may wander in and muddy the test.
        g.p.has_amulet = false;
        assert!(g.p.torch_out);
        assert_eq!(g.sight(), 0);
        assert_eq!(
            g.level.visible.iter().filter(|&&v| v).count(),
            1,
            "you see only where you stand"
        );
        // The wind tears the smoke sideways.
        for _ in 0..30 {
            g.update(0.03, false);
        }
        assert!(g.smoke.iter().any(|s| s.x > 3.0));
        // You cannot relight it while the wind howls...
        g.do_action(Action::Relight);
        assert!(g.p.torch_out);
        for _ in 0..3 {
            g.tick_curses();
        }
        assert!(!g.cursed(Curse::Gale));
        // ...and it stays out until you do.
        assert!(g.p.torch_out);
        g.do_action(Action::Relight);
        assert!(!g.p.torch_out);
        assert!(g.sight() > 2);
        assert!(g.level.visible.iter().filter(|&&v| v).count() > 1);
        // A lit torch smokes steadily.
        g.smoke.clear();
        for _ in 0..20 {
            g.update(0.05, false);
        }
        assert!(!g.smoke.is_empty() && g.smoke.iter().all(|s| s.y < 3.0));
    }

    fn necro() -> Game {
        let mut g = Game::new(
            C_NECRO,
            Bonuses {
                xp_mult: 1.0,
                ..Default::default()
            },
            0,
        );
        g.level.monsters.clear();
        g.level.corpses.clear();
        g.p.max_hp = 500;
        g.p.hp = 500;
        g.p.mp = 500;
        g.p.max_mp = 500;
        g
    }

    fn corpse_near(g: &mut Game, kind: usize) -> (i32, i32) {
        for r in 1..4 {
            for dy in -r..=r {
                for dx in -r..=r {
                    let (x, y) = (g.p.x + dx, g.p.y + dy);
                    if g.level.tile(x, y).walkable()
                        && (x, y) != (g.p.x, g.p.y)
                        && !g.level.corpses.iter().any(|c| (c.x, c.y) == (x, y))
                    {
                        g.level.corpses.push(Corpse {
                            kind,
                            x,
                            y,
                            revive_at: -1.0,
                            raises: 0,
                        });
                        return (x, y);
                    }
                }
            }
        }
        panic!("no room for a corpse");
    }

    #[test]
    fn raising_offers_a_choice_and_fallen_thralls_stay_down() {
        let mut g = necro();
        corpse_near(&mut g, 0);
        let (ox, oy) = corpse_near(&mut g, M_ORC);
        let mp = g.p.mp;
        assert!(
            g.use_skill(Skill::RaiseDead).is_none(),
            "two bodies: ask first"
        );
        assert_eq!(g.raise_menu.len(), 2);
        assert_eq!(g.p.mp, mp, "nothing is spent until you choose");
        let orc_opt = g
            .raise_menu
            .iter()
            .find(|o| o.kind == M_ORC)
            .copied()
            .unwrap();
        assert_eq!(orc_opt.cost, g.thrall_cost(M_ORC, false));
        let max = g.p.max_hp;
        g.do_action(Action::RaiseAt(ox, oy));
        assert!(g.raise_menu.is_empty());
        assert!(g.p.mp < mp);
        let ti = g.level.monsters.iter().position(|m| m.thrall).unwrap();
        assert_eq!(g.level.monsters[ti].kind, M_ORC);
        assert_eq!(g.p.max_hp, max - orc_opt.cost);

        // The orc falls: its body will never answer you again.
        let (fx, fy) = floor_away(&mut g, 4);
        let foe = spawn(&mut g, M_ORC, fx, fy);
        let fid = g.level.monsters[foe].id;
        g.damage_monster(ti, 9999, Src::Mon(fid));
        let c = g.level.corpses.iter().find(|c| c.kind == M_ORC).unwrap();
        assert_eq!(c.raises, 1);
        assert!(c.revive_at < 0.0, "it will not rise on its own either");
        assert!(g.raise_options().iter().all(|o| o.kind != M_ORC));

        // Undying Bond allows one more raising, at a higher price.
        g.bonus.rebind = true;
        // The killer has done its job; keep it from interfering with the rest.
        if let Some(fi) = g.mon_index(fid) {
            g.level.monsters[fi].alive = false;
        }
        let again = g
            .raise_options()
            .into_iter()
            .find(|o| o.kind == M_ORC)
            .unwrap();
        assert!(again.again && again.cost > orc_opt.cost);
        g.p.cds.clear();
        g.do_action(Action::RaiseAt(again.x, again.y));
        let ti = g
            .level
            .monsters
            .iter()
            .position(|m| m.thrall && m.alive && m.kind == M_ORC)
            .unwrap();
        assert_eq!(g.level.monsters[ti].raises, 2);
        g.damage_monster(ti, 9999, Src::Mon(fid));
        assert!(
            g.raise_options().iter().all(|o| o.kind != M_ORC),
            "twice is the limit"
        );

        // A single body needs no menu.
        let mut g = necro();
        corpse_near(&mut g, 0);
        assert!(g.use_skill(Skill::RaiseDead).is_some());
        assert!(g.raise_menu.is_empty());
        assert!(g.level.monsters.iter().any(|m| m.thrall));
    }

    /// An open hall to test abilities in, with the player in the middle.
    fn arena() -> Game {
        let mut g = game();
        for y in 2..14 {
            for x in 2..40 {
                g.level.set(x, y, Tile::Floor);
            }
        }
        g.level.traps.clear();
        g.level.corpses.clear();
        g.level.items.clear();
        g.p.x = 20;
        g.p.y = 8;
        g.refresh_view();
        g
    }

    fn kind_of(name: &str) -> usize {
        MONSTERS.iter().position(|m| m.name == name).unwrap()
    }

    /// Keep giving the monster a turn until it uses a power.
    fn trigger(g: &mut Game, mi: usize, t: Target) -> bool {
        for _ in 0..400 {
            g.level.monsters[mi].abil_cd = 0.0;
            g.p.para_guard = 0.0;
            if g.unique_power(mi, t) {
                return true;
            }
        }
        false
    }

    #[test]
    fn every_monster_has_a_power() {
        for m in MONSTERS.iter() {
            assert!(!m.abil.is_empty(), "{} has no special ability", m.name);
        }
    }

    #[test]
    fn monster_powers_work() {
        // Minotaur: charges from afar and hurls you back.
        let mut g = arena();
        let mi = spawn(&mut g, kind_of("minotaur"), 15, 8);
        let mut knocked = false;
        for _ in 0..200 {
            g.level.monsters[mi].x = 15;
            g.level.monsters[mi].y = 8;
            g.p.x = 20;
            g.p.y = 8;
            g.p.hp = 10_000;
            if trigger(&mut g, mi, Target::Player) && g.p.x > 20 {
                knocked = true;
                break;
            }
        }
        assert!(knocked, "the charge should knock you back");

        // Wraith: saps your life to heal itself.
        let mut g = arena();
        let mi = spawn(&mut g, kind_of("wraith"), 17, 8);
        g.level.monsters[mi].hp = 5;
        let hp = g.p.hp;
        assert!(trigger(&mut g, mi, Target::Player));
        assert!(g.p.hp < hp && g.level.monsters[mi].hp > 5);

        // Myconid: spores paralyse whoever walks into them.
        let mut g = arena();
        let mi = spawn(&mut g, kind_of("myconid"), 22, 8);
        assert!(trigger(&mut g, mi, Target::Player));
        assert!(!g.level.clouds.is_empty());
        g.p.para_guard = 0.0;
        g.p.slow = 0.0;
        g.p.x = 21;
        g.after_player_move();
        assert!(g.p.slow >= 150.0, "the spores seize you");
        // ...but never twice in a row.
        assert!(!g.stagger(Target::Player, 100.0));
        g.time += 1000.0;
        g.tick_clouds();
        assert!(g.level.clouds.is_empty(), "the clouds disperse");

        // Spider: webs root you in place, but you can still fight.
        let mut g = arena();
        let mi = spawn(&mut g, kind_of("giant spider"), 24, 8);
        assert!(trigger(&mut g, mi, Target::Player));
        assert!(g.p.rooted > 0);
        assert!(g.player_move(4).is_some());
        assert_eq!((g.p.x, g.p.y), (20, 8));

        // Slime: splits in two when wounded.
        let mut g = arena();
        let mi = spawn(&mut g, kind_of("green slime"), 21, 8);
        g.level.monsters[mi].hp = 10;
        assert!(trigger(&mut g, mi, Target::Player));
        assert_eq!(
            g.level
                .monsters
                .iter()
                .filter(|m| m.alive && m.kind == kind_of("green slime"))
                .count(),
            2
        );

        // Ogre: the quake hits everything close by, friend or foe.
        let mut g = arena();
        let mi = spawn(&mut g, kind_of("ogre"), 21, 8);
        let gi = spawn(&mut g, M_GOBLIN, 22, 9);
        g.level.monsters[gi].hp = 500;
        let hp = g.p.hp;
        let mut ok = false;
        for _ in 0..50 {
            if trigger(&mut g, mi, Target::Player) && g.level.monsters[gi].hp < 500 {
                ok = true;
                break;
            }
        }
        assert!(ok && g.p.hp < hp);

        // Skeleton: raises its guard (+4 armour).
        let mut g = arena();
        let mi = spawn(&mut g, M_SKELETON, 21, 8);
        let ac = g.level.monsters[mi].ac();
        assert!(trigger(&mut g, mi, Target::Player));
        assert_eq!(g.level.monsters[mi].ac(), ac + 4);

        // Goblin shaman: mends a wounded goblin.
        let mut g = arena();
        g.p.x = 2;
        g.p.y = 2;
        let si = spawn(&mut g, kind_of("goblin shaman"), 30, 8);
        let gi = spawn(&mut g, M_GOBLIN, 32, 8);
        g.level.monsters[gi].hp = 1;
        assert!(trigger(&mut g, si, Target::Player));
        assert!(g.level.monsters[gi].hp > 1);

        // Goblin: grit in the eyes.
        let mut g = arena();
        let mi = spawn(&mut g, M_GOBLIN, 22, 8);
        assert!(trigger(&mut g, mi, Target::Player));
        assert!(g.p.blind > 0 && g.sight() <= 1);

        // Rats call their kin.
        let mut g = arena();
        let mi = spawn(&mut g, 0, 23, 8);
        let ki = spawn(&mut g, 0, 30, 10);
        g.level.monsters[ki].awake = false;
        assert!(trigger(&mut g, mi, Target::Player));
        assert!(g.level.monsters[ki].awake);
        assert_eq!(g.level.monsters[ki].target, Some(Target::Player));

        // Powers work in monster infighting too: a basilisk's gaze stuns a rival.
        let mut g = arena();
        let bi = spawn(&mut g, kind_of("basilisk"), 30, 8);
        let oi = spawn(&mut g, M_GOBLIN, 33, 8);
        let oid = g.level.monsters[oi].id;
        assert!(trigger(&mut g, bi, Target::Mon(oid)));
        assert!(g.level.monsters[oi].stun > 0);

        // The Warden shakes the ground too.
        let mut g = arena();
        let wi = spawn(&mut g, M_WARDEN, 21, 8);
        assert!(trigger(&mut g, wi, Target::Player));
    }

    #[test]
    fn rooms_have_purposes() {
        let mut rng = StdRng::seed_from_u64(11);
        let mut kinds = std::collections::HashSet::new();
        for depth in 1..=9 {
            for _ in 0..8 {
                let mut lv = generate(depth, &mut rng);
                let mut id = 1;
                populate(&mut lv, &mut rng, &mut id, 0, 0.0, false, 1.0);
                // Everything walkable can still be reached.
                let dm = lv.dijkstra(lv.up.0, lv.up.1);
                assert!(lv
                    .tiles
                    .iter()
                    .enumerate()
                    .all(|(i, t)| !t.walkable() || dm[i] < UNREACH));
                for f in &lv.features {
                    kinds.insert(format!("{:?}", f.kind));
                    let r = f.room;
                    let inside = |x: i32, y: i32| r.contains(x, y);
                    match f.kind {
                        RoomKind::Armory => assert!(lv.items.iter().any(|i| inside(i.x, i.y))),
                        RoomKind::Library => assert!(lv
                            .items
                            .iter()
                            .any(|i| inside(i.x, i.y) && matches!(i.item, Item::Scroll(_)))),
                        RoomKind::Ossuary => assert!(lv.corpses.iter().any(|c| inside(c.x, c.y))),
                        RoomKind::Garden => assert!(lv.clouds.iter().any(|c| inside(c.x, c.y))),
                        RoomKind::Chamber => assert!(lv.tiles.iter().any(|&t| t.is_closed_door())),
                        _ => {}
                    }
                }
            }
        }
        assert!(kinds.len() >= 9, "only saw {:?}", kinds);
        // Walking in announces the room.
        let mut g = game();
        let f = Feature {
            kind: RoomKind::Armory,
            room: Rect {
                x: g.p.x - 1,
                y: g.p.y - 1,
                w: 3,
                h: 3,
            },
            seen: false,
        };
        g.level.features.push(f);
        g.refresh_view();
        assert!(g.msgs.iter().any(|m| m.text.contains("armory")));
    }

    #[test]
    fn the_living_dungeon_fights_back() {
        let mut g = game();
        g.p.has_amulet = true;
        g.depth = 2;
        // Sinew and maws on arrival.
        g.infest_level();
        assert!(g.level.traps.iter().any(|t| t.kind == TrapKind::Maw));
        assert!(!g.level.sinews.is_empty());
        for s in &g.level.sinews {
            assert_eq!(g.level.tile(s.x, s.y), Tile::Sinew);
        }
        // Sinew blocks the way, and takes a few cuts to get through.
        let mut g = arena();
        g.p.has_amulet = true;
        g.depth = 2;
        g.grow_sinew(21, 8);
        assert!(g.level.tile(21, 8).opaque() && !g.level.tile(21, 8).walkable());
        let mut cuts = 0;
        while g.level.tile(21, 8) == Tile::Sinew && cuts < 50 {
            g.player_move(2);
            cuts += 1;
        }
        assert_eq!(g.level.tile(21, 8), Tile::Floor);
        assert!(cuts >= 1);
        assert_eq!((g.p.x, g.p.y), (20, 8), "cutting does not move you");
        // Maws bite and hold.
        let mut g = arena();
        g.level.traps.push(Trap {
            x: 21,
            y: 8,
            kind: TrapKind::Maw,
            revealed: false,
        });
        let hp = g.p.hp;
        g.player_move(2);
        assert!(g.p.hp < hp && g.p.rooted > 0);
        // Growth happens over time while you carry the heart.
        let mut g = arena();
        g.p.has_amulet = true;
        g.depth = 1;
        for y in 2..14 {
            if y != 8 {
                for x in 22..24 {
                    g.level.set(x, y, Tile::Wall);
                }
            }
        }
        g.refresh_view();
        for _ in 0..2000 {
            g.living_dungeon_tick();
            if !g.level.sinews.is_empty() {
                break;
            }
        }
        assert!(
            !g.level.sinews.is_empty(),
            "the dungeon grows sinew across your path"
        );
    }

    #[test]
    fn wall_eyes_are_whole_and_stare() {
        // An eye is only drawn where the flesh is solid all the way round it.
        let c = 0.9;
        let mut eyes = 0;
        for my in 0..40 {
            for mx in 0..60 {
                if !crate::render::wall_eye(mx, my, c) {
                    continue;
                }
                eyes += 1;
                for k in 0..24 {
                    let a = k as f32 / 24.0 * std::f32::consts::TAU;
                    for r in [0.0f32, 0.4, 0.8, 0.98] {
                        let (u, v) = (0.5 + a.cos() * 0.14 * r, 0.45 + a.sin() * 0.09 * r);
                        let n = crate::color::vnoise(
                            u * 2.5 + mx as f32 * 2.5,
                            v * 2.5 + my as f32 * 1.9,
                        );
                        let cover = c * 1.3 - crate::color::hashf(mx * 7 + 3, my * 11 + 5) * 0.6;
                        assert!(
                            n <= cover - 0.05,
                            "eye at {},{} has a hole at {},{}",
                            mx,
                            my,
                            u,
                            v
                        );
                    }
                }
            }
        }
        assert!(eyes > 20, "only {} eyes", eyes);
        // A visible eye stares you into stillness.
        let mut g = game();
        g.p.has_amulet = true;
        g.depth = 1;
        let c = g.corruption();
        let mut spot = None;
        'find: for y in 1..g.level.h - 1 {
            for x in 1..g.level.w - 1 {
                if g.level.tile(x, y) == Tile::Wall && crate::render::wall_eye(x, y, c) {
                    for &(dx, dy) in DIRS.iter() {
                        if g.level.tile(x + dx, y + dy) == Tile::Floor {
                            spot = Some((x + dx, y + dy));
                            break 'find;
                        }
                    }
                }
            }
        }
        let (sx, sy) = spot.expect("some eye borders a floor");
        g.p.x = sx;
        g.p.y = sy;
        g.refresh_view();
        let mut stared = false;
        for _ in 0..2000 {
            g.stare_cd = 0.0;
            g.p.para_guard = 0.0;
            g.living_dungeon_tick();
            if g.stare.is_some() {
                stared = true;
                break;
            }
        }
        assert!(stared);
        assert!(g.p.slow >= 150.0);
    }

    #[test]
    fn the_torch_puffs_out_and_sparks_back() {
        let mut g = game();
        g.p.has_amulet = true;
        g.start_curse(Curse::Gale, 2);
        g.p.has_amulet = false;
        g.smoke.clear();
        for _ in 0..12 {
            g.update(0.03, false);
        }
        assert!(g.smoke.len() >= 30, "a puff of smoke as the flame dies");
        for _ in 0..2 {
            g.tick_curses();
        }
        g.do_action(Action::Relight);
        assert!(!g.p.torch_out);
        assert!(g.light_age < 0.05, "the flint sparks start now");
        assert!(g.msgs.iter().any(|m| m.text.contains("flint")));
    }

    #[test]
    fn gates_can_be_seen_through() {
        // Sewers, gaols, crypts and foundries get gates of bars.
        let mut rng = StdRng::seed_from_u64(5);
        let mut lv = generate(2, &mut rng);
        lv.style = 8;
        lv.set(5, 5, Tile::DoorClosed);
        lv.theme_doors();
        assert!(lv.tiles.iter().all(|&t| t != Tile::DoorClosed));
        assert_eq!(lv.tile(5, 5), Tile::GateClosed);
        // Other themes keep solid doors.
        let mut lv = generate(2, &mut rng);
        lv.style = 0;
        lv.set(5, 5, Tile::DoorClosed);
        lv.theme_doors();
        assert_eq!(lv.tile(5, 5), Tile::DoorClosed);

        let mut g = arena();
        for y in 2..14 {
            g.level.set(23, y, Tile::Wall);
        }
        g.level.set(23, 8, Tile::GateClosed);
        g.refresh_view();
        // You can see through the bars...
        assert!(g.level.is_visible(26, 8));
        assert!(g.level.los(20, 8, 26, 8));
        // ...but not walk through until it is opened.
        g.p.x = 22;
        g.player_move(2);
        assert_eq!((g.p.x, g.p.y), (22, 8));
        assert_eq!(g.level.tile(23, 8), Tile::DoorOpen);
        assert!(g.msgs.iter().any(|m| m.text.contains("gate")));
        // A closed gate blocks squeezing diagonally past its corner.
        g.level.set(23, 8, Tile::GateClosed);
        g.level.set(23, 9, Tile::Floor);
        assert!(!g.level.can_step(22, 8, 1, 1));
    }

    #[test]
    fn nothing_leaps_through_a_closed_gate() {
        // Charging, pouncing and blinking monsters see you through the bars,
        // but must open the gate before they can reach you.
        // (Ghosts are the exception: they drift through solid stone.)
        for name in ["minotaur", "dire wolf", "Xal'Veth the Drowned", "Asterion the Tormentor"] {
            let mut g = arena();
            for y in 2..14 {
                g.level.set(23, y, Tile::Wall);
            }
            g.level.set(23, 8, Tile::GateClosed);
            g.refresh_view();
            let mi = spawn(&mut g, kind_of(name), 26, 8);
            for _ in 0..300 {
                g.level.set(23, 8, Tile::GateClosed);
                let m = &mut g.level.monsters[mi];
                m.x = 26;
                m.y = 8;
                m.abil_cd = 0.0;
                g.p.x = 20;
                g.p.y = 8;
                g.p.hp = 10_000;
                g.p.para_guard = 0.0;
                g.unique_power(mi, Target::Player);
                assert!(
                    g.level.monsters[mi].x >= 23,
                    "{} got through a closed gate",
                    name
                );
            }
            // With the gate open the way is clear again.
            assert!(!g.level.clear_walk(26, 8, 21, 8));
            g.level.set(23, 8, Tile::DoorOpen);
            assert!(g.level.clear_walk(26, 8, 21, 8));
        }
        // Walking up to it, a monster opens the gate and only then comes on.
        let mut g = arena();
        for y in 2..14 {
            g.level.set(23, y, Tile::Wall);
        }
        g.level.set(23, 8, Tile::GateClosed);
        g.refresh_view();
        let mi = spawn(&mut g, kind_of("orc"), 24, 8);
        g.level.monsters[mi].target = Some(Target::Player);
        g.monster_step_toward(mi, 20, 8, true);
        assert_eq!(g.level.tile(23, 8), Tile::DoorOpen, "it opens the gate");
        assert_eq!(g.level.monsters[mi].x, 24, "and stays put that turn");
    }

    #[test]
    fn sleepers_snore_until_they_notice_you() {
        let mut g = arena();
        let mi = spawn(&mut g, kind_of("orc"), 24, 8);
        g.level.monsters[mi].awake = false;
        g.level.monsters[mi].target = None;
        g.p.facing = 2;
        g.cam.x = 20.5;
        g.cam.y = 8.5;
        g.cam.ang = 0.0;
        let zs = |g: &Game| {
            let mut c = crate::canvas::Canvas::new(120, 40);
            let mut n = 0;
            for k in 0..12 {
                c.clear();
                crate::render::render_game(&mut c, g, k as f32 * 0.3);
                n += c.cells.iter().filter(|c| c.ch == 'z' || c.ch == 'Z').count();
            }
            n
        };
        // Other text on screen may hold the letter z; compare with no orc.
        let asleep = zs(&g);
        g.level.monsters[mi].awake = true;
        g.level.monsters[mi].target = Some(Target::Player);
        let awake = zs(&g);
        g.level.monsters[mi].alive = false;
        let none = zs(&g);
        assert!(asleep > none, "a sleeping orc snores");
        assert_eq!(awake, none, "awake, the Zs are gone");
    }

    /// Walls in an arena, splitting it into rooms joined by gaps.
    fn rooms() -> Game {
        let mut g = arena();
        for y in 2..14 {
            g.level.set(25, y, Tile::Wall);
        }
        g.level.set(25, 3, Tile::Floor);
        g.refresh_view();
        g
    }

    /// A level of solid rock with these rectangles (inclusive) carved out.
    fn carved(rects: &[(i32, i32, i32, i32)], px: i32, py: i32) -> Game {
        let mut g = game();
        for y in 0..g.level.h {
            for x in 0..g.level.w {
                g.level.set(x, y, Tile::Wall);
            }
        }
        for &(x0, y0, x1, y1) in rects {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    g.level.set(x, y, Tile::Floor);
                }
            }
        }
        g.level.traps.clear();
        g.level.items.clear();
        g.level.corpses.clear();
        g.level.monsters.clear();
        g.p.x = px;
        g.p.y = py;
        g.refresh_view();
        g
    }

    #[test]
    fn corners_and_doors_muffle_sound() {
        // Ten tiles straight down a corridor...
        let g = carved(&[(10, 5, 20, 5)], 10, 5);
        let straight = g.heard_from(20, 5).unwrap();
        // ...or ten tiles that turn a corner halfway.
        let g = carved(&[(10, 5, 15, 5), (15, 5, 15, 10)], 10, 5);
        let around = g.heard_from(15, 10).unwrap();
        assert!(around > straight, "a corner muffles ({} vs {})", around, straight);
        // A closed door in the way muffles more.
        let mut g = carved(&[(10, 5, 20, 5)], 10, 5);
        g.level.set(15, 5, Tile::DoorClosed);
        g.refresh_view();
        assert_eq!(g.heard_from(20, 5), None, "muffled past the edge of hearing");
        g.bonus.hearing = 1;
        g.refresh_view();
        assert!(g.heard_from(20, 5).unwrap() >= straight + 3);
        // And where it seems to come from is the corner, not through the rock.
        let g = carved(&[(10, 5, 15, 5), (15, 5, 15, 10)], 10, 5);
        let (ax, ay) = g.sound_corner(15, 10);
        assert!(g.level.los(10, 5, ax as i32, ay as i32));
        assert_eq!(ax as i32, 15);
    }

    #[test]
    fn listened_sound_fills_its_room_then_runs_toward_you() {
        // You at the west end of a corridor; a hall to the east with the
        // sound in its far corner; a dead-end passage off the hall's south side.
        let mut g = carved(&[(20, 7, 29, 7), (30, 3, 38, 9), (31, 10, 31, 14)], 20, 7);
        g.bonus.hearing = 2;
        spawn(&mut g, kind_of("orc"), 37, 4);
        g.listen();
        assert_eq!(g.pings.len(), 1);
        let wave = &g.pings[0].wave;
        let has = |x: i32, y: i32| wave.iter().any(|&(wx, wy, _)| (wx, wy) == (x, y));
        assert!(has(36, 4) && has(33, 6), "it fills the hall around it");
        assert!(has(37, 2) || has(38, 2) || has(39, 4), "and rings off the hall's walls");
        for x in 21..=29 {
            assert!(has(x, 7), "and runs down the corridor toward you ({} missing)", x);
        }
        assert!(!has(31, 14), "but not down the dead end away from you");
        // Further along the way, the sound has come further.
        let at = |x: i32, y: i32| wave.iter().find(|&&(wx, wy, _)| (wx, wy) == (x, y)).unwrap().2;
        assert!(at(22, 7) > at(28, 7) && at(28, 7) > at(36, 4));
    }

    #[test]
    fn keen_hearing_costs_and_ranges() {
        let mut prof = crate::profile::Profile::default();
        let i = UPGRADES.iter().position(|u| u.id == "hearing").unwrap();
        assert_eq!(prof.next_cost(i), Some(450));
        prof.upgrades.insert("hearing".into(), 1);
        assert_eq!(prof.next_cost(i), Some(750));
        prof.upgrades.insert("hearing".into(), 2);
        assert_eq!(prof.next_cost(i), None);
        assert_eq!(prof.bonuses().hearing, 2);
        let mut g = game();
        let base = g.hear_radius();
        g.bonus.hearing = 1;
        assert!(g.hear_radius() > base);
        assert_eq!(compass(5.0, 0.0), "to the east");
        assert_eq!(compass(0.0, -5.0), "to the north");
        assert_eq!(compass(-4.0, 4.0), "to the south-west");
    }

    #[test]
    fn unseen_fights_are_heard_not_seen() {
        let mut g = rooms();
        // Two foes fighting behind the wall, close by along the corridor.
        let a = spawn(&mut g, kind_of("orc"), 27, 6);
        let b = spawn(&mut g, kind_of("dire wolf"), 28, 6);
        assert!(!g.level.is_visible(27, 6));
        let bid = g.level.monsters[b].id;
        g.level.monsters[b].hp = 10_000;
        g.turns = 1000;
        g.msgs.clear();
        for _ in 0..40 {
            g.monster_melee(a, Target::Mon(bid));
        }
        let heard: Vec<_> = g.msgs.iter().filter(|m| m.text.contains("sounds of a fight")).collect();
        assert!(!heard.is_empty(), "a fight close by is heard");
        assert!(heard.iter().all(|m| !m.text.contains("orc") && !m.text.contains("wolf")), "but not who");
        assert!(!g.msgs.iter().any(|m| m.text.contains("hits the dire wolf")), "or seen");
        // Far off, nothing at all.
        g.level.monsters.clear();
        let (fx, fy) = (38, 13);
        g.level.set(fx, fy, Tile::Floor);
        for y in 2..14 {
            g.level.set(37, y, Tile::Wall);
        }
        g.refresh_view();
        let a = spawn(&mut g, kind_of("orc"), 38, 12);
        let b = spawn(&mut g, kind_of("dire wolf"), 38, 13);
        let bid = g.level.monsters[b].id;
        g.level.monsters[b].hp = 10_000;
        if g.heard_from(38, 12).is_none() {
            g.msgs.clear();
            g.turns += 1000;
            for _ in 0..40 {
                g.monster_melee(a, Target::Mon(bid));
            }
            assert!(!g.msgs.iter().any(|m| m.text.contains("sounds of a fight")));
        }
    }

    #[test]
    fn listening_picks_out_hidden_sounds_once_in_a_while() {
        let mut g = rooms();
        let hidden = spawn(&mut g, kind_of("orc"), 28, 8);
        let seen = spawn(&mut g, kind_of("goblin"), 22, 8);
        assert!(!g.level.is_visible(28, 8) && g.level.is_visible(22, 8));
        let _ = (hidden, seen);
        // Without rank 2 you cannot.
        g.bonus.hearing = 1;
        assert_eq!(g.listen(), None);
        g.bonus.hearing = 2;
        assert_eq!(g.listen(), Some(100.0), "listening takes a turn");
        assert_eq!(g.pings.len(), 1, "only what you cannot see");
        let p = &g.pings[0];
        assert_eq!((p.x, p.y), (28, 8));
        let true_d = g.heard_from(28, 8).unwrap();
        assert!((p.dist - true_d).abs() <= 1, "a rough guess at the distance");
        assert!(g.level.los(g.p.x, g.p.y, p.ax as i32, p.ay as i32), "heard from the gap it comes through");
        assert!(p.wave.iter().any(|&(x, y, _)| g.level.tile(x, y) == Tile::Wall), "it rings off the walls");
        // Ten turn cooldown.
        assert_eq!(g.listen(), None);
        g.time += 900.0;
        assert_eq!(g.listen(), None);
        g.time += 100.0;
        assert_eq!(g.listen(), Some(100.0));
        // Pings fade.
        g.update(Ping::LIFE + 0.1, false);
        assert!(g.pings.is_empty());
        // A sleeper is heard only close by.
        g.level.monsters.clear();
        let m = spawn(&mut g, kind_of("orc"), 28, 8);
        g.level.monsters[m].awake = false;
        g.time += 1000.0;
        g.listen();
        let close = g.heard_from(28, 8).unwrap() <= g.hear_radius() / 2;
        assert_eq!(g.pings.len(), close as usize);
    }

    #[test]
    fn sound_pings_are_drawn_in_both_views() {
        let mut g = rooms();
        spawn(&mut g, kind_of("orc"), 28, 8);
        g.bonus.hearing = 2;
        g.listen();
        let label = format!("~{}", g.pings[0].dist);
        let mut c = crate::canvas::Canvas::new(120, 40);
        crate::render::render_game(&mut c, &g, 1.0);
        let text: String = c.cells.iter().map(|c| c.ch).collect();
        assert!(text.contains(&label), "first person: an arrow and a distance");
        g.overhead = true;
        g.pings[0].age = 0.3;
        c.clear();
        crate::render::render_game(&mut c, &g, 1.0);
        assert!(c.cells.iter().any(|c| c.ch == '~'), "overhead: ripples");
    }

    #[test]
    fn overhead_hides_effects_you_cannot_see() {
        let mut g = rooms();
        g.overhead = true;
        g.fx.text(30.5, 10.5, 0.5, "QX9".to_string(), Rgb::WHITE);
        g.fx.text(21.5, 8.5, 0.5, "QX7".to_string(), Rgb::WHITE);
        let mut c = crate::canvas::Canvas::new(120, 40);
        crate::render::render_game(&mut c, &g, 1.0);
        let text: String = c.cells.iter().map(|c| c.ch).collect();
        assert!(!text.contains("QX9"), "behind the wall: hidden");
        assert!(text.contains("QX7"), "in sight: shown");
    }

    #[test]
    fn switching_views_swoops_the_camera() {
        let mut g = game();
        assert!(!g.overhead);
        g.toggle_view();
        assert!(g.overhead, "the view changes at once for play");
        assert_eq!(g.view_anim.map(|a| a.1), Some(true));
        let mut c = crate::canvas::Canvas::new(100, 40);
        for _ in 0..30 {
            g.update(0.03, false);
            crate::render::render_game(&mut c, &g, 1.0);
        }
        assert!(g.view_anim.is_none(), "the swoop finishes");
        g.toggle_view();
        assert!(!g.overhead);
        assert_eq!(g.view_anim.map(|a| a.1), Some(false));
        for _ in 0..30 {
            g.update(0.03, false);
            crate::render::render_game(&mut c, &g, 1.0);
        }
        assert!(g.view_anim.is_none());
    }

    #[test]
    fn doors_open_in_their_own_way() {
        let cases = [
            (0u8, Tile::DoorClosed, DoorOpening::Swing, "creaks"),
            (8, Tile::GateClosed, DoorOpening::Rise, "rises"),
            (14, Tile::DoorClosed, DoorOpening::Split, "grind apart"),
            (13, Tile::DoorClosed, DoorOpening::Dissolve, "dissipates"),
            (5, Tile::DoorClosed, DoorOpening::Dissolve, "dissolves"),
        ];
        for (style, tile, kind, words) in cases {
            let mut g = arena();
            g.level.style = style;
            for y in 2..14 {
                g.level.set(23, y, Tile::Wall);
            }
            g.level.set(23, 8, tile);
            g.p.x = 22;
            g.player_move(2);
            // The way is open at once; only the picture takes its time.
            assert_eq!(g.level.tile(23, 8), Tile::DoorOpen);
            assert_eq!(g.fx.door_at(23, 8).map(|d| d.0), Some(kind));
            assert!(
                g.msgs.iter().any(|m| m.text.contains(words)),
                "style {}: {:?}",
                style,
                g.msgs.last().map(|m| &m.text)
            );
            let mut c = crate::canvas::Canvas::new(80, 30);
            for _ in 0..25 {
                g.update(0.03, false);
                crate::render::render_game(&mut c, &g, 1.0);
            }
            assert!(g.fx.door_at(23, 8).is_none(), "the animation finishes");
        }
    }

    #[test]
    fn only_you_and_your_thralls_can_touch_the_warden() {
        let mut g = arena();
        g.p.invis = 1000;
        let wi = spawn(&mut g, M_WARDEN, 30, 8);
        let oi = spawn(&mut g, M_ORC, 31, 8);
        let (wid, oid) = (g.level.monsters[wi].id, g.level.monsters[oi].id);
        let hp = g.level.monsters[wi].hp;
        // Monsters cannot hurt it, nor will they try.
        g.damage_monster(wi, 50, Src::Mon(oid));
        assert_eq!(g.level.monsters[wi].hp, hp);
        g.monster_act(oi);
        assert_ne!(g.level.monsters[oi].target, Some(Target::Mon(wid)));
        // It never goes after monsters itself.
        g.monster_act(wi);
        assert_ne!(g.level.monsters[wi].target, Some(Target::Mon(oid)));
        // You can hurt it, and so can a thrall.
        g.damage_monster(wi, 5, Src::Player);
        assert_eq!(g.level.monsters[wi].hp, hp - 5);
        let ti = thrall(&mut g, M_ORC);
        let tid = g.level.monsters[ti].id;
        g.damage_monster(wi, 5, Src::Mon(tid));
        assert_eq!(g.level.monsters[wi].hp, hp - 10);
    }

    #[test]
    fn wards_spawn_guardians_and_weaken_the_warden() {
        // One ward in each band of floors.
        let g = game();
        assert_eq!(g.ward_depths.len(), 4);
        for (d, band) in g.ward_depths.iter().zip([(1, 3), (4, 5), (6, 8), (9, 10)]) {
            assert!((band.0..=band.1).contains(d));
        }

        let mut g = arena();
        g.level.rooms = vec![
            Rect {
                x: 2,
                y: 2,
                w: 2,
                h: 2,
            },
            Rect {
                x: 14,
                y: 4,
                w: 12,
                h: 9,
            },
        ];
        for y in 3..14 {
            for x in 13..27 {
                let edge = x == 13 || x == 26 || y == 3 || y == 13;
                g.level
                    .set(x, y, if edge { Tile::Wall } else { Tile::Floor });
            }
        }
        g.level.set(13, 8, Tile::Floor); // a doorway
        g.level.set(26, 6, Tile::DoorClosed);
        g.level.up = (3, 3);
        g.level.down = (30, 3);
        place_ward(&mut g.level, &mut g.rng);
        let w = g.level.ward.expect("a ward");
        assert_eq!(g.level.tile(w.x, w.y), Tile::Ward);
        // Walk up and strike it: out comes the Guardian, and the room is sealed.
        let (px, py) = (w.x - 1, w.y);
        g.p.x = px;
        g.p.y = py;
        g.refresh_view();
        g.player_move(2);
        assert_eq!((g.p.x, g.p.y), (px, py), "the prism blocks the way");
        let gi = g
            .level
            .monsters
            .iter()
            .position(|m| m.kind == M_GUARDIAN && m.alive)
            .expect("a Guardian");
        assert_eq!(g.level.ward.unwrap().state, WardState::Shielded);
        assert_eq!(g.level.tile(13, 8), Tile::Crystal);
        assert_eq!(g.level.tile(26, 6), Tile::Crystal);
        assert!(g.level.monsters[gi].max_hp > 40, "a tough fight");
        // The shield holds while it lives.
        g.player_move(2);
        assert_eq!(g.level.ward.unwrap().state, WardState::Shielded);
        // Kill it: the ward crumbles and the walls fall.
        g.damage_monster(gi, 99_999, Src::Player);
        assert_eq!(g.level.ward.unwrap().state, WardState::Broken);
        assert_eq!(g.level.tile(w.x, w.y), Tile::Floor);
        assert_eq!(g.level.tile(13, 8), Tile::Floor);
        assert_eq!(g.level.tile(26, 6), Tile::DoorClosed);
        assert_eq!(g.wards_destroyed, 1);
        assert!(g.msgs.iter().any(|m| m.text.contains("scream")));

        // Each broken ward weakens the Warden; all four leave it feeble and powerless.
        let mut g = arena();
        let wi = spawn(&mut g, M_WARDEN, 30, 8);
        let full = g.level.monsters[wi].max_hp;
        let dmg = g.level.monsters[wi].dmg_mult();
        g.wards_destroyed = 2;
        g.weaken_wardens();
        assert!(g.level.monsters[wi].max_hp < full);
        g.wards_destroyed = 4;
        g.weaken_wardens();
        let m = &g.level.monsters[wi];
        assert_eq!(m.max_hp, (full as f32 * 0.3).round() as i32);
        assert!(m.dmg_mult() < dmg * 0.5);
        for _ in 0..50 {
            g.level.monsters[wi].abil_cd = 0.0;
            assert!(!g.unique_power(wi, Target::Player), "no powers left");
        }
    }

    #[test]
    fn taking_the_amulet_crumbles_the_remaining_wards() {
        let mut g = game();
        let mut rng = StdRng::seed_from_u64(4);
        let mut other = generate(5, &mut rng);
        place_ward(&mut other, &mut rng);
        assert!(other.ward.is_some());
        g.stash.insert(5, other);
        let (px, py) = (g.p.x, g.p.y);
        g.level.items.push(FloorItem {
            x: px,
            y: py,
            item: Item::Amulet,
        });
        let i = g.level.items.len() - 1;
        g.pickup_index(i);
        let lv = &g.stash[&5];
        let w = lv.ward.unwrap();
        assert_eq!(w.state, WardState::Broken);
        assert_eq!(lv.tile(w.x, w.y), Tile::Floor, "only the remains are left");
        assert_eq!(g.wards_destroyed, 0, "they do not count as broken by you");
    }

    #[test]
    fn some_things_know_no_fear() {
        let unique = M_UNIQUE0; // the Rat King
        let kinds = [
            (M_ORC, true),
            (M_GOBLIN, true),
            (M_SKELETON, false),
            (kind_of("zombie"), false),
            (M_WRAITH, false),
            (kind_of("lich"), false),
            (M_GUARDIAN, false),
            (unique, false),
            (M_WARDEN, false),
        ];
        // Battle Cry.
        for (k, scared) in kinds {
            let mut g = arena();
            g.p.mp = 100;
            let mi = spawn(&mut g, k, 21, 8);
            g.use_skill(Skill::BattleCry);
            assert_eq!(g.level.monsters[mi].fear > 0, scared, "{} after a battle cry", MONSTERS[k].name);
        }
        // A champion's death scream.
        for (k, scared) in kinds {
            let mut g = arena();
            let ci = spawn(&mut g, M_ORC, 22, 8);
            g.make_champion(ci);
            let wi = spawn(&mut g, k, 24, 8);
            g.damage_monster(ci, 99_999, Src::Player);
            assert_eq!(g.level.monsters[wi].fear > 0, scared, "{} after a champion dies", MONSTERS[k].name);
        }
        // The Warden only quails once three of its wards are broken.
        for (broken, scared) in [(0, false), (2, false), (3, true), (4, true)] {
            let mut g = arena();
            g.p.mp = 100;
            let wi = spawn(&mut g, M_WARDEN, 21, 8);
            g.wards_destroyed = broken;
            g.weaken_wardens();
            g.use_skill(Skill::BattleCry);
            assert_eq!(g.level.monsters[wi].fear > 0, scared, "Warden with {} wards broken", broken);
        }
        // Fear that somehow clings to the fearless is shrugged off at once.
        let mut g = arena();
        g.p.invis = 1000;
        let si = spawn(&mut g, M_SKELETON, 30, 8);
        g.level.monsters[si].fear = 10;
        g.monster_act(si);
        assert_eq!(g.level.monsters[si].fear, 0);
    }
}

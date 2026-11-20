//! Static game data: monsters, weapons, armour, items, classes, skills and
//! the persistent profile upgrades.
//!
//! In ASCII art, the character `§` marks a glowing eye. It is drawn as a bright
//! point that stays visible even in total darkness.

use crate::color::Rgb;
use crate::entity::Faction;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum VoiceKind {
    Squeak,
    Screech,
    Yap,
    Rattle,
    Moan,
    Hiss,
    Growl,
    Snarl,
    Roar,
    Wail,
    Chant,
    Cackle,
    Demon,
    Human,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Element {
    Phys,
    Fire,
    Magic,
    Frost,
}

/// Decides which death screen is shown when this monster kills you.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeathKind {
    Beast,
    Undead,
    Brute,
    Crushed,
    Demon,
}

pub struct RangedAtk {
    pub verb: &'static str,
    pub dmg: (i32, i32),
    pub range: i32,
    pub element: Element,
    pub color: Rgb,
    pub ch: char,
    pub chance: f32,
}

pub struct MonsterDef {
    pub name: &'static str,
    pub glyph: char,
    pub color: Rgb,
    pub eye: Rgb,
    pub hp: i32,
    pub hit: i32,
    pub dmg: (i32, i32),
    pub ac: i32,
    /// Ticks per action (100 = normal speed, lower is faster).
    pub speed: f64,
    pub xp: i32,
    pub faction: Faction,
    /// Sprite height relative to a wall.
    pub size: f32,
    /// Height above the floor (bats, wraiths).
    pub float: f32,
    pub voice: VoiceKind,
    pub depth: (u32, u32),
    pub ranged: Option<RangedAtk>,
    pub poison: bool,
    pub regen: bool,
    pub crush: bool,
    pub boss: bool,
    pub erratic: bool,
    /// Range of turns before a corpse rises again. (0,0) = never.
    pub revive: (f64, f64),
    pub sight: i32,
    pub death: DeathKind,
    pub art: &'static [&'static str],
}

const BASE: MonsterDef = MonsterDef {
    name: "thing",
    glyph: '?',
    color: Rgb(200, 200, 200),
    eye: Rgb(255, 50, 30),
    hp: 5,
    hit: 0,
    dmg: (1, 3),
    ac: 0,
    speed: 100.0,
    xp: 1,
    faction: Faction::Vermin,
    size: 0.6,
    float: 0.0,
    voice: VoiceKind::Growl,
    depth: (1, 10),
    ranged: None,
    poison: false,
    regen: false,
    crush: false,
    boss: false,
    erratic: false,
    revive: (60.0, 120.0),
    sight: 7,
    death: DeathKind::Beast,
    art: &[],
};

const ART_RAT: &[&str] = &[
    r"  ()__()      ",
    r"  (§  §)____  ",
    r"   \ ''/    )~",
    r"    '-'-''-'  ",
];
const ART_BAT: &[&str] = &[
    r"__   /\_/\   __",
    r"\ `-( § § )-' /",
    r" `--/  ^  \--' ",
    r"     `   '     ",
];
const ART_KOBOLD: &[&str] = &[
    r"  /\_/\  ",
    r" ( § § ) ",
    r"  \ v /  ",
    r" /|---|\o",
    r"  |   |  ",
    r"  /   \  ",
];
const ART_GOBLIN: &[&str] = &[
    r"<\ ___ />",
    r"  (§ §)  ",
    r"  /'^'\  ",
    r" /|===|\-",
    r"  |___|  ",
    r"  /   \  ",
    r" _|   |_ ",
];
const ART_SKELETON: &[&str] = &[
    r"   .-.   ",
    r"  (§.§)  ",
    r"   |=|   ",
    r"  /-+-\  ",
    r" / |=| \ ",
    r"   |=|   ",
    r"  /   \  ",
    r" _|   |_ ",
];
const ART_ZOMBIE: &[&str] = &[
    r"   ___   ",
    r"  /x §\  ",
    r"  \_~_/  ",
    r"--+#|#+--",
    r"   |#|   ",
    r"   |_|   ",
    r"   / \   ",
    r"  _| |_  ",
];
const ART_SPIDER: &[&str] = &[
    r" /\  __  /\ ",
    r"/  \(§§)/  \",
    r"  _/(  )\_  ",
    r" /  /\/\  \ ",
];
const ART_WOLF: &[&str] = &[
    r"        /\/\  ",
    r"  _____/ § §\ ",
    r" /           w",
    r"/|  ___  __-' ",
    r" | |   | |    ",
];
const ART_ORC: &[&str] = &[
    r"   _____   ",
    r"  / § § \  ",
    r"  | /^\ |  ",
    r"  \v___v/  ",
    r" /|#####|\ ",
    r"/ |#####| \",
    r"  |__|__|  ",
    r"  |  |  |  ",
    r" _|  |  |_ ",
];
const ART_GHOUL: &[&str] = &[
    r"   ,--,   ",
    r"  (§  §)  ",
    r"   \ww/   ",
    r" __/  \__ ",
    r"/  |  |  \",
    r"   /  \   ",
    r"  /    \  ",
];
const ART_DARKELF: &[&str] = &[
    r" __/^\__ ",
    r"  (§ §)  ",
    r"   \-/   ",
    r"  /|^|\  ",
    r" / |^| \|",
    r"   / \   ",
    r"  /   \  ",
];
const ART_IMP: &[&str] = &[
    r" ^     ^ ",
    r"( § v § )",
    r" \_www_/ ",
    r" /|   |\ ",
    r"  /   \~ ",
];
const ART_TROLL: &[&str] = &[
    r"    _____    ",
    r"   /§   §\   ",
    r"  | ,---, |  ",
    r"  \_\VVV/_/  ",
    r" /|       |\ ",
    r"/ |  ___  | \",
    r"\_|       |_/",
    r"  |___|___|  ",
    r"  |   |   |  ",
    r" _|   |   |_ ",
];
const ART_WRAITH: &[&str] = &[
    r"   .---.   ",
    r"  / § § \  ",
    r"  |  o  |  ",
    r" /|     |\ ",
    r"/ |     | \",
    r"  \/\/\/\/ ",
    r"   ~ ~ ~   ",
];
const ART_OGRE: &[&str] = &[
    r"    .---.    ",
    r"   ( § § )   ",
    r"    \_=_/    ",
    r"  .-'   '-.  ",
    r" /  (   )  \ ",
    r"|  (     )  |",
    r" \  '---'  / ",
    r"  |   |   |  ",
    r"  |   |   |  ",
    r" (___| |___) ",
];
const ART_HOUND: &[&str] = &[
    r"  /\ /\       ",
    r" (§ v §)_____ ",
    r"  \www/      \",
    r"   |  ___   |~",
    r"   |_|   |_|  ",
];
const ART_MINOTAUR: &[&str] = &[
    r"\__       __/",
    r"   \_____/   ",
    r"   ( § § )   ",
    r"   ( o_o )   ",
    r" /|=======|\ ",
    r"/ |=======| \",
    r"  |___|___|  ",
    r"  |   |   |  ",
    r" _|   |   |_ ",
];
const ART_LICH: &[&str] = &[
    r"   _/^\_   ",
    r"  /§   §\  ",
    r"  \ ___ /  ",
    r"  /|+++|\  ",
    r" o |+++| | ",
    r"   |+++| | ",
    r"  /_____\| ",
];
const ART_WARDEN: &[&str] = &[
    r"  \\   /^\   //  ",
    r"   \\_/   \_//   ",
    r"    / §   § \    ",
    r"   |  \___/  |   ",
    r"   \ \VVVVV/ /   ",
    r" __/\_______/\__ ",
    r"/  |  #####  |  \",
    r"|  |  #####  |  |",
    r"\_ |  #####  | _/",
    r"   |___| |___|   ",
    r"   |   | |   |   ",
    r"  _|   | |   |_  ",
];

pub const M_WOLF: usize = 8;
pub const M_WARDEN: usize = 18;

pub static MONSTERS: &[MonsterDef] = &[
    MonsterDef {
        name: "giant rat", glyph: 'r', color: Rgb(150, 120, 100), hp: 4, hit: 1, dmg: (1, 3),
        speed: 90.0, xp: 2, faction: Faction::Vermin, size: 0.35, voice: VoiceKind::Squeak,
        depth: (1, 3), revive: (40.0, 80.0), sight: 6, art: ART_RAT, ..BASE
    },
    MonsterDef {
        name: "cave bat", glyph: 'b', color: Rgb(130, 105, 140), hp: 4, hit: 2, dmg: (1, 3), ac: 2,
        speed: 60.0, xp: 3, faction: Faction::Vermin, size: 0.3, float: 0.45, voice: VoiceKind::Screech,
        depth: (1, 4), erratic: true, revive: (40.0, 80.0), art: ART_BAT, ..BASE
    },
    MonsterDef {
        name: "kobold", glyph: 'k', color: Rgb(190, 120, 60), hp: 7, hit: 2, dmg: (1, 5), ac: 1,
        xp: 5, faction: Faction::Greenskin, size: 0.55, voice: VoiceKind::Yap, depth: (1, 4),
        ranged: Some(RangedAtk { verb: "slings a stone", dmg: (1, 3), range: 5, element: Element::Phys, color: Rgb(160, 160, 160), ch: 'o', chance: 0.25 }),
        revive: (60.0, 120.0), death: DeathKind::Brute, art: ART_KOBOLD, ..BASE
    },
    MonsterDef {
        name: "goblin", glyph: 'g', color: Rgb(100, 170, 60), hp: 9, hit: 3, dmg: (2, 6), ac: 2,
        xp: 7, faction: Faction::Greenskin, size: 0.6, voice: VoiceKind::Yap, depth: (1, 5),
        death: DeathKind::Brute, art: ART_GOBLIN, ..BASE
    },
    MonsterDef {
        name: "skeleton", glyph: 's', color: Rgb(225, 220, 195), eye: Rgb(80, 200, 255), hp: 13, hit: 4,
        dmg: (2, 7), ac: 3, xp: 11, faction: Faction::Undead, size: 0.85, voice: VoiceKind::Rattle,
        depth: (2, 7), revive: (15.0, 40.0), death: DeathKind::Undead, art: ART_SKELETON, ..BASE
    },
    MonsterDef {
        name: "zombie", glyph: 'z', color: Rgb(110, 155, 100), eye: Rgb(220, 220, 60), hp: 20, hit: 3,
        dmg: (3, 7), ac: 1, speed: 150.0, xp: 11, faction: Faction::Undead, size: 0.85, voice: VoiceKind::Moan,
        depth: (2, 6), revive: (20.0, 50.0), death: DeathKind::Undead, art: ART_ZOMBIE, ..BASE
    },
    MonsterDef {
        name: "giant spider", glyph: 'S', color: Rgb(100, 80, 100), eye: Rgb(255, 30, 30), hp: 15, hit: 5,
        dmg: (2, 6), ac: 3, speed: 80.0, xp: 15, faction: Faction::Vermin, size: 0.45, voice: VoiceKind::Hiss,
        depth: (3, 7), poison: true, art: ART_SPIDER, ..BASE
    },
    MonsterDef {
        name: "orc", glyph: 'o', color: Rgb(105, 145, 75), hp: 22, hit: 5, dmg: (3, 9), ac: 3,
        xp: 20, faction: Faction::Greenskin, size: 0.9, voice: VoiceKind::Growl, depth: (3, 8),
        revive: (80.0, 150.0), death: DeathKind::Brute, art: ART_ORC, ..BASE
    },
    MonsterDef {
        name: "dire wolf", glyph: 'w', color: Rgb(150, 150, 160), eye: Rgb(255, 210, 40), hp: 16, hit: 5,
        dmg: (2, 7), ac: 2, speed: 70.0, xp: 14, faction: Faction::Beast, size: 0.55, voice: VoiceKind::Snarl,
        depth: (2, 6), sight: 9, art: ART_WOLF, ..BASE
    },
    MonsterDef {
        name: "ghoul", glyph: 'G', color: Rgb(135, 165, 135), eye: Rgb(200, 255, 120), hp: 24, hit: 6,
        dmg: (3, 8), ac: 3, speed: 90.0, xp: 24, faction: Faction::Undead, size: 0.8, voice: VoiceKind::Moan,
        depth: (4, 8), poison: true, revive: (20.0, 50.0), death: DeathKind::Undead, art: ART_GHOUL, ..BASE
    },
    MonsterDef {
        name: "dark elf", glyph: 'e', color: Rgb(130, 105, 170), eye: Rgb(255, 255, 255), hp: 20, hit: 7,
        dmg: (3, 7), ac: 4, speed: 90.0, xp: 28, faction: Faction::Dark, size: 0.8, voice: VoiceKind::Human,
        depth: (5, 9),
        ranged: Some(RangedAtk { verb: "fires a crossbow bolt", dmg: (3, 8), range: 7, element: Element::Phys, color: Rgb(200, 200, 220), ch: '-', chance: 0.5 }),
        revive: (80.0, 150.0), sight: 9, death: DeathKind::Brute, art: ART_DARKELF, ..BASE
    },
    MonsterDef {
        name: "imp", glyph: 'i', color: Rgb(225, 85, 45), eye: Rgb(255, 255, 80), hp: 12, hit: 6, dmg: (2, 5),
        ac: 4, speed: 70.0, xp: 18, faction: Faction::Demon, size: 0.45, float: 0.2, voice: VoiceKind::Cackle,
        depth: (5, 10), erratic: true,
        ranged: Some(RangedAtk { verb: "hurls a firebolt", dmg: (3, 8), range: 6, element: Element::Fire, color: Rgb(255, 140, 30), ch: '*', chance: 0.4 }),
        revive: (50.0, 100.0), death: DeathKind::Demon, art: ART_IMP, ..BASE
    },
    MonsterDef {
        name: "troll", glyph: 'T', color: Rgb(95, 135, 95), eye: Rgb(255, 120, 0), hp: 45, hit: 8, dmg: (5, 12),
        ac: 4, speed: 110.0, xp: 45, faction: Faction::Beast, size: 1.0, voice: VoiceKind::Roar, depth: (6, 10),
        regen: true, crush: true, revive: (30.0, 60.0), death: DeathKind::Crushed, art: ART_TROLL, ..BASE
    },
    MonsterDef {
        name: "wraith", glyph: 'W', color: Rgb(175, 185, 225), eye: Rgb(120, 255, 255), hp: 28, hit: 8,
        dmg: (4, 10), ac: 5, speed: 80.0, xp: 40, faction: Faction::Undead, size: 0.85, float: 0.15,
        voice: VoiceKind::Wail, depth: (6, 10), revive: (20.0, 40.0), death: DeathKind::Undead, art: ART_WRAITH, ..BASE
    },
    MonsterDef {
        name: "ogre", glyph: 'O', color: Rgb(175, 135, 95), hp: 55, hit: 8, dmg: (6, 15), ac: 3, speed: 120.0,
        xp: 50, faction: Faction::Greenskin, size: 1.05, voice: VoiceKind::Roar, depth: (6, 10), crush: true,
        revive: (80.0, 150.0), death: DeathKind::Crushed, art: ART_OGRE, ..BASE
    },
    MonsterDef {
        name: "hellhound", glyph: 'h', color: Rgb(200, 65, 35), eye: Rgb(255, 230, 0), hp: 30, hit: 9, dmg: (4, 10),
        ac: 5, speed: 70.0, xp: 45, faction: Faction::Demon, size: 0.6, voice: VoiceKind::Snarl, depth: (7, 10),
        ranged: Some(RangedAtk { verb: "breathes fire", dmg: (5, 12), range: 4, element: Element::Fire, color: Rgb(255, 100, 20), ch: '~', chance: 0.35 }),
        revive: (50.0, 100.0), sight: 9, death: DeathKind::Demon, art: ART_HOUND, ..BASE
    },
    MonsterDef {
        name: "minotaur", glyph: 'M', color: Rgb(140, 95, 65), hp: 65, hit: 10, dmg: (7, 16), ac: 5,
        xp: 70, faction: Faction::Dark, size: 1.1, voice: VoiceKind::Roar, depth: (8, 10), crush: true,
        revive: (100.0, 200.0), death: DeathKind::Crushed, art: ART_MINOTAUR, ..BASE
    },
    MonsterDef {
        name: "lich", glyph: 'L', color: Rgb(185, 165, 225), eye: Rgb(90, 255, 140), hp: 50, hit: 10, dmg: (4, 10),
        ac: 6, xp: 90, faction: Faction::Undead, size: 0.9, voice: VoiceKind::Chant, depth: (8, 10),
        ranged: Some(RangedAtk { verb: "casts a necrotic bolt", dmg: (7, 14), range: 8, element: Element::Magic, color: Rgb(120, 255, 120), ch: '*', chance: 0.55 }),
        revive: (30.0, 60.0), sight: 9, death: DeathKind::Undead, art: ART_LICH, ..BASE
    },
    MonsterDef {
        name: "Vorth, Warden of GRAZZARK", glyph: 'V', color: Rgb(205, 45, 45), eye: Rgb(255, 255, 120), hp: 180,
        hit: 13, dmg: (10, 22), ac: 8, xp: 400, faction: Faction::Demon, size: 1.25, voice: VoiceKind::Demon,
        depth: (99, 99), boss: true, crush: true,
        ranged: Some(RangedAtk { verb: "unleashes hellfire", dmg: (8, 16), range: 7, element: Element::Fire, color: Rgb(255, 60, 20), ch: '#', chance: 0.3 }),
        revive: (0.0, 0.0), sight: 10, death: DeathKind::Demon, art: ART_WARDEN, ..BASE
    },
];

// ---------------------------------------------------------------- weapons

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WArt {
    Blade,
    Axe,
    Blunt,
    Dagger,
    Bow,
    Staff,
    Spear,
}

pub struct WeaponDef {
    pub name: &'static str,
    pub art: WArt,
    pub dmg: (i32, i32),
    pub hit: i32,
    pub speed: f64,
    pub crit: f32,
    pub stun: f32,
    /// 0 = melee weapon.
    pub range: i32,
    pub depth: u32,
    pub color: Rgb,
    pub glow: bool,
}

const STEEL: Rgb = Rgb(190, 200, 210);
const WOOD: Rgb = Rgb(150, 105, 60);

pub static WEAPONS: &[WeaponDef] = &[
    WeaponDef { name: "Dagger", art: WArt::Dagger, dmg: (1, 4), hit: 2, speed: 80.0, crit: 0.15, stun: 0.0, range: 0, depth: 1, color: STEEL, glow: false },
    WeaponDef { name: "Short Sword", art: WArt::Blade, dmg: (2, 6), hit: 1, speed: 100.0, crit: 0.08, stun: 0.0, range: 0, depth: 1, color: STEEL, glow: false },
    WeaponDef { name: "Long Sword", art: WArt::Blade, dmg: (3, 8), hit: 0, speed: 100.0, crit: 0.08, stun: 0.0, range: 0, depth: 2, color: STEEL, glow: false },
    WeaponDef { name: "Mace", art: WArt::Blunt, dmg: (3, 7), hit: 0, speed: 110.0, crit: 0.05, stun: 0.15, range: 0, depth: 1, color: Rgb(170, 170, 180), glow: false },
    WeaponDef { name: "Battle Axe", art: WArt::Axe, dmg: (4, 10), hit: -1, speed: 120.0, crit: 0.1, stun: 0.0, range: 0, depth: 3, color: STEEL, glow: false },
    WeaponDef { name: "War Hammer", art: WArt::Blunt, dmg: (5, 12), hit: -1, speed: 130.0, crit: 0.05, stun: 0.25, range: 0, depth: 4, color: Rgb(150, 150, 160), glow: false },
    WeaponDef { name: "Spear", art: WArt::Spear, dmg: (3, 9), hit: 1, speed: 105.0, crit: 0.07, stun: 0.0, range: 0, depth: 2, color: STEEL, glow: false },
    WeaponDef { name: "Greatsword", art: WArt::Blade, dmg: (6, 14), hit: -1, speed: 140.0, crit: 0.1, stun: 0.05, range: 0, depth: 5, color: STEEL, glow: false },
    WeaponDef { name: "Quarterstaff", art: WArt::Staff, dmg: (2, 6), hit: 1, speed: 100.0, crit: 0.05, stun: 0.05, range: 0, depth: 1, color: WOOD, glow: true },
    WeaponDef { name: "Short Bow", art: WArt::Bow, dmg: (2, 6), hit: 1, speed: 100.0, crit: 0.08, stun: 0.0, range: 8, depth: 1, color: WOOD, glow: false },
    WeaponDef { name: "Long Bow", art: WArt::Bow, dmg: (3, 9), hit: 1, speed: 110.0, crit: 0.1, stun: 0.0, range: 10, depth: 4, color: WOOD, glow: false },
    WeaponDef { name: "Crossbow", art: WArt::Bow, dmg: (5, 11), hit: 2, speed: 150.0, crit: 0.12, stun: 0.0, range: 8, depth: 5, color: Rgb(120, 90, 70), glow: false },
    WeaponDef { name: "Runeblade", art: WArt::Blade, dmg: (5, 12), hit: 2, speed: 100.0, crit: 0.12, stun: 0.0, range: 0, depth: 7, color: Rgb(120, 180, 255), glow: true },
    WeaponDef { name: "Soulreaver", art: WArt::Axe, dmg: (8, 17), hit: 1, speed: 120.0, crit: 0.15, stun: 0.0, range: 0, depth: 9, color: Rgb(200, 90, 255), glow: true },
];

pub struct ArmorDef {
    pub name: &'static str,
    pub ac: i32,
    pub depth: u32,
}

pub static ARMORS: &[ArmorDef] = &[
    ArmorDef { name: "Rags", ac: 0, depth: 99 },
    ArmorDef { name: "Robe", ac: 1, depth: 1 },
    ArmorDef { name: "Leather Armor", ac: 2, depth: 1 },
    ArmorDef { name: "Studded Leather", ac: 3, depth: 2 },
    ArmorDef { name: "Chain Mail", ac: 4, depth: 3 },
    ArmorDef { name: "Scale Mail", ac: 5, depth: 5 },
    ArmorDef { name: "Mithril Coat", ac: 6, depth: 7 },
    ArmorDef { name: "Plate Armor", ac: 7, depth: 8 },
];

// ---------------------------------------------------------------- items

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Gear {
    pub def: usize,
    pub bonus: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Potion {
    Healing,
    Mana,
    Might,
    Vigor,
    Cure,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scroll {
    Teleport,
    Mapping,
    Fire,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Weapon(Gear),
    Armor(Gear),
    Potion(Potion),
    Scroll(Scroll),
    Amulet,
}

pub fn gear_name(n: &str, b: i32) -> String {
    if b > 0 {
        format!("+{} {}", b, n)
    } else if b < 0 {
        format!("{} {}", b, n)
    } else {
        n.to_string()
    }
}

const ART_POTION: &[&str] = &[r"  _  ", r" |=| ", r"/~~~\", r"\___/"];
const ART_SCROLL: &[&str] = &[r" ____ ", r"(____)", r" |~~| ", r"(____)"];
const ART_WEAPON: &[&str] = &[r"     /", r"    / ", r" \ /  ", r"  X   ", r" / \  "];
const ART_ARMOR: &[&str] = &[r"/\___/\", r"|#####|", r" |###| ", r" |___| "];
const ART_AMULET: &[&str] = &[r" .-~-. ", r"(     )", r" \   / ", r"  (§)  "];
pub const ART_CORPSE: &[&str] = &[r"    ,%%%,    ", r" _,(%%%%%),_ "];
pub const ART_BONES: &[&str] = &[r"  o   ,=-,  ", r"='-%=--o-'=-"];

impl Item {
    pub fn name(&self) -> String {
        match self {
            Item::Weapon(g) => gear_name(WEAPONS[g.def].name, g.bonus),
            Item::Armor(g) => gear_name(ARMORS[g.def].name, g.bonus),
            Item::Potion(p) => format!(
                "Potion of {}",
                match p {
                    Potion::Healing => "Healing",
                    Potion::Mana => "Mana",
                    Potion::Might => "Might",
                    Potion::Vigor => "Vigor",
                    Potion::Cure => "Cure Poison",
                }
            ),
            Item::Scroll(s) => format!(
                "Scroll of {}",
                match s {
                    Scroll::Teleport => "Teleportation",
                    Scroll::Mapping => "Magic Mapping",
                    Scroll::Fire => "Fire",
                }
            ),
            Item::Amulet => "the Amulet of GRAZZARK".to_string(),
        }
    }

    pub fn glyph(&self) -> char {
        match self {
            Item::Weapon(_) => '/',
            Item::Armor(_) => '[',
            Item::Potion(_) => '!',
            Item::Scroll(_) => '?',
            Item::Amulet => '"',
        }
    }

    pub fn color(&self) -> Rgb {
        match self {
            Item::Weapon(g) => WEAPONS[g.def].color,
            Item::Armor(_) => Rgb(170, 160, 140),
            Item::Potion(p) => match p {
                Potion::Healing => Rgb(255, 60, 60),
                Potion::Mana => Rgb(70, 120, 255),
                Potion::Might => Rgb(255, 160, 40),
                Potion::Vigor => Rgb(80, 230, 80),
                Potion::Cure => Rgb(200, 255, 200),
            },
            Item::Scroll(_) => Rgb(235, 225, 180),
            Item::Amulet => Rgb(255, 210, 60),
        }
    }

    pub fn art(&self) -> &'static [&'static str] {
        match self {
            Item::Weapon(_) => ART_WEAPON,
            Item::Armor(_) => ART_ARMOR,
            Item::Potion(_) => ART_POTION,
            Item::Scroll(_) => ART_SCROLL,
            Item::Amulet => ART_AMULET,
        }
    }
}

// ---------------------------------------------------------------- skills

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Skill {
    Cleave,
    WarStomp,
    ShadowVeil,
    FanOfKnives,
    Fireball,
    FrostNova,
    Multishot,
    SummonWolf,
    Heal,
    TurnUndead,
    RaiseDead,
    DrainLife,
    Blink,
    SoulBurst,
    PhaseWard,
    SecondWind,
}

impl Skill {
    pub fn name(self) -> &'static str {
        match self {
            Skill::Cleave => "Cleave",
            Skill::WarStomp => "War Stomp",
            Skill::ShadowVeil => "Shadow Veil",
            Skill::FanOfKnives => "Fan of Knives",
            Skill::Fireball => "Fireball",
            Skill::FrostNova => "Frost Nova",
            Skill::Multishot => "Multishot",
            Skill::SummonWolf => "Summon Wolf",
            Skill::Heal => "Heal",
            Skill::TurnUndead => "Turn Undead",
            Skill::RaiseDead => "Raise Dead",
            Skill::DrainLife => "Drain Life",
            Skill::Blink => "Blink",
            Skill::SoulBurst => "Soul Burst",
            Skill::PhaseWard => "Phase Ward",
            Skill::SecondWind => "Second Wind",
        }
    }

    pub fn mp(self) -> i32 {
        match self {
            Skill::Cleave => 3,
            Skill::WarStomp => 5,
            Skill::ShadowVeil => 6,
            Skill::FanOfKnives => 4,
            Skill::Fireball => 6,
            Skill::FrostNova => 5,
            Skill::Multishot => 4,
            Skill::SummonWolf => 10,
            Skill::Heal => 6,
            Skill::TurnUndead => 5,
            Skill::RaiseDead => 8,
            Skill::DrainLife => 4,
            _ => 0,
        }
    }

    /// Cooldown in turns.
    pub fn cooldown(self) -> f64 {
        match self {
            Skill::WarStomp => 6.0,
            Skill::ShadowVeil => 20.0,
            Skill::FrostNova => 5.0,
            Skill::SummonWolf => 30.0,
            Skill::Heal => 3.0,
            Skill::TurnUndead => 8.0,
            Skill::RaiseDead => 10.0,
            Skill::Blink => 25.0,
            Skill::SoulBurst => 40.0,
            Skill::PhaseWard => 60.0,
            Skill::SecondWind => 150.0,
            _ => 0.0,
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            Skill::Cleave => "Strike every adjacent enemy.",
            Skill::WarStomp => "Shake the earth, stunning and hurting nearby foes.",
            Skill::ShadowVeil => "Become invisible for 12 turns. Backstabs deal x3.",
            Skill::FanOfKnives => "Throw three knives in a fan ahead of you.",
            Skill::Fireball => "Hurl an exploding fireball. Don't stand too close.",
            Skill::FrostNova => "Freeze and damage every foe within 2 tiles.",
            Skill::Multishot => "Loose three arrows in a spread.",
            Skill::SummonWolf => "Call a dire wolf companion to fight beside you.",
            Skill::Heal => "Restore health and cure poison.",
            Skill::TurnUndead => "Holy light sears and terrifies nearby undead.",
            Skill::RaiseDead => "Raise a nearby corpse as your thrall.",
            Skill::DrainLife => "Steal life from the foe in front of you.",
            Skill::Blink => "Teleport up to 4 tiles forward.",
            Skill::SoulBurst => "Blast every foe within 2 tiles with soulfire.",
            Skill::PhaseWard => "A ward absorbs the next 20+ damage.",
            Skill::SecondWind => "Instantly heal half your health.",
        }
    }
}

// ---------------------------------------------------------------- classes

pub struct ClassDef {
    pub name: &'static str,
    pub desc: &'static str,
    pub hp: i32,
    pub mp: i32,
    pub hp_lvl: i32,
    pub mp_lvl: i32,
    pub hit: i32,
    pub weapon: usize,
    pub armor: usize,
    pub skills: [Skill; 2],
    /// Profile upgrade id that unlocks this class, if any.
    pub unlock: Option<&'static str>,
    pub stealth: i32,
    pub color: Rgb,
}

pub const C_ROGUE: usize = 1;
pub const C_MAGE: usize = 2;
pub const C_NECRO: usize = 5;

pub static CLASSES: &[ClassDef] = &[
    ClassDef {
        name: "Warrior", desc: "A hardened fighter in chain mail. High health, strong melee, few tricks.",
        hp: 30, mp: 6, hp_lvl: 8, mp_lvl: 1, hit: 4, weapon: 2, armor: 4,
        skills: [Skill::Cleave, Skill::WarStomp], unlock: None, stealth: 0, color: Rgb(220, 90, 70),
    },
    ClassDef {
        name: "Rogue", desc: "Quick and quiet. Spots traps, and backstabs unaware foes for triple damage.",
        hp: 22, mp: 10, hp_lvl: 5, mp_lvl: 2, hit: 5, weapon: 0, armor: 2,
        skills: [Skill::ShadowVeil, Skill::FanOfKnives], unlock: None, stealth: 3, color: Rgb(150, 150, 170),
    },
    ClassDef {
        name: "Mage", desc: "Frail but devastating. Fireballs and frost, with fast mana regeneration.",
        hp: 16, mp: 22, hp_lvl: 4, mp_lvl: 4, hit: 1, weapon: 8, armor: 1,
        skills: [Skill::Fireball, Skill::FrostNova], unlock: None, stealth: 1, color: Rgb(100, 140, 255),
    },
    ClassDef {
        name: "Cleric", desc: "A holy warrior. Heals wounds and burns the undead with sacred light.",
        hp: 26, mp: 14, hp_lvl: 6, mp_lvl: 3, hit: 3, weapon: 3, armor: 3,
        skills: [Skill::Heal, Skill::TurnUndead], unlock: None, stealth: 0, color: Rgb(240, 220, 120),
    },
    ClassDef {
        name: "Ranger", desc: "Archer of the deep wilds. Fights at range and summons a wolf companion.",
        hp: 22, mp: 12, hp_lvl: 5, mp_lvl: 2, hit: 5, weapon: 9, armor: 2,
        skills: [Skill::Multishot, Skill::SummonWolf], unlock: Some("class_ranger"), stealth: 2, color: Rgb(90, 200, 90),
    },
    ClassDef {
        name: "Necromancer", desc: "Master of death. Raises corpses as thralls and drains the living.",
        hp: 18, mp: 18, hp_lvl: 4, mp_lvl: 4, hit: 2, weapon: 0, armor: 1,
        skills: [Skill::RaiseDead, Skill::DrainLife], unlock: Some("class_necro"), stealth: 1, color: Rgb(150, 90, 200),
    },
];

// ---------------------------------------------------------------- profile upgrades

pub struct UpgradeDef {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub max: u32,
    pub cost: u32,
    pub skill: Option<Skill>,
}

pub static UPGRADES: &[UpgradeDef] = &[
    UpgradeDef { id: "vitality", name: "Vitality", desc: "+4 max HP per rank", max: 5, cost: 10, skill: None },
    UpgradeDef { id: "might", name: "Might", desc: "+1 damage per rank", max: 5, cost: 12, skill: None },
    UpgradeDef { id: "ironskin", name: "Iron Skin", desc: "+1 armor class per rank", max: 3, cost: 15, skill: None },
    UpgradeDef { id: "arcana", name: "Arcane Well", desc: "+3 max MP per rank", max: 5, cost: 8, skill: None },
    UpgradeDef { id: "keen", name: "Keen Eyes", desc: "+1 sight radius and +3% crit per rank", max: 3, cost: 10, skill: None },
    UpgradeDef { id: "scholar", name: "Scholar", desc: "+10% experience per rank", max: 5, cost: 10, skill: None },
    UpgradeDef { id: "regen", name: "Troll Blood", desc: "Faster health regeneration", max: 3, cost: 15, skill: None },
    UpgradeDef { id: "provision", name: "Provisioned", desc: "+1 starting Potion of Healing per rank", max: 3, cost: 8, skill: None },
    UpgradeDef { id: "luck", name: "Fortune", desc: "More and better loot", max: 3, cost: 12, skill: None },
    UpgradeDef { id: "siphon", name: "Soul Siphon", desc: "Heal 1 HP per kill per rank", max: 3, cost: 15, skill: None },
    UpgradeDef { id: "blink", name: "Skill: Blink", desc: "Active: teleport up to 4 tiles forward", max: 1, cost: 30, skill: Some(Skill::Blink) },
    UpgradeDef { id: "burst", name: "Skill: Soul Burst", desc: "Active: blast all foes within 2 tiles", max: 1, cost: 40, skill: Some(Skill::SoulBurst) },
    UpgradeDef { id: "ward", name: "Skill: Phase Ward", desc: "Active: absorb the next 20+ damage", max: 1, cost: 35, skill: Some(Skill::PhaseWard) },
    UpgradeDef { id: "wind", name: "Skill: Second Wind", desc: "Active: heal 50% of max HP", max: 1, cost: 45, skill: Some(Skill::SecondWind) },
    UpgradeDef { id: "class_ranger", name: "Unlock: Ranger", desc: "New class: archer with a wolf companion", max: 1, cost: 40, skill: None },
    UpgradeDef { id: "class_necro", name: "Unlock: Necromancer", desc: "New class: raise the dead, drain life", max: 1, cost: 80, skill: None },
];

pub static DEPTH_NAMES: [&str; 10] = [
    "The Old Cellars",
    "The Rat Warrens",
    "The Forgotten Crypts",
    "The Mossy Halls",
    "The Sunken Galleries",
    "The Fungal Deep",
    "The Obsidian Vaults",
    "The Molten Stair",
    "The Halls of Torment",
    "The Sanctum of GRAZZARK",
];

// ---------------------------------------------------------------- held-weapon art

pub fn view_art(a: WArt) -> &'static [&'static str] {
    match a {
        WArt::Blade => &[
            r"         /|",
            r"        / /",
            r"       / / ",
            r"      / /  ",
            r"     / /   ",
            r"  __/ /__  ",
            r"  \_  __/  ",
            r"    ||     ",
            r"    ||     ",
        ],
        WArt::Axe => &[
            r"   ___     ",
            r"  /   \||  ",
            r" |     ||  ",
            r"  \___/||  ",
            r"       ||  ",
            r"       ||  ",
            r"       ||  ",
            r"       ||  ",
        ],
        WArt::Blunt => &[
            r"  \ | /  ",
            r" --(O)-- ",
            r"  / | \  ",
            r"    |    ",
            r"    |    ",
            r"    |    ",
            r"    |    ",
        ],
        WArt::Dagger => &[
            r"     /\ ",
            r"    / / ",
            r"   / /  ",
            r" _/ /_  ",
            r" \_ _/  ",
            r"   ||   ",
        ],
        WArt::Bow => &[
            r"   |\      ",
            r"   | \     ",
            r"   |  )    ",
            r"---+--)--> ",
            r"   |  )    ",
            r"   | /     ",
            r"   |/      ",
        ],
        WArt::Staff => &[
            r"  (§)  ",
            r"  \|/  ",
            r"   |   ",
            r"   |   ",
            r"   |   ",
            r"   |   ",
            r"   |   ",
            r"   |   ",
        ],
        WArt::Spear => &[
            r"    /\  ",
            r"   /  \ ",
            r"   \  / ",
            r"    ||  ",
            r"    ||  ",
            r"    ||  ",
            r"    ||  ",
            r"    ||  ",
        ],
    }
}

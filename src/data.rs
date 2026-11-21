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
    /// Drifts through walls and can strike from inside them.
    pub ghost: bool,
    /// Floor (1-10) this unique monster haunts; 0 = an ordinary monster.
    pub unique: u32,
    /// Special powers (unique monsters).
    pub abil: &'static [MAbil],
    /// Signature unique item guaranteed to drop (index into UNIQUES).
    pub sig: usize,
}

/// Special powers that unique monsters can have.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MAbil {
    /// Calls `n` minions of monster kind `k` to its side.
    Summon(usize, u8),
    /// Raises nearby corpses as its servants.
    Raise,
    /// Teleports beside its target.
    Blink,
    /// Charges across the room and strikes with extra force.
    Charge,
    /// Its melee blows heal it.
    Vampiric,
    /// Below half health it flies into a faster, deadlier rage.
    Berserk,
    /// Scorches anything standing next to it.
    FireAura,
    /// Its blows freeze you in place.
    FrostTouch,
    /// Its blows drain your mana.
    ManaDrain,
    /// Calls sleeping and idle kin of its faction to the hunt.
    Rally,
    /// Leaps a short distance onto its prey.
    Pounce,
    /// Spits a web that roots its prey in place.
    Web,
    /// Seizes an adjacent foe, holding it fast.
    Grab,
    /// Braces behind shield or stone hide: +4 armour for a few turns.
    Guard,
    /// Strikes twice in a frenzy.
    Flurry,
    /// Throws grit or darkness into its prey's eyes.
    Blind,
    /// Saps life from afar and heals itself with it.
    LifeSap,
    /// Slams the ground: damages and staggers everything nearby.
    Quake,
    /// Hurls a boulder.
    Throw,
    /// Bursts into a cloud of paralysing spores.
    Spores,
    /// Splits in two when wounded.
    Split,
    /// Heals a wounded ally of its kind.
    Mend,
    /// Its sting or claws can paralyse.
    Paralyze,
    /// Bashes with a shield, staggering its foe.
    Bash,
    /// A gaze that turns limbs to stone for a moment.
    Gaze,
    /// A deafening shriek that staggers and chills.
    Shriek,
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
    ghost: false,
    unique: 0,
    abil: &[],
    sig: 0,
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

const ART_RATKING: &[&str] = &[
    r"   _/\/\/\_     ",
    r"   |######|     ",
    r"   ()____()     ",
    r"  (§      §)___ ",
    r"   \ ',,' /    )~",
    r"   /'-''-'\--''  ",
    r"  ()  ()  ()     ",
];

const ART_CENTIPEDE: &[&str] = &[
    r"  __ __ __ __   ",
    r" (§§)OO-OO-OO>- ",
    r"  /\ /\ /\ /\   ",
];
const ART_SLIME: &[&str] = &[
    r"   .-~~-.   ",
    r"  / §  § \  ",
    r" (   ~~   ) ",
    r"  '~-..-~'  ",
];
const ART_MYCONID: &[&str] = &[
    r"  _.---._  ",
    r" (o  o  o) ",
    r"  '-----'  ",
    r"   |§ §|   ",
    r"   | ~ |   ",
    r"  _|   |_  ",
];
const ART_SHAMAN: &[&str] = &[
    r"  _/^\_  ",
    r" <(§ §)> ",
    r"   /~\   ",
    r" */|=|\  ",
    r" | |_|   ",
    r" | / \   ",
];
const ART_SCORPION: &[&str] = &[
    r"  _      __, ",
    r" ( \_§§_/  \ ",
    r"  \=OOOO=   v",
    r"  /\/\/\/\   ",
];
const ART_HOBGOBLIN: &[&str] = &[
    r"  _/V\_  ",
    r" ( § § ) ",
    r"  \ww /  ",
    r" /|###|\ ",
    r"O |###| |",
    r"  |_|_|  ",
    r"  /   \  ",
];
const ART_BEAR: &[&str] = &[
    r"  ()_____()  ",
    r"  / §   § \  ",
    r" |    w    | ",
    r"  \  '-'  /  ",
    r" /'-------'\ ",
    r"|  |     |  |",
    r" \_|_____|_/ ",
    r"   ||   ||   ",
];
const ART_GARGOYLE: &[&str] = &[
    r"\\  /^^\  //",
    r" \\(§  §)// ",
    r"  \/'vv'\/  ",
    r"   /|##|\   ",
    r"    /  \    ",
];
const ART_WIGHT: &[&str] = &[
    r"   _____   ",
    r"  /§   §\  ",
    r"  \_===_/  ",
    r"  /|\ /|\  ",
    r" / | v | \ ",
    r"   |   |   ",
    r"  _/   \_  ",
];
const ART_BANSHEE: &[&str] = &[
    r"   .~~~.   ",
    r"  ( §O§ )  ",
    r"  /\ O /\  ",
    r" / /   \ \ ",
    r"  ~ ~ ~ ~  ",
];
const ART_ELEMENTAL: &[&str] = &[
    r"   ( ) (    ",
    r"  ) (§ §)   ",
    r" ( \(^^^)/  ",
    r"  ) )   ( ( ",
    r"   ( ) ( )  ",
    r"    \ ~ /   ",
];
const ART_BASILISK: &[&str] = &[
    r"   __            ",
    r"  /§§\_____      ",
    r" <___ ^^^^^\__   ",
    r"   /\/\  /\/\ \~ ",
];
const ART_FIEND: &[&str] = &[
    r" \\__    __// ",
    r"   (§ \/ §)   ",
    r"    \VVVV/    ",
    r" /\_/####\_/\ ",
    r"/  |######|  \",
    r"   |##||##|   ",
    r"   /_/  \_\   ",
];
const ART_GUARDIAN: &[&str] = &[
    r"    /\  /\    ",
    r"   <  §§  >   ",
    r"  /|\====/|\  ",
    r" / | \||/ | \ ",
    r"<  |  /\  |  >",
    r"   | /  \ |   ",
    r"  _/\/  \/\_  ",
];

pub const M_WOLF: usize = 8;
/// The Prism Guardian, torn out of a ward prism when you strike it.
pub const M_GUARDIAN: usize = 42;
pub const M_WARDEN: usize = 18;

pub static MONSTERS: &[MonsterDef] = &[
    MonsterDef {
        name: "giant rat",
        glyph: 'r',
        color: Rgb(150, 120, 100),
        hp: 4,
        hit: 1,
        dmg: (1, 3),
        speed: 90.0,
        xp: 2,
        faction: Faction::Vermin,
        size: 0.35,
        voice: VoiceKind::Squeak,
        depth: (1, 3),
        revive: (40.0, 80.0),
        sight: 6,
        art: ART_RAT,
        abil: &[MAbil::Rally],
        ..BASE
    },
    MonsterDef {
        name: "cave bat",
        glyph: 'b',
        color: Rgb(130, 105, 140),
        hp: 4,
        hit: 2,
        dmg: (1, 3),
        ac: 2,
        speed: 60.0,
        xp: 3,
        faction: Faction::Vermin,
        size: 0.3,
        float: 0.45,
        voice: VoiceKind::Screech,
        depth: (1, 4),
        erratic: true,
        revive: (40.0, 80.0),
        art: ART_BAT,
        abil: &[MAbil::Pounce],
        ..BASE
    },
    MonsterDef {
        name: "kobold",
        glyph: 'k',
        color: Rgb(190, 120, 60),
        hp: 7,
        hit: 2,
        dmg: (1, 5),
        ac: 1,
        xp: 5,
        faction: Faction::Greenskin,
        size: 0.55,
        voice: VoiceKind::Yap,
        depth: (1, 4),
        ranged: Some(RangedAtk {
            verb: "slings a stone",
            dmg: (1, 3),
            range: 5,
            element: Element::Phys,
            color: Rgb(160, 160, 160),
            ch: 'o',
            chance: 0.25,
        }),
        revive: (60.0, 120.0),
        death: DeathKind::Brute,
        art: ART_KOBOLD,
        abil: &[MAbil::Rally],
        ..BASE
    },
    MonsterDef {
        name: "goblin",
        glyph: 'g',
        color: Rgb(100, 170, 60),
        hp: 9,
        hit: 3,
        dmg: (2, 6),
        ac: 2,
        xp: 7,
        faction: Faction::Greenskin,
        size: 0.6,
        voice: VoiceKind::Yap,
        depth: (1, 5),
        death: DeathKind::Brute,
        art: ART_GOBLIN,
        abil: &[MAbil::Blind],
        ..BASE
    },
    MonsterDef {
        name: "skeleton",
        glyph: 's',
        color: Rgb(225, 220, 195),
        eye: Rgb(80, 200, 255),
        hp: 13,
        hit: 4,
        dmg: (2, 7),
        ac: 3,
        xp: 11,
        faction: Faction::Undead,
        size: 0.85,
        voice: VoiceKind::Rattle,
        depth: (2, 7),
        revive: (15.0, 40.0),
        death: DeathKind::Undead,
        art: ART_SKELETON,
        abil: &[MAbil::Guard],
        ..BASE
    },
    MonsterDef {
        name: "zombie",
        glyph: 'z',
        color: Rgb(110, 155, 100),
        eye: Rgb(220, 220, 60),
        hp: 20,
        hit: 3,
        dmg: (3, 7),
        ac: 1,
        speed: 150.0,
        xp: 11,
        faction: Faction::Undead,
        size: 0.85,
        voice: VoiceKind::Moan,
        depth: (2, 6),
        revive: (20.0, 50.0),
        death: DeathKind::Undead,
        art: ART_ZOMBIE,
        abil: &[MAbil::Grab],
        ..BASE
    },
    MonsterDef {
        name: "giant spider",
        glyph: 'S',
        color: Rgb(100, 80, 100),
        eye: Rgb(255, 30, 30),
        hp: 15,
        hit: 5,
        dmg: (2, 6),
        ac: 3,
        speed: 80.0,
        xp: 15,
        faction: Faction::Vermin,
        size: 0.45,
        voice: VoiceKind::Hiss,
        depth: (3, 7),
        poison: true,
        art: ART_SPIDER,
        abil: &[MAbil::Web],
        ..BASE
    },
    MonsterDef {
        name: "orc",
        glyph: 'o',
        color: Rgb(105, 145, 75),
        hp: 22,
        hit: 5,
        dmg: (3, 9),
        ac: 3,
        xp: 20,
        faction: Faction::Greenskin,
        size: 0.9,
        voice: VoiceKind::Growl,
        depth: (3, 8),
        revive: (80.0, 150.0),
        death: DeathKind::Brute,
        art: ART_ORC,
        abil: &[MAbil::Berserk],
        ..BASE
    },
    MonsterDef {
        name: "dire wolf",
        glyph: 'w',
        color: Rgb(150, 150, 160),
        eye: Rgb(255, 210, 40),
        hp: 16,
        hit: 5,
        dmg: (2, 7),
        ac: 2,
        speed: 70.0,
        xp: 14,
        faction: Faction::Beast,
        size: 0.55,
        voice: VoiceKind::Snarl,
        depth: (2, 6),
        sight: 9,
        art: ART_WOLF,
        abil: &[MAbil::Pounce, MAbil::Rally],
        ..BASE
    },
    MonsterDef {
        name: "ghoul",
        glyph: 'G',
        color: Rgb(135, 165, 135),
        eye: Rgb(200, 255, 120),
        hp: 24,
        hit: 6,
        dmg: (3, 8),
        ac: 3,
        speed: 90.0,
        xp: 24,
        faction: Faction::Undead,
        size: 0.8,
        voice: VoiceKind::Moan,
        depth: (4, 8),
        poison: true,
        revive: (20.0, 50.0),
        death: DeathKind::Undead,
        art: ART_GHOUL,
        abil: &[MAbil::Paralyze],
        ..BASE
    },
    MonsterDef {
        name: "dark elf",
        glyph: 'e',
        color: Rgb(130, 105, 170),
        eye: Rgb(255, 255, 255),
        hp: 20,
        hit: 7,
        dmg: (3, 7),
        ac: 4,
        speed: 90.0,
        xp: 28,
        faction: Faction::Dark,
        size: 0.8,
        voice: VoiceKind::Human,
        depth: (5, 9),
        ranged: Some(RangedAtk {
            verb: "fires a crossbow bolt",
            dmg: (3, 8),
            range: 7,
            element: Element::Phys,
            color: Rgb(200, 200, 220),
            ch: '-',
            chance: 0.5,
        }),
        revive: (80.0, 150.0),
        sight: 9,
        death: DeathKind::Brute,
        art: ART_DARKELF,
        abil: &[MAbil::Blink, MAbil::Blind],
        ..BASE
    },
    MonsterDef {
        name: "imp",
        glyph: 'i',
        color: Rgb(225, 85, 45),
        eye: Rgb(255, 255, 80),
        hp: 12,
        hit: 6,
        dmg: (2, 5),
        ac: 4,
        speed: 70.0,
        xp: 18,
        faction: Faction::Demon,
        size: 0.45,
        float: 0.2,
        voice: VoiceKind::Cackle,
        depth: (5, 10),
        erratic: true,
        ranged: Some(RangedAtk {
            verb: "hurls a firebolt",
            dmg: (3, 8),
            range: 6,
            element: Element::Fire,
            color: Rgb(255, 140, 30),
            ch: '*',
            chance: 0.4,
        }),
        revive: (50.0, 100.0),
        death: DeathKind::Demon,
        art: ART_IMP,
        abil: &[MAbil::Blink],
        ..BASE
    },
    MonsterDef {
        name: "troll",
        glyph: 'T',
        color: Rgb(95, 135, 95),
        eye: Rgb(255, 120, 0),
        hp: 45,
        hit: 8,
        dmg: (5, 12),
        ac: 4,
        speed: 110.0,
        xp: 45,
        faction: Faction::Beast,
        size: 1.0,
        voice: VoiceKind::Roar,
        depth: (6, 10),
        regen: true,
        crush: true,
        revive: (30.0, 60.0),
        death: DeathKind::Crushed,
        art: ART_TROLL,
        abil: &[MAbil::Throw],
        ..BASE
    },
    MonsterDef {
        name: "wraith",
        glyph: 'W',
        color: Rgb(175, 185, 225),
        eye: Rgb(120, 255, 255),
        hp: 28,
        hit: 8,
        dmg: (4, 10),
        ac: 5,
        speed: 80.0,
        xp: 40,
        faction: Faction::Undead,
        size: 0.85,
        float: 0.15,
        voice: VoiceKind::Wail,
        depth: (6, 10),
        revive: (20.0, 40.0),
        death: DeathKind::Undead,
        art: ART_WRAITH,
        ghost: true,
        abil: &[MAbil::LifeSap],
        ..BASE
    },
    MonsterDef {
        name: "ogre",
        glyph: 'O',
        color: Rgb(175, 135, 95),
        hp: 55,
        hit: 8,
        dmg: (6, 15),
        ac: 3,
        speed: 120.0,
        xp: 50,
        faction: Faction::Greenskin,
        size: 1.05,
        voice: VoiceKind::Roar,
        depth: (6, 10),
        crush: true,
        revive: (80.0, 150.0),
        death: DeathKind::Crushed,
        art: ART_OGRE,
        abil: &[MAbil::Quake],
        ..BASE
    },
    MonsterDef {
        name: "hellhound",
        glyph: 'h',
        color: Rgb(200, 65, 35),
        eye: Rgb(255, 230, 0),
        hp: 30,
        hit: 9,
        dmg: (4, 10),
        ac: 5,
        speed: 70.0,
        xp: 45,
        faction: Faction::Demon,
        size: 0.6,
        voice: VoiceKind::Snarl,
        depth: (7, 10),
        ranged: Some(RangedAtk {
            verb: "breathes fire",
            dmg: (5, 12),
            range: 4,
            element: Element::Fire,
            color: Rgb(255, 100, 20),
            ch: '~',
            chance: 0.35,
        }),
        revive: (50.0, 100.0),
        sight: 9,
        death: DeathKind::Demon,
        art: ART_HOUND,
        abil: &[MAbil::Pounce],
        ..BASE
    },
    MonsterDef {
        name: "minotaur",
        glyph: 'M',
        color: Rgb(140, 95, 65),
        hp: 65,
        hit: 10,
        dmg: (7, 16),
        ac: 5,
        xp: 70,
        faction: Faction::Dark,
        size: 1.1,
        voice: VoiceKind::Roar,
        depth: (8, 10),
        crush: true,
        revive: (100.0, 200.0),
        death: DeathKind::Crushed,
        art: ART_MINOTAUR,
        abil: &[MAbil::Charge],
        ..BASE
    },
    MonsterDef {
        name: "lich",
        glyph: 'L',
        color: Rgb(185, 165, 225),
        eye: Rgb(90, 255, 140),
        hp: 50,
        hit: 10,
        dmg: (4, 10),
        ac: 6,
        xp: 90,
        faction: Faction::Undead,
        size: 0.9,
        voice: VoiceKind::Chant,
        depth: (8, 10),
        ranged: Some(RangedAtk {
            verb: "casts a necrotic bolt",
            dmg: (7, 14),
            range: 8,
            element: Element::Magic,
            color: Rgb(120, 255, 120),
            ch: '*',
            chance: 0.55,
        }),
        revive: (30.0, 60.0),
        sight: 9,
        death: DeathKind::Undead,
        art: ART_LICH,
        abil: &[MAbil::Raise, MAbil::LifeSap],
        ..BASE
    },
    MonsterDef {
        name: "Vorth, Warden of GRAZZARK",
        glyph: 'V',
        color: Rgb(205, 45, 45),
        eye: Rgb(255, 255, 120),
        hp: 180,
        hit: 13,
        dmg: (10, 22),
        ac: 8,
        xp: 400,
        faction: Faction::Demon,
        size: 1.25,
        voice: VoiceKind::Demon,
        depth: (99, 99),
        boss: true,
        crush: true,
        ranged: Some(RangedAtk {
            verb: "unleashes hellfire",
            dmg: (8, 16),
            range: 7,
            element: Element::Fire,
            color: Rgb(255, 60, 20),
            ch: '#',
            chance: 0.3,
        }),
        revive: (0.0, 0.0),
        sight: 10,
        death: DeathKind::Demon,
        art: ART_WARDEN,
        abil: &[MAbil::Summon(11, 2), MAbil::Quake, MAbil::Charge],
        ..BASE
    },
    // ------------------------------------------------ floor uniques (one per depth)
    MonsterDef {
        name: "Gnawbone, the Rat King",
        glyph: 'R',
        color: Rgb(185, 150, 110),
        eye: Rgb(255, 40, 40),
        hp: 38,
        hit: 4,
        dmg: (2, 7),
        ac: 2,
        speed: 85.0,
        xp: 45,
        faction: Faction::Vermin,
        size: 0.6,
        voice: VoiceKind::Squeak,
        depth: (99, 99),
        revive: (0.0, 0.0),
        sight: 8,
        art: ART_RATKING,
        unique: 1,
        abil: &[MAbil::Summon(0, 3)],
        sig: 0,
        ..BASE
    },
    MonsterDef {
        name: "Grutch the Warchief",
        glyph: 'C',
        color: Rgb(140, 190, 70),
        eye: Rgb(255, 200, 40),
        hp: 55,
        hit: 5,
        dmg: (3, 9),
        ac: 3,
        xp: 70,
        faction: Faction::Greenskin,
        size: 0.8,
        voice: VoiceKind::Growl,
        depth: (99, 99),
        revive: (0.0, 0.0),
        death: DeathKind::Brute,
        art: ART_GOBLIN,
        unique: 2,
        abil: &[MAbil::Berserk, MAbil::Summon(2, 2)],
        sig: 1,
        ..BASE
    },
    MonsterDef {
        name: "the Weeping Bride",
        glyph: 'B',
        color: Rgb(210, 220, 255),
        eye: Rgb(160, 230, 255),
        hp: 60,
        hit: 6,
        dmg: (3, 9),
        ac: 4,
        speed: 85.0,
        xp: 95,
        faction: Faction::Undead,
        size: 0.85,
        float: 0.2,
        voice: VoiceKind::Wail,
        depth: (99, 99),
        revive: (0.0, 0.0),
        sight: 9,
        death: DeathKind::Undead,
        art: ART_WRAITH,
        ghost: true,
        unique: 3,
        abil: &[MAbil::FrostTouch, MAbil::Blink],
        sig: 2,
        ..BASE
    },
    MonsterDef {
        name: "Old Mossback",
        glyph: 'Q',
        color: Rgb(90, 160, 70),
        eye: Rgb(255, 230, 60),
        hp: 95,
        hit: 7,
        dmg: (4, 11),
        ac: 4,
        speed: 110.0,
        xp: 130,
        faction: Faction::Beast,
        size: 1.05,
        voice: VoiceKind::Roar,
        depth: (99, 99),
        regen: true,
        crush: true,
        revive: (0.0, 0.0),
        death: DeathKind::Crushed,
        art: ART_TROLL,
        unique: 4,
        abil: &[MAbil::Vampiric],
        sig: 3,
        ..BASE
    },
    MonsterDef {
        name: "Xal'Veth the Drowned",
        glyph: 'X',
        color: Rgb(80, 150, 170),
        eye: Rgb(120, 255, 230),
        hp: 80,
        hit: 8,
        dmg: (4, 10),
        ac: 5,
        speed: 90.0,
        xp: 160,
        faction: Faction::Dark,
        size: 0.85,
        voice: VoiceKind::Chant,
        depth: (99, 99),
        revive: (0.0, 0.0),
        sight: 9,
        death: DeathKind::Brute,
        art: ART_DARKELF,
        unique: 5,
        ranged: Some(RangedAtk {
            verb: "hurls a lance of black ice",
            dmg: (5, 11),
            range: 7,
            element: Element::Frost,
            color: Rgb(140, 220, 255),
            ch: '*',
            chance: 0.45,
        }),
        abil: &[MAbil::Blink, MAbil::FrostTouch],
        sig: 4,
        ..BASE
    },
    MonsterDef {
        name: "Vyrmatha, Queen of Webs",
        glyph: 'Y',
        color: Rgb(120, 60, 120),
        eye: Rgb(255, 60, 200),
        hp: 90,
        hit: 9,
        dmg: (4, 10),
        ac: 5,
        speed: 75.0,
        xp: 190,
        faction: Faction::Vermin,
        size: 0.7,
        voice: VoiceKind::Hiss,
        depth: (99, 99),
        poison: true,
        revive: (0.0, 0.0),
        art: ART_SPIDER,
        unique: 6,
        abil: &[MAbil::Summon(6, 2), MAbil::Vampiric],
        sig: 5,
        ..BASE
    },
    MonsterDef {
        name: "Kragh the Obsidian Colossus",
        glyph: 'K',
        color: Rgb(70, 60, 85),
        eye: Rgb(255, 120, 40),
        hp: 150,
        hit: 10,
        dmg: (7, 16),
        ac: 8,
        speed: 115.0,
        xp: 240,
        faction: Faction::Greenskin,
        size: 1.2,
        voice: VoiceKind::Roar,
        depth: (99, 99),
        crush: true,
        revive: (0.0, 0.0),
        death: DeathKind::Crushed,
        art: ART_OGRE,
        unique: 7,
        abil: &[MAbil::Charge],
        sig: 6,
        ..BASE
    },
    MonsterDef {
        name: "Ignar the Cinderlord",
        glyph: 'I',
        color: Rgb(255, 110, 40),
        eye: Rgb(255, 255, 160),
        hp: 120,
        hit: 10,
        dmg: (5, 12),
        ac: 6,
        speed: 80.0,
        xp: 270,
        faction: Faction::Demon,
        size: 0.8,
        float: 0.15,
        voice: VoiceKind::Cackle,
        depth: (99, 99),
        revive: (0.0, 0.0),
        sight: 9,
        death: DeathKind::Demon,
        art: ART_IMP,
        unique: 8,
        ranged: Some(RangedAtk {
            verb: "hurls a gout of cinders",
            dmg: (6, 13),
            range: 7,
            element: Element::Fire,
            color: Rgb(255, 150, 40),
            ch: '*',
            chance: 0.4,
        }),
        abil: &[MAbil::FireAura, MAbil::Summon(11, 2)],
        sig: 7,
        ..BASE
    },
    MonsterDef {
        name: "Asterion the Tormentor",
        glyph: 'A',
        color: Rgb(170, 70, 50),
        eye: Rgb(255, 40, 40),
        hp: 170,
        hit: 12,
        dmg: (8, 18),
        ac: 6,
        speed: 95.0,
        xp: 320,
        faction: Faction::Dark,
        size: 1.2,
        voice: VoiceKind::Roar,
        depth: (99, 99),
        crush: true,
        revive: (0.0, 0.0),
        sight: 10,
        death: DeathKind::Crushed,
        art: ART_MINOTAUR,
        unique: 9,
        abil: &[MAbil::Charge, MAbil::Berserk],
        sig: 8,
        ..BASE
    },
    MonsterDef {
        name: "Malzareth the Undying",
        glyph: 'Z',
        color: Rgb(150, 120, 220),
        eye: Rgb(255, 80, 255),
        hp: 150,
        hit: 12,
        dmg: (6, 13),
        ac: 7,
        xp: 380,
        faction: Faction::Undead,
        size: 1.0,
        float: 0.1,
        voice: VoiceKind::Chant,
        depth: (99, 99),
        revive: (0.0, 0.0),
        sight: 10,
        death: DeathKind::Undead,
        art: ART_LICH,
        unique: 10,
        ranged: Some(RangedAtk {
            verb: "unleashes a soulfire bolt",
            dmg: (8, 16),
            range: 8,
            element: Element::Magic,
            color: Rgb(220, 120, 255),
            ch: '*',
            chance: 0.5,
        }),
        abil: &[MAbil::Raise, MAbil::Blink, MAbil::ManaDrain],
        sig: 9,
        ..BASE
    },
    // ---- the wider bestiary (appended so saved indices stay valid)
    MonsterDef {
        name: "giant centipede",
        glyph: 'c',
        color: Rgb(170, 90, 50),
        eye: Rgb(255, 200, 60),
        hp: 6,
        hit: 2,
        dmg: (1, 4),
        speed: 75.0,
        xp: 4,
        faction: Faction::Vermin,
        size: 0.3,
        voice: VoiceKind::Hiss,
        depth: (1, 5),
        poison: true,
        revive: (40.0, 80.0),
        art: ART_CENTIPEDE,
        abil: &[MAbil::Flurry],
        ..BASE
    },
    MonsterDef {
        name: "green slime",
        glyph: 'j',
        color: Rgb(110, 200, 90),
        eye: Rgb(20, 60, 10),
        hp: 14,
        hit: 1,
        dmg: (1, 4),
        speed: 140.0,
        xp: 4,
        faction: Faction::Vermin,
        size: 0.4,
        voice: VoiceKind::Moan,
        depth: (1, 4),
        revive: (30.0, 70.0),
        sight: 5,
        art: ART_SLIME,
        abil: &[MAbil::Split],
        ..BASE
    },
    MonsterDef {
        name: "myconid",
        glyph: 'm',
        color: Rgb(200, 150, 190),
        eye: Rgb(255, 255, 160),
        hp: 12,
        hit: 3,
        dmg: (2, 5),
        ac: 1,
        speed: 120.0,
        xp: 8,
        faction: Faction::Vermin,
        size: 0.6,
        voice: VoiceKind::Hiss,
        depth: (2, 6),
        poison: true,
        revive: (50.0, 100.0),
        sight: 6,
        art: ART_MYCONID,
        abil: &[MAbil::Spores],
        ..BASE
    },
    MonsterDef {
        name: "goblin shaman",
        glyph: 'n',
        color: Rgb(120, 190, 140),
        eye: Rgb(255, 120, 255),
        hp: 10,
        hit: 3,
        dmg: (1, 4),
        ac: 1,
        xp: 12,
        faction: Faction::Greenskin,
        size: 0.6,
        voice: VoiceKind::Yap,
        depth: (2, 6),
        ranged: Some(RangedAtk {
            verb: "hurls a hex",
            dmg: (2, 6),
            range: 6,
            element: Element::Magic,
            color: Rgb(200, 100, 255),
            ch: '*',
            chance: 0.45,
        }),
        sight: 8,
        death: DeathKind::Brute,
        art: ART_SHAMAN,
        abil: &[MAbil::Mend],
        ..BASE
    },
    MonsterDef {
        name: "giant scorpion",
        glyph: 'x',
        color: Rgb(160, 120, 60),
        eye: Rgb(255, 60, 30),
        hp: 20,
        hit: 5,
        dmg: (3, 7),
        ac: 5,
        speed: 90.0,
        xp: 18,
        faction: Faction::Vermin,
        size: 0.45,
        voice: VoiceKind::Hiss,
        depth: (3, 8),
        poison: true,
        revive: (60.0, 120.0),
        art: ART_SCORPION,
        abil: &[MAbil::Paralyze],
        ..BASE
    },
    MonsterDef {
        name: "hobgoblin",
        glyph: 'H',
        color: Rgb(170, 110, 60),
        eye: Rgb(255, 220, 60),
        hp: 26,
        hit: 6,
        dmg: (3, 9),
        ac: 4,
        xp: 22,
        faction: Faction::Greenskin,
        size: 0.85,
        voice: VoiceKind::Growl,
        depth: (3, 7),
        revive: (80.0, 150.0),
        death: DeathKind::Brute,
        art: ART_HOBGOBLIN,
        abil: &[MAbil::Bash],
        ..BASE
    },
    MonsterDef {
        name: "cave bear",
        glyph: 'U',
        color: Rgb(120, 85, 60),
        eye: Rgb(255, 180, 60),
        hp: 40,
        hit: 7,
        dmg: (4, 11),
        ac: 3,
        speed: 110.0,
        xp: 35,
        faction: Faction::Beast,
        size: 0.95,
        voice: VoiceKind::Roar,
        depth: (4, 8),
        crush: true,
        death: DeathKind::Crushed,
        art: ART_BEAR,
        abil: &[MAbil::Charge],
        ..BASE
    },
    MonsterDef {
        name: "gargoyle",
        glyph: 'E',
        color: Rgb(130, 130, 140),
        eye: Rgb(255, 90, 40),
        hp: 32,
        hit: 7,
        dmg: (3, 9),
        ac: 8,
        speed: 90.0,
        xp: 38,
        faction: Faction::Dark,
        size: 0.8,
        float: 0.1,
        voice: VoiceKind::Screech,
        depth: (5, 9),
        revive: (0.0, 0.0),
        death: DeathKind::Crushed,
        art: ART_GARGOYLE,
        abil: &[MAbil::Guard, MAbil::Pounce],
        ..BASE
    },
    MonsterDef {
        name: "wight",
        glyph: 'J',
        color: Rgb(170, 180, 160),
        eye: Rgb(120, 200, 255),
        hp: 30,
        hit: 8,
        dmg: (3, 9),
        ac: 5,
        xp: 36,
        faction: Faction::Undead,
        size: 0.85,
        voice: VoiceKind::Wail,
        depth: (5, 9),
        revive: (20.0, 50.0),
        death: DeathKind::Undead,
        art: ART_WIGHT,
        abil: &[MAbil::Vampiric, MAbil::Grab],
        ..BASE
    },
    MonsterDef {
        name: "banshee",
        glyph: 'F',
        color: Rgb(200, 220, 240),
        eye: Rgb(150, 255, 255),
        hp: 24,
        hit: 8,
        dmg: (3, 8),
        ac: 5,
        speed: 75.0,
        xp: 42,
        faction: Faction::Undead,
        size: 0.8,
        float: 0.25,
        voice: VoiceKind::Wail,
        depth: (6, 10),
        ghost: true,
        ranged: Some(RangedAtk {
            verb: "lets loose a keening shriek",
            dmg: (4, 9),
            range: 5,
            element: Element::Frost,
            color: Rgb(180, 230, 255),
            ch: '~',
            chance: 0.3,
        }),
        revive: (25.0, 50.0),
        death: DeathKind::Undead,
        art: ART_BANSHEE,
        abil: &[MAbil::FrostTouch, MAbil::Shriek],
        ..BASE
    },
    MonsterDef {
        name: "fire elemental",
        glyph: 'f',
        color: Rgb(255, 130, 40),
        eye: Rgb(255, 255, 200),
        hp: 36,
        hit: 9,
        dmg: (4, 10),
        ac: 4,
        speed: 85.0,
        xp: 50,
        faction: Faction::Demon,
        size: 0.9,
        float: 0.05,
        voice: VoiceKind::Demon,
        depth: (7, 10),
        revive: (40.0, 80.0),
        death: DeathKind::Demon,
        art: ART_ELEMENTAL,
        abil: &[MAbil::FireAura, MAbil::Blink],
        ..BASE
    },
    MonsterDef {
        name: "basilisk",
        glyph: 'l',
        color: Rgb(90, 140, 80),
        eye: Rgb(255, 255, 0),
        hp: 42,
        hit: 9,
        dmg: (4, 10),
        ac: 6,
        speed: 110.0,
        xp: 52,
        faction: Faction::Beast,
        size: 0.5,
        voice: VoiceKind::Hiss,
        depth: (6, 10),
        poison: true,
        ranged: Some(RangedAtk {
            verb: "fixes you with a freezing gaze",
            dmg: (3, 7),
            range: 6,
            element: Element::Frost,
            color: Rgb(200, 255, 120),
            ch: '*',
            chance: 0.3,
        }),
        art: ART_BASILISK,
        abil: &[MAbil::Gaze],
        ..BASE
    },
    MonsterDef {
        name: "pit fiend",
        glyph: 'P',
        color: Rgb(170, 40, 40),
        eye: Rgb(255, 200, 0),
        hp: 75,
        hit: 11,
        dmg: (7, 16),
        ac: 6,
        speed: 95.0,
        xp: 95,
        faction: Faction::Demon,
        size: 1.15,
        voice: VoiceKind::Demon,
        depth: (8, 10),
        crush: true,
        ranged: Some(RangedAtk {
            verb: "spits hellfire",
            dmg: (6, 12),
            range: 6,
            element: Element::Fire,
            color: Rgb(255, 80, 20),
            ch: '#',
            chance: 0.3,
        }),
        revive: (60.0, 120.0),
        sight: 9,
        death: DeathKind::Demon,
        art: ART_FIEND,
        abil: &[MAbil::Charge, MAbil::Summon(11, 1)],
        ..BASE
    },
    MonsterDef {
        name: "Prism Guardian",
        glyph: '&',
        color: Rgb(150, 230, 255),
        eye: Rgb(255, 255, 255),
        hp: 40,
        hit: 7,
        dmg: (3, 8),
        ac: 6,
        xp: 120,
        faction: Faction::Dark,
        size: 1.1,
        voice: VoiceKind::Chant,
        depth: (99, 99),
        ranged: Some(RangedAtk {
            verb: "hurls a shard of crystal",
            dmg: (3, 7),
            range: 6,
            element: Element::Magic,
            color: Rgb(160, 240, 255),
            ch: '*',
            chance: 0.4,
        }),
        revive: (0.0, 0.0),
        sight: 10,
        death: DeathKind::Crushed,
        art: ART_GUARDIAN,
        abil: &[MAbil::Guard],
        ..BASE
    },
];

/// How likely a monster is to be chosen at `depth`: highest in the middle of
/// its range and tapering off toward either end, so weaker creatures crowd the
/// upper floors and stronger ones the lower.
pub fn spawn_weight(d: &MonsterDef, depth: u32) -> f32 {
    if d.boss || d.unique > 0 || depth < d.depth.0 || depth > d.depth.1 {
        return 0.0;
    }
    let home = (d.depth.0 + d.depth.1) as f32 / 2.0;
    let spread = ((d.depth.1 - d.depth.0) as f32 / 2.0).max(1.0) * 1.1;
    let z = (depth as f32 - home) / spread;
    (-z * z).exp().max(0.05)
}

/// Bestiary display order: common monsters by depth, then the Warden, then
/// the floor uniques in order.
pub fn bestiary_order() -> Vec<usize> {
    let mut common: Vec<usize> = (0..MONSTERS.len())
        .filter(|&i| !MONSTERS[i].boss && MONSTERS[i].unique == 0)
        .collect();
    common.sort_by_key(|&i| (MONSTERS[i].depth.0, MONSTERS[i].xp));
    let mut v = common;
    v.extend((0..MONSTERS.len()).filter(|&i| MONSTERS[i].boss));
    let mut uniq: Vec<usize> = (0..MONSTERS.len())
        .filter(|&i| MONSTERS[i].unique > 0)
        .collect();
    uniq.sort_by_key(|&i| MONSTERS[i].unique);
    v.extend(uniq);
    v
}

/// Index of the first floor unique in MONSTERS; floor N's unique is at M_UNIQUE0 + N - 1.
pub const M_UNIQUE0: usize = 19;

/// Bestiary lore, one entry per monster in MONSTERS.
pub static MONSTER_LORE: &[&str] = &[
    "Bloated cellar rats grown fat on the dungeon's dead. Alone they are a nuisance; in a pack they can strip a careless adventurer to the bone. They die easily, but never stay dead for long.",
    "Leathery bats that flit in erratic loops, too quick for most blades. Their bites are shallow, but they are hard to pin down and harder to escape.",
    "Small, cowardly reptilian scavengers that sling stones from a distance before closing in. They hate goblins only slightly less than they hate you.",
    "Wiry greenskins with notched blades and no fear of a fair fight, because they never fight one. They hunt alongside kobolds and orcs, and quarrel with beasts and dark elves.",
    "Bones bound together by GRAZZARK's will. Their armoured frames turn blades, and smashed skeletons knit themselves back together surprisingly quickly.",
    "Shambling corpses that soak up punishment and keep coming. They are slow, which is the only mercy they offer, and they rise again soon after falling.",
    "Bristling hunters that lurk in the dark between rooms. Their venom lingers in the blood long after the bite, and they move faster than they look.",
    "Brutal greenskin warriors that hit hard and hold their ground. They war with beasts and dark elves, and they bear grudges for a very long time.",
    "Grey wolves the size of ponies, with eyes that catch the torchlight. They see far, run fast, and hunt in silence until the moment they strike.",
    "Corpse-eaters with poisoned claws, quick for undead. They feed on the fallen and rise again from their own remains before long.",
    "Pale-eyed archers from the Underdark who fire crossbow bolts from the shadows. They see far, strike from range, and despise greenskins.",
    "Cackling fire-sprites that dart about and hurl firebolts. Their erratic flight makes them maddening to corner, and they hate everything that lives.",
    "Hulking regenerating brutes whose wounds close as you watch. A troll's blow can crush a skull, and it must be killed quickly or not at all.",
    "Hateful spirits that drift through solid stone. A wraith will follow you through walls and strike from inside them, unseen.",
    "Towering, dim-witted greenskins with clubs the size of a man. Slow, but a single hit can shatter armour and bone alike.",
    "Infernal dogs with molten eyes that breathe gouts of fire. They are fast and see far, and they are never alone for long.",
    "Bull-headed giants that guard the deepest halls. Their crushing blows are among the deadliest in GRAZZARK's domain.",
    "Dead sorcerers bound to GRAZZARK's service, casting necrotic bolts from afar. Destroy one and it will gather itself again in the dark.",
    "Vorth, the Warden of GRAZZARK, guardian of the Amulet. A demon of fire and iron that hurls hellfire and crushes all who come near.",
    "A monstrous rat grown from a knot of a hundred tails. It calls its children to its side and never, ever fights alone.",
    "The warlord of the goblin warrens. When wounded, Grutch flies into a berserk rage and summons his warband to the fight.",
    "A drowned bride who weeps cold tears through the crypt walls. Her touch freezes the blood, and she steps through shadow to reach her prey.",
    "An ancient troll overgrown with moss and fungus. Every blow it lands feeds its wounds, and it regenerates relentlessly.",
    "A sorcerer drowned in the sunken galleries who returned with a heart of black ice. Xal'Veth blinks between shadows and casts lances of frost.",
    "The spider queen of the fungal deep. She drinks the life of her victims and calls her brood to swarm the unwary.",
    "A living statue of volcanic glass that charges across rooms with unstoppable force. Its obsidian hide turns aside all but the hardest blows.",
    "Lord of the molten stair, wreathed in flame. Everything near Ignar burns, and imps flock to his call.",
    "The minotaur jailer of the halls of torment. Asterion charges headlong at its prey and flies into a berserk frenzy when wounded.",
    "A lich-king who has cheated death a thousand times. Malzareth raises the fallen, drains the minds of the living, and steps through space at will.",
    "A rust-red centipede as long as a man's arm, scuttling on a hundred legs. Its bite carries a stinging venom, and it is quicker than anything that size has a right to be.",
    "A quivering mound of acidic jelly that oozes along the flagstones. Slow and witless, but it soaks up blows like a sponge and slowly dissolves whatever it engulfs.",
    "A walking fungus with a cap of pale spores. Its touch seeds its rot into your blood, and its spores drift long after the creature itself has fallen.",
    "A goblin mystic draped in bones and feathers who hurls crackling hexes from behind its kin. Kill it first, or its curses will wear you down.",
    "An armoured scorpion the size of a hound. Its plates turn aside weak blows, and its tail drips venom that burns long after the sting.",
    "The larger, crueller cousin of the goblin: disciplined, well-armed, and fond of shields. Hobgoblins drill the warbands and lead from the front.",
    "A shaggy cave bear driven mad by the dungeon's dark. Its claws can crush plate armour, but it sleeps deeply and stays dead once slain.",
    "A winged demon of carved stone that perches unmoving among the statues. Its stony hide is among the hardest in the dungeon, and when broken it never rises again.",
    "A barrow-lord risen in rusted mail, its touch stealing the warmth from the living to feed its own. Every blow it lands mends its rotten flesh.",
    "The ghost of a mourning woman who drifts through solid stone. Her keening shriek chills the blood, and her touch freezes you where you stand.",
    "A living whirl of flame summoned from the molten depths. Everything that stands beside it burns, friend or foe alike.",
    "A great grey lizard with a crown of horns and a gaze that numbs the limbs. Its bite is venomous, and its hide is thick enough to turn a sword.",
    "A towering demon of the deepest pits, armoured in black scale. It crushes bone with its fists and spits gouts of hellfire at anything it cannot reach.",
    "A golem of living crystal that bursts out of a ward prism when it is struck. It seals the room behind walls of crystal, hurls razor shards, and shields the ward for as long as it stands. Break it, and the ward breaks with it.",
];

impl MAbil {
    pub fn describe(self) -> String {
        match self {
            MAbil::Summon(k, n) => format!("summons {} {}s", n, MONSTERS[k].name),
            MAbil::Raise => "raises the dead".to_string(),
            MAbil::Blink => "teleports beside its prey".to_string(),
            MAbil::Charge => "charges across rooms".to_string(),
            MAbil::Vampiric => "heals by hitting you".to_string(),
            MAbil::Berserk => "goes berserk when wounded".to_string(),
            MAbil::FireAura => "burns everything adjacent".to_string(),
            MAbil::FrostTouch => "freezing touch slows you".to_string(),
            MAbil::ManaDrain => "drains your mana".to_string(),
            MAbil::Rally => "calls its kin to the hunt".to_string(),
            MAbil::Pounce => "pounces from a distance".to_string(),
            MAbil::Web => "spits webs that root you in place".to_string(),
            MAbil::Grab => "seizes and holds you fast".to_string(),
            MAbil::Guard => "braces to harden its defences".to_string(),
            MAbil::Flurry => "strikes twice in a frenzy".to_string(),
            MAbil::Blind => "blinds you".to_string(),
            MAbil::LifeSap => "saps your life from afar to heal itself".to_string(),
            MAbil::Quake => "slams the ground, staggering all nearby".to_string(),
            MAbil::Throw => "hurls boulders".to_string(),
            MAbil::Spores => "bursts into clouds of paralysing spores".to_string(),
            MAbil::Split => "splits in two when wounded".to_string(),
            MAbil::Mend => "heals its wounded kin".to_string(),
            MAbil::Paralyze => "can paralyse with its strikes".to_string(),
            MAbil::Bash => "shield-bashes, staggering you".to_string(),
            MAbil::Gaze => "a gaze that turns limbs to stone".to_string(),
            MAbil::Shriek => "a deafening, chilling shriek".to_string(),
        }
    }
}

/// Kills needed for each tier of bestiary knowledge.
#[derive(Clone, Copy, Debug)]
pub struct LoreTiers {
    /// Name and portrait.
    pub name: u32,
    /// Health, damage, armour, speed.
    pub basic: u32,
    /// The written description.
    pub desc: u32,
    /// Depths, accuracy, traits and the rest.
    pub adv: u32,
    /// The star, and the crit bonus against it.
    pub master: u32,
}

/// Common monsters are slain by the hundred.
pub const LORE_COMMON: LoreTiers = LoreTiers {
    name: 1,
    basic: 10,
    desc: 100,
    adv: 100,
    master: 200,
};
/// Uniques and the Warden are met at most once a run.
pub const LORE_RARE: LoreTiers = LoreTiers {
    name: 1,
    basic: 3,
    desc: 5,
    adv: 7,
    master: 10,
};

impl MonsterDef {
    pub fn lore(&self) -> LoreTiers {
        // Bosses, uniques and the Prism Guardian are met only a few times a run.
        if self.boss || self.unique > 0 || self.depth.0 >= 99 {
            LORE_RARE
        } else {
            LORE_COMMON
        }
    }
}

// ---------------------------------------------------------------- unique items

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum USlot {
    Weapon,
    Armor,
    Ring,
    Amulet,
}

/// Special powers granted by unique gear for the rest of the run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UPower {
    Soulfeast,
    Executioner,
    Undying,
    Regrowth,
    Frost,
    Vampiric,
    Stoneskin,
    Flame,
    Swift,
    Arcane,
    Thorns,
    Storm,
    Deathsight,
    Fortune,
    Shadow,
    Wrath,
}

pub struct UniqueDef {
    pub name: &'static str,
    pub slot: USlot,
    /// Base weapon or armour (index into WEAPONS / ARMORS); unused for rings and amulets.
    pub base: usize,
    pub bonus: i32,
    pub power: UPower,
    pub desc: &'static str,
    pub color: Rgb,
}

/// The first ten are the signature drops of the floor uniques (in floor
/// order); the rest can only be found on champions slain on depth 10.
pub static UNIQUES: &[UniqueDef] = &[
    UniqueDef {
        name: "Gnawbone's Signet",
        slot: USlot::Ring,
        base: 0,
        bonus: 0,
        power: UPower::Soulfeast,
        desc: "Every kill feeds you: +3 HP and +2 MP.",
        color: Rgb(220, 180, 120),
    },
    UniqueDef {
        name: "Grutch's Cleaver",
        slot: USlot::Weapon,
        base: 4,
        bonus: 2,
        power: UPower::Executioner,
        desc: "+12% critical chance; critical hits deal triple damage.",
        color: Rgb(200, 230, 120),
    },
    UniqueDef {
        name: "Locket of the Weeping Bride",
        slot: USlot::Amulet,
        base: 0,
        bonus: 0,
        power: UPower::Undying,
        desc: "Once per floor, a killing blow leaves you at 1 HP instead.",
        color: Rgb(180, 220, 255),
    },
    UniqueDef {
        name: "Mossback Hide",
        slot: USlot::Armor,
        base: 3,
        bonus: 2,
        power: UPower::Regrowth,
        desc: "Your wounds knit shut: regenerate 1 HP every turn.",
        color: Rgb(110, 190, 90),
    },
    UniqueDef {
        name: "Tidecaller",
        slot: USlot::Weapon,
        base: 6,
        bonus: 2,
        power: UPower::Frost,
        desc: "Hits have a 25% chance to freeze the foe solid.",
        color: Rgb(120, 210, 255),
    },
    UniqueDef {
        name: "Vyrmatha's Fang",
        slot: USlot::Weapon,
        base: 0,
        bonus: 4,
        power: UPower::Vampiric,
        desc: "Drinks blood: heal 20% of the damage you deal.",
        color: Rgb(220, 80, 200),
    },
    UniqueDef {
        name: "Obsidian Bulwark",
        slot: USlot::Armor,
        base: 7,
        bonus: 1,
        power: UPower::Stoneskin,
        desc: "Every blow against you is reduced by 2.",
        color: Rgb(120, 100, 150),
    },
    UniqueDef {
        name: "Cinderbrand",
        slot: USlot::Weapon,
        base: 12,
        bonus: 2,
        power: UPower::Flame,
        desc: "Your strikes burn for 1d6 + depth/2 extra fire damage.",
        color: Rgb(255, 130, 40),
    },
    UniqueDef {
        name: "Hoof of Asterion",
        slot: USlot::Ring,
        base: 0,
        bonus: 0,
        power: UPower::Swift,
        desc: "Your steps are swift: moving takes 30% less time.",
        color: Rgb(230, 110, 80),
    },
    UniqueDef {
        name: "Malzareth's Phylactery",
        slot: USlot::Amulet,
        base: 0,
        bonus: 0,
        power: UPower::Arcane,
        desc: "Skills cost half mana, and mana returns faster.",
        color: Rgb(200, 130, 255),
    },
    UniqueDef {
        name: "Briarmail",
        slot: USlot::Armor,
        base: 4,
        bonus: 3,
        power: UPower::Thorns,
        desc: "Foes that strike you in melee are torn by thorns.",
        color: Rgb(140, 170, 90),
    },
    UniqueDef {
        name: "Stormcaller",
        slot: USlot::Weapon,
        base: 2,
        bonus: 3,
        power: UPower::Storm,
        desc: "Hits arc lightning into another nearby foe.",
        color: Rgb(170, 200, 255),
    },
    UniqueDef {
        name: "Eye of the Deep",
        slot: USlot::Amulet,
        base: 0,
        bonus: 0,
        power: UPower::Deathsight,
        desc: "You sense every living thing on the floor (maps).",
        color: Rgb(120, 255, 200),
    },
    UniqueDef {
        name: "Ring of Avarice",
        slot: USlot::Ring,
        base: 0,
        bonus: 0,
        power: UPower::Fortune,
        desc: "Slain foes drop treasure far more often.",
        color: Rgb(255, 215, 80),
    },
    UniqueDef {
        name: "Shadowweave",
        slot: USlot::Armor,
        base: 2,
        bonus: 3,
        power: UPower::Shadow,
        desc: "You move like a shadow: +3 stealth.",
        color: Rgb(110, 110, 160),
    },
    UniqueDef {
        name: "Wrathstone Band",
        slot: USlot::Ring,
        base: 0,
        bonus: 0,
        power: UPower::Wrath,
        desc: "Burning fury: +3 damage to every strike.",
        color: Rgb(255, 80, 60),
    },
];

/// Number of signature items (the rest form the depth-10 champion pool).
pub const N_SIGNATURE: usize = 10;

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
    WeaponDef {
        name: "Dagger",
        art: WArt::Dagger,
        dmg: (1, 4),
        hit: 2,
        speed: 80.0,
        crit: 0.15,
        stun: 0.0,
        range: 0,
        depth: 1,
        color: STEEL,
        glow: false,
    },
    WeaponDef {
        name: "Short Sword",
        art: WArt::Blade,
        dmg: (2, 6),
        hit: 1,
        speed: 100.0,
        crit: 0.08,
        stun: 0.0,
        range: 0,
        depth: 1,
        color: STEEL,
        glow: false,
    },
    WeaponDef {
        name: "Long Sword",
        art: WArt::Blade,
        dmg: (3, 8),
        hit: 0,
        speed: 100.0,
        crit: 0.08,
        stun: 0.0,
        range: 0,
        depth: 2,
        color: STEEL,
        glow: false,
    },
    WeaponDef {
        name: "Mace",
        art: WArt::Blunt,
        dmg: (3, 7),
        hit: 0,
        speed: 110.0,
        crit: 0.05,
        stun: 0.15,
        range: 0,
        depth: 1,
        color: Rgb(170, 170, 180),
        glow: false,
    },
    WeaponDef {
        name: "Battle Axe",
        art: WArt::Axe,
        dmg: (4, 10),
        hit: -1,
        speed: 120.0,
        crit: 0.1,
        stun: 0.0,
        range: 0,
        depth: 3,
        color: STEEL,
        glow: false,
    },
    WeaponDef {
        name: "War Hammer",
        art: WArt::Blunt,
        dmg: (5, 12),
        hit: -1,
        speed: 130.0,
        crit: 0.05,
        stun: 0.25,
        range: 0,
        depth: 4,
        color: Rgb(150, 150, 160),
        glow: false,
    },
    WeaponDef {
        name: "Spear",
        art: WArt::Spear,
        dmg: (3, 9),
        hit: 1,
        speed: 105.0,
        crit: 0.07,
        stun: 0.0,
        range: 0,
        depth: 2,
        color: STEEL,
        glow: false,
    },
    WeaponDef {
        name: "Greatsword",
        art: WArt::Blade,
        dmg: (6, 14),
        hit: -1,
        speed: 140.0,
        crit: 0.1,
        stun: 0.05,
        range: 0,
        depth: 5,
        color: STEEL,
        glow: false,
    },
    WeaponDef {
        name: "Quarterstaff",
        art: WArt::Staff,
        dmg: (2, 6),
        hit: 1,
        speed: 100.0,
        crit: 0.05,
        stun: 0.05,
        range: 0,
        depth: 1,
        color: WOOD,
        glow: true,
    },
    WeaponDef {
        name: "Short Bow",
        art: WArt::Bow,
        dmg: (2, 6),
        hit: 1,
        speed: 100.0,
        crit: 0.08,
        stun: 0.0,
        range: 8,
        depth: 1,
        color: WOOD,
        glow: false,
    },
    WeaponDef {
        name: "Long Bow",
        art: WArt::Bow,
        dmg: (3, 9),
        hit: 1,
        speed: 110.0,
        crit: 0.1,
        stun: 0.0,
        range: 10,
        depth: 4,
        color: WOOD,
        glow: false,
    },
    WeaponDef {
        name: "Crossbow",
        art: WArt::Bow,
        dmg: (5, 11),
        hit: 2,
        speed: 150.0,
        crit: 0.12,
        stun: 0.0,
        range: 8,
        depth: 5,
        color: Rgb(120, 90, 70),
        glow: false,
    },
    WeaponDef {
        name: "Runeblade",
        art: WArt::Blade,
        dmg: (5, 12),
        hit: 2,
        speed: 100.0,
        crit: 0.12,
        stun: 0.0,
        range: 0,
        depth: 7,
        color: Rgb(120, 180, 255),
        glow: true,
    },
    WeaponDef {
        name: "Soulreaver",
        art: WArt::Axe,
        dmg: (8, 17),
        hit: 1,
        speed: 120.0,
        crit: 0.15,
        stun: 0.0,
        range: 0,
        depth: 9,
        color: Rgb(200, 90, 255),
        glow: true,
    },
];

pub struct ArmorDef {
    pub name: &'static str,
    pub ac: i32,
    pub depth: u32,
}

pub static ARMORS: &[ArmorDef] = &[
    ArmorDef {
        name: "Rags",
        ac: 0,
        depth: 99,
    },
    ArmorDef {
        name: "Robe",
        ac: 1,
        depth: 1,
    },
    ArmorDef {
        name: "Leather Armor",
        ac: 2,
        depth: 1,
    },
    ArmorDef {
        name: "Studded Leather",
        ac: 3,
        depth: 2,
    },
    ArmorDef {
        name: "Chain Mail",
        ac: 4,
        depth: 3,
    },
    ArmorDef {
        name: "Scale Mail",
        ac: 5,
        depth: 5,
    },
    ArmorDef {
        name: "Mithril Coat",
        ac: 6,
        depth: 7,
    },
    ArmorDef {
        name: "Plate Armor",
        ac: 7,
        depth: 8,
    },
];

// ---------------------------------------------------------------- items

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Gear {
    pub def: usize,
    pub bonus: i32,
    /// Index into UNIQUES if this is a unique piece.
    pub uniq: Option<u8>,
}

impl Gear {
    pub fn plain(def: usize, bonus: i32) -> Gear {
        Gear {
            def,
            bonus,
            uniq: None,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Potion {
    Healing,
    Mana,
    Might,
    Vigor,
    Cure,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scroll {
    Teleport,
    Mapping,
    Fire,
    Vision,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Weapon(Gear),
    Armor(Gear),
    Potion(Potion),
    Scroll(Scroll),
    Amulet,
    /// A unique ring or amulet (index into UNIQUES).
    Trinket(u8),
}

/// Builds the item for unique `u`.
pub fn unique_item(u: usize) -> Item {
    let d = &UNIQUES[u];
    match d.slot {
        USlot::Weapon => Item::Weapon(Gear {
            def: d.base,
            bonus: d.bonus,
            uniq: Some(u as u8),
        }),
        USlot::Armor => Item::Armor(Gear {
            def: d.base,
            bonus: d.bonus,
            uniq: Some(u as u8),
        }),
        USlot::Ring | USlot::Amulet => Item::Trinket(u as u8),
    }
}

/// Display name of a weapon or armour, including unique names.
pub fn gear_label(base: &str, g: &Gear) -> String {
    match g.uniq {
        Some(u) => UNIQUES[u as usize].name.to_string(),
        None => gear_name(base, g.bonus),
    }
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
const ART_RING: &[&str] = &[r" .-. ", r"( § )", r" '-' "];
const ART_AMULET: &[&str] = &[r" .-~-. ", r"(     )", r" \   / ", r"  (§)  "];
pub const ART_CORPSE: &[&str] = &[r"    ,%%%,    ", r" _,(%%%%%),_ "];
pub const ART_BONES: &[&str] = &[r"  o   ,=-,  ", r"='-%=--o-'=-"];

impl Item {
    pub fn name(&self) -> String {
        match self {
            Item::Weapon(g) => gear_label(WEAPONS[g.def].name, g),
            Item::Armor(g) => gear_label(ARMORS[g.def].name, g),
            Item::Trinket(u) => UNIQUES[*u as usize].name.to_string(),
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
                    Scroll::Vision => "Monster Vision",
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
            Item::Trinket(u) => {
                if UNIQUES[*u as usize].slot == USlot::Ring {
                    '='
                } else {
                    '"'
                }
            }
        }
    }

    pub fn color(&self) -> Rgb {
        match self {
            Item::Weapon(Gear { uniq: Some(u), .. })
            | Item::Armor(Gear { uniq: Some(u), .. })
            | Item::Trinket(u) => UNIQUES[*u as usize].color,
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
            Item::Trinket(u) => {
                if UNIQUES[*u as usize].slot == USlot::Ring {
                    ART_RING
                } else {
                    ART_AMULET
                }
            }
        }
    }

    /// The unique this item is, if any.
    pub fn unique(&self) -> Option<usize> {
        match self {
            Item::Weapon(Gear { uniq: Some(u), .. })
            | Item::Armor(Gear { uniq: Some(u), .. })
            | Item::Trinket(u) => Some(*u as usize),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- skills

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
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
    BattleCry,
    Shadowstep,
    SmokeBomb,
    ChainLightning,
    Smite,
    PiercingShot,
    RainOfArrows,
    CorpseExplosion,
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
            Skill::PhaseWard => "Holy Ward",
            Skill::SecondWind => "Second Wind",
            Skill::BattleCry => "Battle Cry",
            Skill::Shadowstep => "Shadowstep",
            Skill::SmokeBomb => "Smoke Bomb",
            Skill::ChainLightning => "Chain Lightning",
            Skill::Smite => "Smite",
            Skill::PiercingShot => "Piercing Shot",
            Skill::RainOfArrows => "Rain of Arrows",
            Skill::CorpseExplosion => "Corpse Explosion",
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
            Skill::Blink => 4,
            Skill::SoulBurst => 7,
            Skill::PhaseWard => 5,
            Skill::BattleCry => 4,
            Skill::Shadowstep => 5,
            Skill::SmokeBomb => 6,
            Skill::ChainLightning => 8,
            Skill::Smite => 6,
            Skill::PiercingShot => 5,
            Skill::RainOfArrows => 8,
            Skill::CorpseExplosion => 6,
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
            Skill::Blink => 12.0,
            Skill::SoulBurst => 20.0,
            Skill::PhaseWard => 30.0,
            Skill::SecondWind => 120.0,
            Skill::BattleCry => 25.0,
            Skill::Shadowstep => 8.0,
            Skill::SmokeBomb => 30.0,
            Skill::ChainLightning => 6.0,
            Skill::Smite => 4.0,
            Skill::PiercingShot => 5.0,
            Skill::RainOfArrows => 12.0,
            Skill::CorpseExplosion => 10.0,
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
            Skill::SoulBurst => {
                "Blast every foe within 2 tiles with soulfire; heals you for each foe hit."
            }
            Skill::PhaseWard => "A holy ward absorbs the next 20+ damage.",
            Skill::SecondWind => "Instantly heal half your health and shrug off poison.",
            Skill::BattleCry => {
                "A terrible roar: nearby foes flee, and you deal +50% damage for 8 turns."
            }
            Skill::Shadowstep => {
                "Vanish and reappear behind a visible foe, striking it as an unseen backstab."
            }
            Skill::SmokeBomb => {
                "Foes within 4 tiles are blinded and lose you; you vanish for 5 turns."
            }
            Skill::ChainLightning => {
                "A bolt that leaps between up to 5 foes, weakening with each jump."
            }
            Skill::Smite => {
                "Call down holy fire on the foe ahead. Double damage to undead and demons."
            }
            Skill::PiercingShot => "An arrow that tears through every foe in a line.",
            Skill::RainOfArrows => {
                "Arrows rain on the area ahead, striking every foe within 1 tile."
            }
            Skill::CorpseExplosion => {
                "Detonate every corpse within 6 tiles. The dead never rise again."
            }
        }
    }

    /// The class that can learn this skill at the Soul Altar.
    pub fn class(self) -> Option<usize> {
        match self {
            Skill::SecondWind | Skill::BattleCry => Some(0),
            Skill::Shadowstep | Skill::SmokeBomb => Some(1),
            Skill::Blink | Skill::ChainLightning => Some(2),
            Skill::PhaseWard | Skill::Smite => Some(3),
            Skill::PiercingShot | Skill::RainOfArrows => Some(4),
            Skill::SoulBurst | Skill::CorpseExplosion => Some(5),
            _ => None,
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
        name: "Warrior",
        desc: "A hardened fighter in chain mail. High health, strong melee, few tricks.",
        hp: 30,
        mp: 6,
        hp_lvl: 8,
        mp_lvl: 1,
        hit: 4,
        weapon: 2,
        armor: 4,
        skills: [Skill::Cleave, Skill::WarStomp],
        unlock: None,
        stealth: 0,
        color: Rgb(220, 90, 70),
    },
    ClassDef {
        name: "Rogue",
        desc: "Quick and quiet. Spots traps, and backstabs unaware foes for triple damage.",
        hp: 22,
        mp: 10,
        hp_lvl: 5,
        mp_lvl: 2,
        hit: 5,
        weapon: 0,
        armor: 2,
        skills: [Skill::ShadowVeil, Skill::FanOfKnives],
        unlock: None,
        stealth: 3,
        color: Rgb(150, 150, 170),
    },
    ClassDef {
        name: "Mage",
        desc: "Frail but devastating. Fireballs and frost, with fast mana regeneration.",
        hp: 16,
        mp: 22,
        hp_lvl: 4,
        mp_lvl: 4,
        hit: 1,
        weapon: 8,
        armor: 1,
        skills: [Skill::Fireball, Skill::FrostNova],
        unlock: None,
        stealth: 1,
        color: Rgb(100, 140, 255),
    },
    ClassDef {
        name: "Cleric",
        desc: "A holy warrior. Heals wounds and burns the undead with sacred light.",
        hp: 26,
        mp: 14,
        hp_lvl: 6,
        mp_lvl: 3,
        hit: 3,
        weapon: 3,
        armor: 3,
        skills: [Skill::Heal, Skill::TurnUndead],
        unlock: None,
        stealth: 0,
        color: Rgb(240, 220, 120),
    },
    ClassDef {
        name: "Ranger",
        desc: "Archer of the deep wilds. Fights at range and summons a wolf companion.",
        hp: 22,
        mp: 12,
        hp_lvl: 5,
        mp_lvl: 2,
        hit: 5,
        weapon: 9,
        armor: 2,
        skills: [Skill::Multishot, Skill::SummonWolf],
        unlock: Some("class_ranger"),
        stealth: 2,
        color: Rgb(90, 200, 90),
    },
    ClassDef {
        name: "Necromancer",
        desc: "Master of death. Raises corpses as thralls and drains the living.",
        hp: 18,
        mp: 18,
        hp_lvl: 4,
        mp_lvl: 4,
        hit: 2,
        weapon: 0,
        armor: 1,
        skills: [Skill::RaiseDead, Skill::DrainLife],
        unlock: Some("class_necro"),
        stealth: 1,
        color: Rgb(150, 90, 200),
    },
];

// ---------------------------------------------------------------- profile upgrades

/// Sections of the Soul Altar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UCat {
    /// Plain stat bonuses.
    Stat,
    /// Boons that change how a run plays.
    Boon,
    /// Unlock a new class.
    Class,
    /// A skill only one class can learn.
    Skill(usize),
}

pub struct UpgradeDef {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub max: u32,
    pub cost: u32,
    pub skill: Option<Skill>,
    pub cat: UCat,
    /// Fixed price per rank, overriding the usual cost x rank^2.
    pub ranks: &'static [u32],
}

const fn stat(
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    max: u32,
    cost: u32,
) -> UpgradeDef {
    UpgradeDef {
        id,
        name,
        desc,
        max,
        cost,
        skill: None,
        cat: UCat::Stat,
        ranks: &[],
    }
}
const fn boon(
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    max: u32,
    cost: u32,
) -> UpgradeDef {
    UpgradeDef {
        id,
        name,
        desc,
        max,
        cost,
        skill: None,
        cat: UCat::Boon,
        ranks: &[],
    }
}
const fn cskill(id: &'static str, class: usize, s: Skill, cost: u32) -> UpgradeDef {
    UpgradeDef {
        id,
        name: "",
        desc: "",
        max: 1,
        cost,
        skill: Some(s),
        cat: UCat::Skill(class),
        ranks: &[],
    }
}

pub static UPGRADES: &[UpgradeDef] = &[
    stat("vitality", "Vitality", "+4 max HP per rank", 5, 40),
    stat("might", "Might", "+1 damage per rank", 5, 50),
    stat("ironskin", "Iron Skin", "+1 armor class per rank", 3, 70),
    stat("arcana", "Arcane Well", "+3 max MP per rank", 5, 35),
    stat("keen", "Keen Eyes", "+1 sight radius and +3% crit per rank", 3, 50),
    stat("scholar", "Scholar", "+10% experience per rank", 5, 45),
    stat("regen", "Troll Blood", "Faster health regeneration", 3, 70),
    stat("provision", "Provisioned", "+1 starting Potion of Healing per rank", 3, 40),
    stat("luck", "Fortune", "More and better loot", 3, 60),
    stat("siphon", "Soul Siphon", "Heal 1 HP per kill per rank", 3, 60),
    boon("harvest", "Soul Harvest", "+10% soul points from every descent per rank", 3, 80),
    boon("bloodlust", "Bloodlust", "Each killing blow has a 10% chance per rank to cost no time at all", 3, 90),
    boon("pockets", "Deep Pockets", "+3 pack slots per rank", 2, 60),
    UpgradeDef {
        ranks: &[1500, 5000],
        ..boon("carto", "Cartographer", "Rank 1: the way down is revealed on every floor. Rank 2: every room is mapped too", 2, 1500)
    },
    boon("anchor", "Soul Anchor", "Once per descent, cheat death: rise with 40% health as nearby foes flee", 1, 600),
    boon("coup", "Coup de Grace", "+15% damage per rank to foes below 30% health", 3, 80),
    boon("alchemy", "Alchemy", "Potions are 25% more potent per rank", 3, 70),
    boon("thorns", "Thorned Hide", "Monsters that strike you in melee take 1-3 damage per rank", 3, 70),
    boon("bloodmagic", "Blood Magic", "When short of mana, pay for skills with 2 health per missing point", 1, 300),
    boon("hunter", "Champion Hunter", "+15% damage per rank against champions, overlords and uniques", 3, 90),
    boon("heirloom", "Heirloom Arms", "Your starting weapon and armour are +1 per rank", 3, 60),
    boon("ironwill", "Iron Will", "Rank 1: poison wears off twice as fast. Rank 2: immune to poison", 2, 70),
    boon("leech", "Mana Leech", "+1 MP per kill per rank", 3, 60),
    boon("laststand", "Last Stand", "Below 25% health: +15% damage and +1 AC per rank", 3, 80),
    boon("fleet", "Fleet Foot", "Walking takes 10% less time per rank", 2, 120),
    boon("scavenger", "Scavenger", "Start each descent with one random scroll per rank", 3, 50),
    UpgradeDef { id: "class_ranger", name: "Unlock: Ranger", desc: "New class: archer with a wolf companion", max: 1, cost: 400, skill: None, cat: UCat::Class, ranks: &[] },
    UpgradeDef { id: "class_necro", name: "Unlock: Necromancer", desc: "New class: raise the dead, drain life", max: 1, cost: 700, skill: None, cat: UCat::Class, ranks: &[] },
    cskill("w_wind", 0, Skill::SecondWind, 350),
    cskill("w_cry", 0, Skill::BattleCry, 450),
    cskill("r_step", 1, Skill::Shadowstep, 400),
    cskill("r_smoke", 1, Skill::SmokeBomb, 350),
    cskill("m_blink", 2, Skill::Blink, 300),
    cskill("m_chain", 2, Skill::ChainLightning, 500),
    cskill("c_ward", 3, Skill::PhaseWard, 300),
    cskill("c_smite", 3, Skill::Smite, 450),
    cskill("g_pierce", 4, Skill::PiercingShot, 400),
    cskill("g_rain", 4, Skill::RainOfArrows, 500),
    cskill("n_corpse", 5, Skill::CorpseExplosion, 450),
    cskill("n_burst", 5, Skill::SoulBurst, 400),
    UpgradeDef {
        id: "n_rebind",
        name: "Undying Bond",
        desc: "Passive: the body of a fallen thrall can be raised one more time, at about 50% more max HP upkeep",
        max: 1,
        cost: 500,
        skill: None,
        cat: UCat::Skill(5),
        ranks: &[],
    },
];

impl UpgradeDef {
    /// Display name; class skills are named after their skill.
    pub fn title(&self) -> String {
        match self.skill {
            Some(s) => s.name().to_string(),
            None => self.name.to_string(),
        }
    }
    pub fn text(&self) -> String {
        match self.skill {
            Some(s) => format!(
                "{} ({} mp, {} turn cooldown)",
                s.desc(),
                s.mp(),
                s.cooldown()
            ),
            None => self.desc.to_string(),
        }
    }
}

/// Upgrades from older versions of the altar, refunded when the profile loads.
pub static LEGACY_UPGRADES: &[(&str, u32)] =
    &[("blink", 250), ("burst", 350), ("ward", 300), ("wind", 400)];

// ---------------------------------------------------------------- difficulty

pub struct DiffDef {
    pub name: &'static str,
    pub desc: &'static str,
    /// Monster health multiplier.
    pub hp: f32,
    /// Monster damage multiplier.
    pub dmg: f32,
    /// Soul point multiplier.
    pub souls: f32,
    pub color: Rgb,
}

pub static DIFFICULTIES: [DiffDef; 4] = [
    DiffDef {
        name: "Normal",
        desc: "The dungeon as GRAZZARK made it. Souls come 20% thinner here.",
        hp: 1.0,
        dmg: 1.0,
        souls: 0.8,
        color: Rgb(170, 220, 170),
    },
    DiffDef {
        name: "Hard",
        desc: "Monsters have 30% more health and deal 20% more damage.",
        hp: 1.3,
        dmg: 1.2,
        souls: 1.5,
        color: Rgb(240, 210, 110),
    },
    DiffDef {
        name: "Difficult",
        desc: "Monsters have 60% more health and deal 40% more damage.",
        hp: 1.6,
        dmg: 1.4,
        souls: 2.0,
        color: Rgb(255, 140, 70),
    },
    DiffDef {
        name: "Chaotic",
        desc: "Monsters have double health and deal 70% more damage.",
        hp: 2.0,
        dmg: 1.7,
        souls: 3.0,
        color: Rgb(255, 60, 90),
    },
];

/// Wall styles near the surface (depths 1-3): cellars, crypts, gaols, sewers.
pub const SHALLOW_THEMES: [u8; 4] = [0, 6, 7, 8];
/// The middle depths (4-6): caves, mines, ice and drowned ruins.
pub const MIDDLE_THEMES: [u8; 5] = [1, 4, 9, 10, 11];
/// The deep (7-9): the strangest places of all.
pub const DEEP_THEMES: [u8; 6] = [2, 5, 12, 13, 14, 15];
/// Every style a floor above the Sanctum can have.
#[allow(dead_code)]
pub const FLOOR_THEMES: [u8; 15] = [0, 6, 7, 8, 1, 4, 9, 10, 11, 2, 5, 12, 13, 14, 15];
pub const SANCTUM_STYLE: u8 = 3;

/// The names a theme's floors take, in the order you reach them.
fn theme_names(style: u8) -> [&'static str; 4] {
    match style {
        0 => [
            "The Old Cellars",
            "The Rat Warrens",
            "The Forgotten Crypts",
            "The Drowned Cisterns",
        ],
        1 => [
            "The Mossy Halls",
            "The Sunken Galleries",
            "The Fungal Deep",
            "The Rotting Gardens",
        ],
        2 => [
            "The Obsidian Vaults",
            "The Molten Stair",
            "The Halls of Torment",
            "The Ember Forges",
        ],
        4 => [
            "The Frozen Catacombs",
            "The Rime Halls",
            "The Glacial Tombs",
            "The Howling Cold",
        ],
        5 => [
            "The Glittering Grotto",
            "The Crystal Caverns",
            "The Prism Deep",
            "The Shard Halls",
        ],
        6 => [
            "The Old Crypt",
            "The Burial Vaults",
            "The Catacombs",
            "The Charnel Niches",
        ],
        7 => [
            "The Dank Dungeon",
            "The Oubliettes",
            "The Weeping Cells",
            "The Gaol Pits",
        ],
        8 => [
            "The Sewers",
            "The Cistern Tunnels",
            "The Overflow",
            "The Drowned Drains",
        ],
        9 => [
            "The Fungal Grotto",
            "The Glowcap Caves",
            "The Spore Gardens",
            "The Mycelium Deep",
        ],
        10 => [
            "The Drowned Temple",
            "The Flooded Nave",
            "The Tidal Sanctum",
            "The Sunken Shrine",
        ],
        11 => [
            "The Old Mines",
            "The Collapsed Shafts",
            "The Dwarven Workings",
            "The Deep Seam",
        ],
        12 => [
            "The Ossuary",
            "The Bone Galleries",
            "The Halls of the Dead",
            "The Skull Stacks",
        ],
        13 => [
            "The Starless Abyss",
            "The Void Halls",
            "The Edge of Nothing",
            "The Black Between",
        ],
        14 => [
            "The Blood Temple",
            "The Crimson Nave",
            "The Altar of Veins",
            "The Scarlet Halls",
        ],
        15 => [
            "The Infernal Foundry",
            "The Iron Furnaces",
            "The Slag Halls",
            "The Hellforge",
        ],
        _ => ["The Sanctum of GRAZZARK"; 4],
    }
}

/// Lay out the themes for depths 1-9. Each tier of three floors (near the
/// surface, the middle depths, the deep) draws from its own pool, using one
/// theme for all three floors or two themes split 2+1.
pub fn roll_themes(rng: &mut rand::rngs::StdRng) -> Vec<u8> {
    use rand::seq::SliceRandom;
    use rand::Rng;
    let mut v = Vec::with_capacity(9);
    for pool in [&SHALLOW_THEMES[..], &MIDDLE_THEMES[..], &DEEP_THEMES[..]] {
        let picks: Vec<u8> = pool.choose_multiple(rng, 2).copied().collect();
        match rng.gen_range(0..3) {
            0 => v.extend([picks[0]; 3]),
            1 => v.extend([picks[0], picks[0], picks[1]]),
            _ => v.extend([picks[0], picks[1], picks[1]]),
        }
    }
    v
}

/// The themes every run had before they were shuffled.
pub fn classic_themes() -> Vec<u8> {
    vec![0, 0, 0, 1, 1, 1, 2, 2, 2]
}

/// Name of a floor, from its depth and the run's theme layout.
pub fn floor_name(themes: &[u8], depth: u32) -> &'static str {
    if depth >= 10 || depth == 0 {
        return theme_names(SANCTUM_STYLE)[0];
    }
    let i = (depth - 1) as usize;
    let style = themes.get(i).copied().unwrap_or(0);
    // Position of this floor within its band.
    let mut k = 0;
    while k < i && themes.get(i - k - 1) == Some(&style) {
        k += 1;
    }
    theme_names(style)[k.min(3)]
}

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

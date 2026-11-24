//! Monster sprite animation: blinking, attacking, walking and dying.
//!
//! A sprite's ASCII art never changes; instead each frame gets a `Pose` (a
//! stretch, a lean, a tilt, flailing arm rows, stepping legs, a jaw dropping
//! open, a body slumping to the floor...) and the renderer reads the art
//! through it with `sample`. How each monster moves is worked out from what
//! it is (its material, whether it flies, its shape), so new monsters pick
//! up sensible animations without any extra data.

use crate::color::{hashf, Rgb};
use crate::data::*;
use std::f32::consts::{FRAC_PI_2, PI};

/// How long a death takes to play out, in seconds.
pub const DEATH_TIME: f32 = 1.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttackStyle {
    /// Jaws gape open, then snap shut.
    Bite,
    /// A jelly stretches out toward its prey.
    Stretch,
    /// Arms thrash wildly (the walking dead).
    Flail,
    /// Swoops down at you.
    Swoop,
    /// Swells and flares.
    Pulse,
    /// Leans into a swing.
    Swing,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WalkStyle {
    /// Upright, legs stepping.
    Walk,
    /// Heavy and swaying.
    Lumber,
    /// Bounding along in hops, squashing as it lands.
    Hop,
    /// Many legs, very quick.
    Scuttle,
    /// A body rippling from side to side.
    Slither,
    /// Drifting, swaying in the air.
    Glide,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeathStyle {
    /// Knees go, then the body folds down into a heap.
    Crumple,
    /// Keels over sideways like a felled tree.
    Topple,
    /// Drops out of the air, then crumples.
    Fall,
    /// Flattens into a puddle.
    Splat,
    /// Clatters apart into a heap of bones.
    Collapse,
    /// Thins into mist and is gone.
    Dissipate,
    /// Gutters and burns out.
    BurnOut,
    /// Cracks and breaks apart.
    Shatter,
    /// Flips onto its back, legs curling.
    FlipOver,
    /// Wilts and sags.
    Wilt,
    /// The Mimic: lid flung wide, tongue lolling out over its teeth.
    Mimic,
}

/// Everything about how one kind of monster moves.
#[derive(Clone, Copy, Debug)]
pub struct Anim {
    pub blinks: bool,
    pub attack: AttackStyle,
    pub walk: WalkStyle,
    pub death: DeathStyle,
}

fn four_legged(name: &str) -> bool {
    matches!(
        name,
        "giant rat" | "dire wolf" | "hellhound" | "cave bear" | "basilisk" | "Gnawbone, the Rat King"
    )
}

pub fn anim_of(kind: usize) -> Anim {
    let def = &MONSTERS[kind];
    let mat = def.material();
    let bleed = def.bleed();
    let flies = def.float > 0.0;
    let has_eyes = def.art.iter().any(|l| l.contains('§'));
    // Only living things with lids blink: not the dead, not stone, not bugs.
    let blinks = has_eyes
        && matches!(bleed, Bleed::Blood | Bleed::Demon | Bleed::Slime | Bleed::Spores);
    let attack = if kind == M_MIMIC || four_legged(def.name) || def.name.contains("spider") {
        AttackStyle::Bite
    } else if mat == Material::Ooze {
        AttackStyle::Stretch
    } else if bleed == Bleed::Rot {
        AttackStyle::Flail
    } else if flies || mat == Material::Spirit {
        AttackStyle::Swoop
    } else if matches!(mat, Material::Flame | Material::Crystal) || bleed == Bleed::Spores {
        AttackStyle::Pulse
    } else {
        AttackStyle::Swing
    };
    let walk = if kind == M_MIMIC || mat == Material::Ooze {
        WalkStyle::Hop
    } else if def.name.contains("centipede") {
        WalkStyle::Slither
    } else if mat == Material::Shell || def.name.contains("rat") {
        WalkStyle::Scuttle
    } else if flies || mat == Material::Spirit {
        WalkStyle::Glide
    } else if def.size >= 0.95 {
        WalkStyle::Lumber
    } else {
        WalkStyle::Walk
    };
    let death = if kind == M_MIMIC {
        DeathStyle::Mimic
    } else if mat == Material::Ooze {
        DeathStyle::Splat
    } else if mat == Material::Bone {
        DeathStyle::Collapse
    } else if mat == Material::Spirit {
        DeathStyle::Dissipate
    } else if mat == Material::Flame {
        DeathStyle::BurnOut
    } else if matches!(mat, Material::Stone | Material::Crystal) {
        DeathStyle::Shatter
    } else if mat == Material::Shell {
        DeathStyle::FlipOver
    } else if bleed == Bleed::Spores {
        DeathStyle::Wilt
    } else if flies {
        DeathStyle::Fall
    } else if four_legged(def.name) || def.death == DeathKind::Crushed {
        DeathStyle::Topple
    } else {
        DeathStyle::Crumple
    };
    Anim { blinks, attack, walk, death }
}

/// A frame's worth of distortion applied to a sprite's art. Distances are in
/// art cells: columns across, rows up.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    /// Draw this art instead of the monster's own.
    pub art: Option<&'static [&'static str]>,
    pub sx: f32,
    pub sy: f32,
    /// Raised off the floor (world height, like a sprite's float).
    pub lift: f32,
    /// Columns of lean per row above the floor.
    pub shear: f32,
    /// Rotation about the feet, radians (positive leans right).
    pub tilt: f32,
    /// A side-to-side ripple running up the body.
    pub wave: f32,
    pub wave_ph: f32,
    /// Arm rows thrashing, and the phase of the thrash.
    pub arms: f32,
    pub arms_ph: f32,
    /// Leg rows stepping, and which leg is forward.
    pub legs: f32,
    /// Rows the jaw hangs open, and the art row the mouth opens at.
    pub jaw: f32,
    pub jaw_row: usize,
    pub blink: bool,
    /// 0..1: the body slumps down into a heap, each column at its own pace.
    pub slump: f32,
    /// 0..1: cells drop out of the picture.
    pub fade: f32,
    /// Blend the colour toward this.
    pub tint: Option<(Rgb, f32)>,
    pub seed: u32,
}

impl Default for Pose {
    fn default() -> Pose {
        Pose {
            art: None,
            sx: 1.0,
            sy: 1.0,
            lift: 0.0,
            shear: 0.0,
            tilt: 0.0,
            wave: 0.0,
            wave_ph: 0.0,
            arms: 0.0,
            arms_ph: 0.0,
            legs: 0.0,
            jaw: 0.0,
            jaw_row: 0,
            blink: false,
            slump: 0.0,
            fade: 0.0,
            tint: None,
            seed: 0,
        }
    }
}

impl Pose {
    /// Nothing moves the art about (a blink or a tint may still apply).
    pub fn plain(&self) -> bool {
        self.art.is_none()
            && self.sx == 1.0
            && self.sy == 1.0
            && self.shear == 0.0
            && self.tilt == 0.0
            && self.wave == 0.0
            && self.arms == 0.0
            && self.legs == 0.0
            && self.jaw <= 0.0
            && self.slump <= 0.0
            && self.fade <= 0.0
    }
}

/// The row a jaw opens at: just below the eyes.
fn mouth_row(art: &[&str]) -> usize {
    let eyes = art.iter().position(|l| l.contains('§')).unwrap_or(0);
    (eyes + 1).min(art.len().saturating_sub(1))
}

fn ease_in(x: f32) -> f32 {
    x * x
}

fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x) * (1.0 - x)
}

/// A bounce: up and down once over 0..1.
fn hump(x: f32) -> f32 {
    (x.clamp(0.0, 1.0) * PI).sin()
}

/// What a living monster is doing this frame.
pub struct Live {
    pub kind: usize,
    pub id: u32,
    pub awake: bool,
    /// 1 just after it strikes, falling to 0.
    pub atk: f32,
    /// How far it still has to slide to its square (0 when standing).
    pub moving: f32,
    pub t: f32,
}

pub fn live_pose(l: &Live) -> Pose {
    let a = anim_of(l.kind);
    let def = &MONSTERS[l.kind];
    let mut p = Pose { seed: l.id, ..Pose::default() };
    let t = l.t;
    let id = l.id as i32;
    // Blinking: every few seconds, now and then twice. Asleep, the eyes stay shut.
    if a.blinks {
        let every = 2.6 + 3.4 * hashf(id, 1);
        let ph = (t + hashf(id, 2) * every).rem_euclid(every);
        let twice = hashf(id, (t / every) as i32) < 0.3;
        p.blink = !l.awake || ph < 0.13 || (twice && (0.3..0.43).contains(&ph));
    }
    // Walking.
    let m = (l.moving * 3.0).min(1.0);
    if m > 0.0 {
        let fast = 1.0 + 0.5 * hashf(id, 3);
        match a.walk {
            WalkStyle::Walk => {
                let step = (t * 9.0 * fast).sin();
                p.legs = step.signum() * m;
                p.lift += step.abs() * 0.025 * m;
            }
            WalkStyle::Lumber => {
                let step = (t * 5.5).sin();
                p.legs = step.signum() * m;
                p.shear = step * 0.12 * m;
                p.lift += step.abs() * 0.03 * m;
            }
            WalkStyle::Hop => {
                let ph = (t * 5.0 * fast).rem_euclid(1.0);
                p.lift += hump(ph) * 0.18 * m;
                // Squashed flat as it lands and pushes off, stretched in the air.
                let squash = (1.0 - hump(ph) * 2.0).max(0.0) * 0.25 * m;
                p.sy = 1.0 - squash + hump(ph) * 0.1 * m;
                p.sx = 1.0 + squash;
            }
            WalkStyle::Scuttle => {
                p.legs = (t * 18.0).sin().signum() * m;
                p.lift += (t * 18.0).sin().abs() * 0.01 * m;
            }
            WalkStyle::Slither => {
                p.wave = 1.2 * m;
                p.wave_ph = t * 10.0;
            }
            WalkStyle::Glide => {
                p.shear = (t * 3.0).sin() * 0.15 * m;
                p.wave = 0.4 * m;
                p.wave_ph = t * 4.0;
            }
        }
    }
    // Attacking.
    if l.atk > 0.0 {
        let k = 1.0 - l.atk;
        let swell = hump(k);
        match a.attack {
            AttackStyle::Bite => {
                // Gape wide, then snap shut on the last third.
                let open = if k < 0.6 {
                    ease_out(k / 0.6)
                } else {
                    1.0 - (k - 0.6) / 0.4
                };
                let rows = def.art.len() as f32;
                p.jaw = open * (rows * 0.3).clamp(1.5, 2.5);
                p.jaw_row = if l.kind == M_MIMIC { 3 } else { mouth_row(def.art) };
                p.sx *= 1.0 + 0.1 * swell;
                p.sy *= 1.0 + 0.1 * swell;
            }
            AttackStyle::Stretch => {
                // Reaching out to you: swelling up and toward you.
                p.sx *= 1.0 + 0.35 * swell;
                p.sy *= 1.0 + 0.55 * swell;
                p.wave = 0.4 * swell;
                p.wave_ph = t * 12.0;
            }
            AttackStyle::Flail => {
                p.arms = 2.0 * swell.max(0.4 * l.atk);
                p.arms_ph = t * 30.0;
                p.shear += (t * 14.0).sin() * 0.15 * swell;
            }
            AttackStyle::Swoop => {
                p.lift -= 0.12 * swell;
                p.sx *= 1.0 + 0.2 * swell;
                p.sy *= 1.0 + 0.2 * swell;
            }
            AttackStyle::Pulse => {
                p.sx *= 1.0 + 0.25 * swell;
                p.sy *= 1.0 + 0.25 * swell;
                p.tint = Some((Rgb(255, 255, 230), 0.4 * swell));
            }
            AttackStyle::Swing => {
                let side = if l.id.is_multiple_of(2) { 1.0 } else { -1.0 };
                p.shear += side * 0.35 * (k * PI * 1.5).sin() * l.atk.min(1.0);
                p.sx *= 1.0 + 0.12 * swell;
                p.sy *= 1.0 + 0.12 * swell;
            }
        }
    }
    p
}

/// A death in progress, `k` 0..1 of the way through. `corpse` says whether a
/// body will be left behind (if not, it fades away at the end); `burning`
/// that Last Rites will take it (it reddens toward the end).
pub fn death_pose(kind: usize, id: u32, k: f32, corpse: bool, burning: bool) -> Pose {
    let a = anim_of(kind);
    let def = &MONSTERS[kind];
    let mut p = Pose { seed: id, blink: a.blinks, ..Pose::default() };
    let k = k.clamp(0.0, 1.0);
    let side = if hashf(id as i32, 4) < 0.5 { 1.0 } else { -1.0 };
    // The killing blow: a white flash.
    if k < 0.08 {
        p.tint = Some((Rgb(255, 255, 255), 0.6 * (1.0 - k / 0.08)));
    }
    match a.death {
        DeathStyle::Crumple => {
            // A stagger, the knees go, and it folds down into a heap.
            p.shear = side * 0.25 * hump(k * 1.4);
            p.slump = ease_in((k / 0.85).min(1.0)) * 0.88;
            p.sx = 1.0 + 0.25 * p.slump;
        }
        DeathStyle::Topple => {
            let fall = ease_in((k / 0.7).min(1.0));
            // A small bounce when it hits the floor.
            let bounce = if k > 0.7 { hump((k - 0.7) / 0.3) * 0.08 } else { 0.0 };
            p.tilt = side * (FRAC_PI_2 * fall - bounce);
            p.sy = 1.0 - 0.25 * fall;
            p.slump = 0.3 * ease_in(((k - 0.7) / 0.3).max(0.0));
        }
        DeathStyle::Fall => {
            // Drops out of the air, then crumples where it lands.
            let drop = ease_in((k / 0.4).min(1.0));
            p.lift = -def.float * drop;
            p.tilt = side * 0.5 * drop;
            p.slump = ease_out(((k - 0.4) / 0.5).clamp(0.0, 1.0)) * 0.85;
            p.sx = 1.0 + 0.3 * p.slump;
        }
        DeathStyle::Splat => {
            let f = ease_out((k / 0.6).min(1.0));
            p.sy = 1.0 - 0.8 * f + 0.15 * hump(k * 3.0) * (1.0 - f);
            p.sx = 1.0 + 0.7 * f;
            p.wave = 0.5 * (1.0 - f);
            p.wave_ph = k * 30.0;
        }
        DeathStyle::Collapse => {
            // A shudder, then it drops straight down into a pile of bones.
            p.shear = (k * 60.0).sin() * 0.12 * (1.0 - k);
            p.slump = ease_in((k / 0.55).min(1.0)) * 0.92;
            p.sx = 1.0 + 0.4 * p.slump;
            p.fade = 0.25 * p.slump;
        }
        DeathStyle::Dissipate => {
            p.lift = 0.15 * k;
            p.wave = 1.5 * k;
            p.wave_ph = k * 20.0;
            p.sx = 1.0 + 0.3 * k;
            p.fade = ease_in(k);
            p.tint = Some((Rgb(200, 220, 255), 0.5 * k));
        }
        DeathStyle::BurnOut => {
            p.sy = 1.0 - 0.6 * ease_in(k);
            p.wave = 0.6 * (1.0 - k);
            p.wave_ph = k * 40.0;
            p.fade = ease_in(k);
            p.tint = Some((Rgb(255, 120, 30), 0.6 * k));
        }
        DeathStyle::Shatter => {
            // Cracks run through it, then the pieces drop away.
            p.shear = (k * 70.0).sin() * 0.1 * (1.0 - k);
            p.slump = ease_in(((k - 0.25) / 0.6).clamp(0.0, 1.0)) * 0.85;
            p.fade = 0.5 * ease_in(((k - 0.25) / 0.75).clamp(0.0, 1.0));
        }
        DeathStyle::FlipOver => {
            let flip = ease_in((k / 0.5).min(1.0));
            p.tilt = side * PI * flip;
            p.lift = 0.12 * hump(k / 0.5);
            p.sy = 1.0 - 0.35 * flip;
            // Legs twitch and curl.
            p.legs = (k * 50.0).sin().signum() * (1.0 - k);
        }
        DeathStyle::Wilt => {
            p.shear = side * 0.45 * ease_in(k);
            p.slump = ease_in(k) * 0.8;
            p.tint = Some((Rgb(110, 90, 60), 0.5 * k));
        }
        DeathStyle::Mimic => {
            // The lid flies wide, the tongue lolls out, and it settles.
            p.art = Some(ART_MIMIC_DEAD);
            p.lift = 0.12 * hump((k / 0.35).min(1.0));
            p.sy = if k < 0.35 { 1.0 + 0.15 * hump(k / 0.35) } else { 1.0 };
            p.blink = false;
        }
    }
    if !corpse {
        p.fade = p.fade.max(ease_in(((k - 0.6) / 0.4).max(0.0)));
    }
    if burning {
        let heat = ((k - 0.5) / 0.5).max(0.0);
        p.tint = Some((Rgb(255, 110, 30), 0.6 * heat));
    }
    p
}

/// What a sampled cell shows: a glyph, and a colour of its own if it is not
/// the body's (teeth, tongue, the dark of a throat).
pub type Cell = (char, Option<Rgb>);

pub const IVORY: Rgb = Rgb(240, 232, 205);
const GULLET: Rgb = Rgb(70, 10, 18);
const TONGUE: Rgb = Rgb(225, 95, 120);

/// A sprite's art laid out as a grid of characters, so reading a cell is cheap.
pub struct Grid {
    cells: Vec<char>,
    pub w: usize,
    pub h: usize,
}

impl Grid {
    pub fn new(art: &[&str]) -> Grid {
        let w = art.iter().map(|l| l.chars().count()).max().unwrap_or(1).max(1);
        let h = art.len().max(1);
        let mut cells = vec![' '; w * h];
        for (r, l) in art.iter().enumerate() {
            for (c, ch) in l.chars().enumerate() {
                cells[r * w + c] = ch;
            }
        }
        Grid { cells, w, h }
    }

    fn at(&self, r: i32, c: i32) -> char {
        if r < 0 || c < 0 || r as usize >= self.h || c as usize >= self.w {
            return ' ';
        }
        self.cells[r as usize * self.w + c as usize]
    }
}

/// Glyphs turned on their side or upside down.
fn turned(ch: char, tilt: f32) -> char {
    let a = tilt.abs();
    if a > PI * 0.75 {
        match ch {
            '/' => '\\',
            '\\' => '/',
            '^' => 'v',
            'v' => '^',
            '_' => '-',
            'V' => 'A',
            'A' => 'V',
            c => c,
        }
    } else if a > PI * 0.25 {
        match ch {
            '|' => '-',
            '-' | '_' | '=' => '|',
            '/' => '\\',
            '\\' => '/',
            c => c,
        }
    } else {
        ch
    }
}

/// Read the art through a pose. `x` is columns from the sprite's centre line
/// and `y` rows up from its feet (both may be fractional).
pub fn sample(art: &Grid, p: &Pose, x: f32, y: f32) -> Option<Cell> {
    let ah = art.h as f32;
    let aw = art.w as f32;
    let (mut x, mut y) = (x, y);
    // Undo the tilt: rotate back about the feet. Cells are about twice as tall
    // as they are wide, so turn in "square" units.
    if p.tilt != 0.0 {
        let (s, c) = p.tilt.sin_cos();
        let (vx, vy) = (x * 0.5, y);
        let (rx, ry) = (vx * c - vy * s, vx * s + vy * c);
        x = rx * 2.0;
        y = ry;
    }
    if y < 0.0 {
        return None;
    }
    // Undo the stretch.
    x /= p.sx.max(0.05);
    y /= p.sy.max(0.05);
    // Each column slumps toward the floor at its own pace.
    if p.slump > 0.0 {
        let col = (x + aw / 2.0).floor() as i32;
        let pace = 0.55 + 0.45 * hashf(p.seed as i32 ^ 0x51, col);
        // Never quite flat: a heap at least a row high, showing the middle of the body.
        let squash = (1.0 - p.slump * (0.7 + 0.3 * pace)).max(1.2 / ah).max(0.06);
        y /= squash;
    }
    // Undo the lean.
    x -= p.shear * y;
    if p.wave != 0.0 {
        x -= p.wave * (y * 1.3 + p.wave_ph).sin();
    }
    let mut row = ah - y;
    let r = row.floor() as i32;
    // Thrashing arms: the upper-middle rows jerk from side to side.
    if p.arms != 0.0 && (row / ah) > 0.25 && (row / ah) < 0.65 {
        x -= p.arms * (p.arms_ph + r as f32 * 2.3).sin();
    }
    // Stepping legs: alternate rows of the bottom quarter shift.
    if p.legs != 0.0 && row / ah > 0.72 {
        let side = if r % 2 == 0 { 1.0 } else { -1.0 };
        x -= p.legs * side * (aw / 14.0).clamp(0.6, 1.2);
    }
    let col = (x + aw / 2.0).floor() as i32;
    // A gaping jaw: the head is lifted clear of the lower jaw, with teeth,
    // throat and tongue showing in between.
    if p.jaw > 0.0 {
        let j = p.jaw_row as f32;
        if row < j - p.jaw {
            row += p.jaw;
        } else if row < j {
            let below = art.at(p.jaw_row as i32, col);
            let above = art.at(p.jaw_row as i32 - 1, col);
            if below == ' ' && above == ' ' {
                return None;
            }
            let into = row - (j - p.jaw);
            let mid = (col as f32 + 0.5 - aw / 2.0).abs() < aw * 0.14;
            return Some(if into < 0.8 {
                (if col % 2 == 0 { 'V' } else { 'v' }, Some(IVORY))
            } else if mid && into > p.jaw - 1.0 {
                ('U', Some(TONGUE))
            } else {
                ('▓', Some(GULLET))
            });
        }
    }
    let r = row.floor() as i32;
    let ch = art.at(r, col);
    if ch == ' ' {
        return None;
    }
    if p.fade > 0.0 && hashf(p.seed as i32 + r * 131, col) < p.fade {
        return None;
    }
    // Marked cells in special art.
    let cell = match ch {
        '¤' => ('▓', Some(TONGUE)),
        '†' => ('V', Some(IVORY)),
        '‡' => ('A', Some(IVORY)),
        '¦' => ('▓', Some(GULLET)),
        '§' if p.blink => ('-', None),
        c => (turned(c, p.tilt), None),
    };
    Some(cell)
}

/// How far beyond its feet a posed sprite can reach, in art cells
/// (columns either side, rows above, rows below), so the renderer knows how
/// much of the screen to look at.
pub fn reach(art: &Grid, p: &Pose) -> (f32, f32, f32) {
    let ah = art.h as f32;
    let aw = art.w as f32;
    if p.plain() {
        return (aw * 0.5, ah, 0.0);
    }
    let wobble = p.shear.abs() * ah * p.sy + p.wave.abs() + p.arms.abs() + p.legs.abs() * 1.2;
    let mut side = aw * 0.5 * p.sx + wobble * p.sx + 1.0;
    let mut up = ah * p.sy + p.jaw + 1.0;
    let down = 0.5;
    if p.tilt != 0.0 {
        // Turned about the feet, it can lie anywhere within its own length.
        let len = (up * up + (side * 0.5).powi(2)).sqrt();
        side = side.max(len * 2.0);
        up = up.max(len);
    }
    (side, up, down)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(name: &str) -> usize {
        MONSTERS.iter().position(|m| m.name == name).unwrap()
    }

    #[test]
    fn styles_fit_the_creature() {
        assert!(!anim_of(kind("zombie")).blinks, "zombies never blink");
        assert!(!anim_of(kind("skeleton")).blinks, "nor do skeletons");
        assert!(anim_of(kind("goblin")).blinks);
        assert_eq!(anim_of(kind("green slime")).attack, AttackStyle::Stretch);
        assert_eq!(anim_of(kind("zombie")).attack, AttackStyle::Flail);
        assert_eq!(anim_of(M_MIMIC).attack, AttackStyle::Bite);
        assert_eq!(anim_of(M_MIMIC).walk, WalkStyle::Hop);
        assert_eq!(anim_of(M_MIMIC).death, DeathStyle::Mimic);
        assert_eq!(anim_of(kind("zombie")).death, DeathStyle::Crumple);
        assert_eq!(anim_of(kind("imp")).death, DeathStyle::Fall);
        assert_eq!(anim_of(kind("skeleton")).death, DeathStyle::Collapse);
        assert_eq!(anim_of(kind("wraith")).death, DeathStyle::Dissipate);
        assert_eq!(anim_of(kind("giant spider")).death, DeathStyle::FlipOver);
        assert_eq!(anim_of(kind("cave bear")).death, DeathStyle::Topple);
    }

    /// Draw a pose into text, one screen cell per art cell.
    pub fn draw(art: &[&str], p: &Pose) -> Vec<String> {
        let art = &Grid::new(art);
        let (side, up, down) = reach(art, p);
        let w = side.ceil() as i32;
        (-(up.ceil() as i32)..down.ceil() as i32)
            .map(|r| {
                (-w..w)
                    .map(|c| {
                        sample(art, p, c as f32 + 0.5, -(r as f32) - 0.5)
                            .map(|s| s.0)
                            .unwrap_or(' ')
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .filter(|l| !l.trim().is_empty())
            .collect()
    }

    #[test]
    fn a_still_pose_draws_the_art_unchanged() {
        let art = MONSTERS[kind("zombie")].art;
        let got: Vec<String> = draw(art, &Pose::default()).iter().map(|l| l.trim().to_string()).collect();
        let want: Vec<String> = art.iter().map(|l| l.trim().to_string()).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn blinking_closes_the_eyes() {
        let art = MONSTERS[kind("goblin")].art;
        let open = draw(art, &Pose::default()).join("\n");
        let shut = draw(art, &Pose { blink: true, ..Pose::default() }).join("\n");
        assert!(open.contains('§') && !shut.contains('§'));
    }

    #[test]
    fn a_biting_mimic_shows_its_teeth() {
        let p = Pose { jaw: 2.0, jaw_row: 3, ..Pose::default() };
        let art = &Grid::new(MONSTERS[M_MIMIC].art);
        let (side, up, _) = reach(art, &p);
        let mut teeth = 0;
        for r in 0..up.ceil() as i32 {
            for c in -side.ceil() as i32..side.ceil() as i32 {
                if let Some((_, Some(col))) = sample(art, &p, c as f32 + 0.5, r as f32 + 0.5) {
                    if col == IVORY {
                        teeth += 1;
                    }
                }
            }
        }
        assert!(teeth >= 6, "only {} teeth", teeth);
    }

    #[test]
    fn the_dying_end_low_or_gone() {
        for k in 0..MONSTERS.len() {
            if anim_of(k).death == DeathStyle::Mimic {
                continue;
            }
            let p = death_pose(k, 7, 1.0, true, false);
            let art = &Grid::new(MONSTERS[k].art);
            let ah = art.h as f32;
            // How much of the body still stands above half its height.
            let (mut high, mut total) = (0, 0);
            for r in 0..(ah as i32 * 3) {
                for c in -40..40 {
                    if sample(art, &p, c as f32 + 0.5, r as f32 + 0.5).is_some() {
                        total += 1;
                        if (r as f32) > ah * 0.55 {
                            high += 1;
                        }
                    }
                }
            }
            assert!(
                high * 4 <= total.max(1),
                "{} still stands at the end of its death ({} of {} cells high)",
                MONSTERS[k].name,
                high,
                total
            );
        }
    }
}

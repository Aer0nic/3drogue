//! Rendering of the game view: an ASCII raycaster with textured walls,
//! floor/ceiling casting, coloured dynamic lighting and scaled ASCII sprites;
//! the overhead map; the HUD and in-game overlays.

use crate::canvas::{Canvas, Cell};
use crate::color::{hash2, hashf, vnoise, Rgb};
use crate::data::*;
use crate::entity::{Faction, Monster};
use crate::game::*;
use crate::map::*;
use crate::anim::{death_pose, live_pose, Live, Pose};
use std::f32::consts::PI;

pub const HUD_H: i32 = 7;
const FOV: f32 = 1.15;
const WALL_RAMP: [char; 9] = ['.', ':', '-', '=', '+', '*', '#', '%', '@'];
const FLOOR_RAMP: [char; 6] = ['.', '.', ',', ',', ':', ';'];
const CEIL_RAMP: [char; 5] = ['`', '`', '\'', '-', '~'];
const ARROWS: [char; 8] = ['^', '/', '>', '\\', 'v', '/', '<', '\\'];

pub struct Palette {
    pub wall: Rgb,
    pub floor: Rgb,
    pub ceil: Rgb,
    pub accent: Rgb,
}

pub fn palette(style: u8) -> Palette {
    match style {
        0 => Palette {
            wall: Rgb(160, 115, 85),
            floor: Rgb(125, 118, 105),
            ceil: Rgb(80, 72, 65),
            accent: Rgb(220, 160, 90),
        },
        1 => Palette {
            wall: Rgb(100, 128, 88),
            floor: Rgb(95, 108, 88),
            ceil: Rgb(55, 66, 55),
            accent: Rgb(130, 220, 120),
        },
        2 => Palette {
            wall: Rgb(88, 80, 108),
            floor: Rgb(92, 84, 100),
            ceil: Rgb(45, 38, 55),
            accent: Rgb(255, 110, 60),
        },
        4 => Palette {
            wall: Rgb(150, 175, 200),
            floor: Rgb(120, 135, 150),
            ceil: Rgb(55, 65, 85),
            accent: Rgb(150, 220, 255),
        },
        5 => Palette {
            wall: Rgb(85, 95, 110),
            floor: Rgb(80, 85, 95),
            ceil: Rgb(35, 40, 55),
            accent: Rgb(90, 240, 220),
        },
        6 => Palette {
            wall: Rgb(140, 140, 135),
            floor: Rgb(110, 108, 100),
            ceil: Rgb(55, 55, 58),
            accent: Rgb(200, 200, 190),
        },
        7 => Palette {
            wall: Rgb(105, 112, 100),
            floor: Rgb(85, 88, 80),
            ceil: Rgb(45, 48, 45),
            accent: Rgb(170, 200, 150),
        },
        8 => Palette {
            wall: Rgb(125, 95, 75),
            floor: Rgb(70, 75, 60),
            ceil: Rgb(50, 40, 35),
            accent: Rgb(140, 200, 90),
        },
        9 => Palette {
            wall: Rgb(95, 85, 100),
            floor: Rgb(80, 75, 70),
            ceil: Rgb(35, 30, 40),
            accent: Rgb(120, 255, 200),
        },
        10 => Palette {
            wall: Rgb(110, 130, 135),
            floor: Rgb(90, 110, 110),
            ceil: Rgb(40, 55, 60),
            accent: Rgb(110, 220, 230),
        },
        11 => Palette {
            wall: Rgb(115, 95, 75),
            floor: Rgb(95, 85, 70),
            ceil: Rgb(45, 38, 30),
            accent: Rgb(255, 200, 90),
        },
        12 => Palette {
            wall: Rgb(200, 190, 160),
            floor: Rgb(120, 105, 90),
            ceil: Rgb(60, 50, 45),
            accent: Rgb(230, 220, 180),
        },
        13 => Palette {
            wall: Rgb(60, 45, 90),
            floor: Rgb(50, 45, 60),
            ceil: Rgb(10, 8, 20),
            accent: Rgb(200, 140, 255),
        },
        14 => Palette {
            wall: Rgb(150, 40, 45),
            floor: Rgb(110, 50, 50),
            ceil: Rgb(50, 15, 20),
            accent: Rgb(255, 200, 80),
        },
        15 => Palette {
            wall: Rgb(95, 88, 85),
            floor: Rgb(80, 70, 65),
            ceil: Rgb(35, 30, 30),
            accent: Rgb(255, 140, 40),
        },
        _ => Palette {
            wall: Rgb(205, 195, 165),
            floor: Rgb(125, 92, 82),
            ceil: Rgb(65, 42, 42),
            accent: Rgb(180, 110, 255),
        },
    }
}

#[inline]
fn lum(l: [f32; 3]) -> f32 {
    l[0] * 0.3 + l[1] * 0.59 + l[2] * 0.11
}

fn flicker(t: f32) -> f32 {
    0.9 + 0.06 * (t * 13.0).sin() * (t * 7.3).cos() + 0.04 * (t * 23.0).sin()
}

/// Total light (RGB) at a world position: baked torches + the player's own
/// torch + dynamic lights from spells and explosions + glowing projectiles.
pub fn light_at(g: &Game, x: f32, y: f32, t: f32) -> [f32; 3] {
    let lv = &g.level;
    let fx = x - 0.5;
    let fy = y - 0.5;
    let x0 = fx.floor();
    let y0 = fy.floor();
    let ax = fx - x0;
    let ay = fy - y0;
    let (ix, iy) = (x0 as i32, y0 as i32);
    let s = |xx: i32, yy: i32| -> [f32; 3] {
        if lv.inb(xx, yy) {
            lv.light[lv.idx(xx, yy)]
        } else {
            [0.0; 3]
        }
    };
    let dark = g.p.torch_out;
    let s = |xx: i32, yy: i32| -> [f32; 3] {
        if dark {
            [0.0; 3]
        } else {
            s(xx, yy)
        }
    };
    let l00 = s(ix, iy);
    let l10 = s(ix + 1, iy);
    let l01 = s(ix, iy + 1);
    let l11 = s(ix + 1, iy + 1);
    let tf = 0.86 + 0.14 * (t * 9.0 + x * 0.7).sin() * (t * 5.3 + y).cos();
    let mut l = [0f32; 3];
    for k in 0..3 {
        let a = l00[k] + (l10[k] - l00[k]) * ax;
        let b = l01[k] + (l11[k] - l01[k]) * ax;
        l[k] = (a + (b - a) * ay) * tf;
    }
    let dx = x - g.cam.x;
    let dy = y - g.cam.y;
    let d2 = dx * dx + dy * dy;
    let s2 = (g.sight() as f32).powi(2);
    let torch = if dark || s2 <= 0.0 {
        0.0
    } else {
        1.25 / (1.0 + d2 * 7.5 / s2) * flicker(t)
    };
    let tc = if g.p.invis > 0 {
        [0.45, 0.5, 0.8]
    } else {
        [1.0, 0.8, 0.55]
    };
    for k in 0..3 {
        l[k] += tc[k] * torch;
    }
    for dl in &g.fx.lights {
        let dx = x - dl.x;
        let dy = y - dl.y;
        let d2 = dx * dx + dy * dy;
        let a = (dl.life / dl.max).clamp(0.0, 1.0) / (1.0 + d2 * 4.0 / (dl.radius * dl.radius));
        for k in 0..3 {
            l[k] += dl.color[k] * a;
        }
    }
    for b in &g.fx.bolts {
        if let Some(c) = b.light {
            let (bx, by) = b.pos_at(b.t);
            let d2 = (x - bx).powi(2) + (y - by).powi(2);
            let a = 1.0 / (1.0 + d2 * 1.2);
            for k in 0..3 {
                l[k] += c[k] * a;
            }
        }
    }
    l
}

/// Per-frame context for wall texturing.
struct WallCtx {
    t: f32,
    /// Sanctum faces: 0 still, 1 screaming now and then, 2 shrieking in torment or dead (the Amulet is taken).
    dread: u8,
    /// How far the living dungeon has rotted the walls, 0..1.
    corrupt: f32,
    /// The wall eye that is staring you down right now.
    stare: Option<(i32, i32)>,
}

/// Does the rotting wall at (mx,my) carry an eye? Only where the flesh is
/// solid all the way round it, so no hole ever eats part of the eye away.
/// The game uses this too, so only eyes you can see can stare at you.
pub fn wall_eye(mx: i32, my: i32, c: f32) -> bool {
    if c <= 0.0 || hashf(mx * 17 + 2, my * 19 + 4) >= 0.3 * c {
        return false;
    }
    let cover = c * 1.3 - hashf(mx * 7 + 3, my * 11 + 5) * 0.6;
    let flesh = |u: f32, v: f32| {
        vnoise(u * 2.5 + mx as f32 * 2.5, v * 2.5 + my as f32 * 1.9) <= cover - 0.07
    };
    if !flesh(0.5, 0.45) {
        return false;
    }
    for k in 0..12 {
        let a = k as f32 / 12.0 * std::f32::consts::TAU;
        for r in [0.5f32, 1.0, 1.3] {
            if !flesh(0.5 + a.cos() * 0.14 * r, 0.45 + a.sin() * 0.09 * r) {
                return false;
            }
        }
    }
    true
}

type Texel = (f32, Option<char>, Option<Rgb>);

/// The living dungeon's heartbeat: a double thump, 0..1.
fn heartbeat(t: f32) -> f32 {
    let p = (t * 1.1).fract();
    (1.0 - p * 7.0).max(0.0) + 0.6 * (1.0 - (p - 0.22).abs() * 9.0).max(0.0)
}

/// What one carved face in the Sanctum's brickwork is doing right now.
enum Brick {
    Still,
    /// Screaming silently; 0..1 is how wide.
    Scream(f32),
    Dead,
}

/// Each whole brick face has a small chance, cycle by cycle, of coming alive
/// in a silent scream, so a few writhe at a time and different ones each time.
/// Once the Amulet is taken they shriek in torment, and some die: those are
/// fixed by the brick itself, so a dead face stays dead.
fn brick_state(mx: i32, my: i32, row: i32, bi: i32, wc: &WallCtx) -> Brick {
    let (a, b) = (mx * 4 + bi, my * 4 + row);
    if wc.dread == 2 && hashf(a * 7 + 3, b * 11 + 5) < 0.12 {
        return Brick::Dead;
    }
    let seed = hashf(a * 13 + 1, b * 17 + 2);
    let torment = wc.dread == 2;
    let tt = wc.t / 8.0 + seed;
    let epoch = tt.floor() as i32;
    let chance = if torment { 0.15 } else { 0.09 };
    if hashf(a * 31 + epoch * 7, b * 29 - epoch * 3) >= chance {
        return Brick::Still;
    }
    let win = if torment { 0.6 } else { 0.4 };
    let ph = tt.fract();
    if ph > win {
        return Brick::Still;
    }
    let mut k = (ph / win * PI).sin();
    if torment {
        // Shrieking fit to burst, the jaw juddering.
        k = (k * 1.3 * (0.8 + 0.2 * (wc.t * 11.0 + seed * 40.0).sin())).min(1.0);
    }
    if k < 0.1 {
        Brick::Still
    } else {
        Brick::Scream(k)
    }
}

/// The Sanctum's walls: bricks of carved bone faces, some of them alive.
fn sanctum_tex(u: f32, v: f32, mx: i32, my: i32, wc: &WallCtx) -> Texel {
    let row = (v * 4.0).floor() as i32;
    let fv = (v * 4.0).fract();
    let odd = row % 2 != 0;
    let bu = u * 2.0 + if odd { 0.5 } else { 0.0 };
    let bi = bu.floor() as i32;
    let fu = bu.fract();
    if fv < 0.08 || fu < 0.05 {
        return (0.3, Some('.'), Some(Rgb(120, 40, 40)));
    }
    let grain = 0.9 + 0.2 * hashf(mx, my + row);
    // Only whole bricks come alive; half bricks straddle two wall tiles.
    let whole = !odd || bi == 1;
    let state = if whole && wc.dread > 0 {
        brick_state(mx, my, row, bi, wc)
    } else {
        Brick::Still
    };
    let eye_l = (0.22f32..0.4).contains(&fu);
    let eye_r = (0.6f32..0.78).contains(&fu);
    let in_eyes = (0.3f32..0.55).contains(&fv);
    match state {
        Brick::Still => {
            if (eye_l || eye_r) && in_eyes {
                return (0.15, Some('o'), None);
            }
            if (0.3f32..0.7).contains(&fu) && fv > 0.75 {
                return (0.6, Some('"'), None);
            }
            (grain, None, None)
        }
        Brick::Dead => {
            let rot = Rgb(150, 155, 128);
            let blood = Rgb(140, 15, 15);
            // Each dead face bleeds a little differently.
            let seed = hashf(mx * 4 + bi, my * 4 + row);
            // Eyes closed for good.
            if (eye_l || eye_r) && (0.4f32..0.47).contains(&fv) {
                return (0.35, Some('-'), Some(Rgb(60, 50, 45)));
            }
            // Blood weeping from beneath the lids.
            let tear_l = 0.28 + 0.04 * seed;
            let tear_r = 0.68 - 0.04 * seed;
            if (fu - tear_l).abs() < 0.02 && fv >= 0.47 && fv < 0.62 + 0.12 * seed {
                return (0.55, Some('|'), Some(blood));
            }
            if (fu - tear_r).abs() < 0.02 && fv >= 0.47 && fv < 0.7 - 0.1 * seed {
                return (0.55, Some('|'), Some(blood));
            }
            // Lips pressed shut...
            if (0.38f32..0.62).contains(&fu) && (0.72f32..0.78).contains(&fv) {
                return (0.3, Some('-'), Some(Rgb(70, 35, 30)));
            }
            // ...with blood running from the mouth.
            let drip = 0.44 + 0.12 * seed;
            if (fu - drip).abs() < 0.02 && fv >= 0.78 {
                return (0.55, Some('|'), Some(blood));
            }
            (grain * 0.7, None, Some(rot))
        }
        Brick::Scream(k) => {
            let torment = wc.dread == 2;
            let skin = Rgb(205, 195, 165).lerp(Rgb(228, 168, 148), 0.55 * k);
            // The whole face trembles inside its brick.
            let fu = fu + 0.02 * k * (wc.t * 43.0 + (bi + row) as f32).sin();
            let eye_l = (0.22f32..0.4).contains(&fu);
            let eye_r = (0.6f32..0.78).contains(&fu);
            // Brows knotted in anguish.
            if k > 0.3 && (0.16f32..0.27).contains(&fv) {
                if (0.18f32..0.42).contains(&fu) {
                    return (0.4, Some('/'), Some(skin));
                }
                if (0.58f32..0.82).contains(&fu) {
                    return (0.4, Some('\\'), Some(skin));
                }
            }
            if (eye_l || eye_r) && in_eyes {
                if torment {
                    // Rolled back, white and bloodshot.
                    return (1.1, Some('O'), Some(Rgb(235, 215, 200)));
                }
                // Squeezed shut.
                if (0.38f32..0.48).contains(&fv) {
                    return (
                        0.2,
                        Some(if eye_l { '>' } else { '<' }),
                        Some(Rgb(60, 30, 25)),
                    );
                }
                return (grain * 0.8, None, Some(skin));
            }
            // Tears of blood.
            if torment
                && ((0.29f32..0.33).contains(&fu) || (0.67f32..0.71).contains(&fu))
                && fv >= 0.55
                && fv < 0.55 + 0.4 * k
            {
                return (0.6, Some('|'), Some(Rgb(160, 20, 20)));
            }
            // The silent scream, stretching wide.
            let hw = 0.08 + if torment { 0.16 } else { 0.12 } * k;
            let hh = 0.06 + 0.17 * k;
            let (mu, mv) = ((fu - 0.5) / hw, (fv - 0.74) / hh);
            let md = mu * mu + mv * mv;
            if md < 0.6 {
                return (0.0, Some(' '), None);
            }
            if md < 1.0 {
                return (
                    0.4,
                    Some(if mu < 0.0 { '(' } else { ')' }),
                    Some(Rgb(130, 40, 40)),
                );
            }
            (grain, None, Some(skin))
        }
    }
}

/// Flesh, veins and eyes swallowing the stonework as you near the surface.
fn corrupt_tex(u: f32, v: f32, mx: i32, my: i32, wc: &WallCtx) -> Option<Texel> {
    let c = wc.corrupt;
    if c <= 0.0 {
        return None;
    }
    let n = vnoise(u * 2.5 + mx as f32 * 2.5, v * 2.5 + my as f32 * 1.9);
    let cover = c * 1.3 - hashf(mx * 7 + 3, my * 11 + 5) * 0.6;
    if n > cover {
        return None;
    }
    let hb = heartbeat(wc.t + (mx + my) as f32 * 0.03);
    // A ragged margin where the flesh eats into the stone.
    if n > cover - 0.05 {
        return Some((0.6, Some('%'), Some(Rgb(95, 35, 35))));
    }
    // A lidless eye, watching.
    let (eu, ev) = ((u - 0.5) / 0.14, (v - 0.45) / 0.09);
    if eu * eu + ev * ev < 1.0 && wall_eye(mx, my, c) {
        {
            if wc.stare == Some((mx, my)) {
                // Glaring red, pupil blown wide, fixed on you.
                if eu * eu + ev * ev < 0.25 {
                    return Some((0.1, Some('@'), Some(Rgb(30, 0, 0))));
                }
                return Some((1.5, Some('O'), Some(Rgb(255, 60, 40))));
            }
            let seed = hashf(mx, my * 3);
            if (wc.t * 0.35 + seed * 5.0).fract() < 0.06 {
                return Some((0.4, Some('-'), Some(Rgb(120, 50, 50))));
            }
            let look = (wc.t * 0.8 + seed * 12.0).sin() * 0.45;
            if (eu - look).abs() < 0.16 {
                return Some((0.2, Some('|'), Some(Rgb(40, 0, 0))));
            }
            return Some((1.4, Some('O'), Some(Rgb(230, 205, 110))));
        }
    }
    let vn = vnoise(u * 7.0 + mx as f32 * 7.0, v * 9.0 + my as f32 * 3.0);
    if (vn - 0.5).abs() < 0.035 {
        return Some((
            0.9 + 0.6 * hb,
            Some(if hash2(mx, my) % 2 == 0 { '~' } else { '-' }),
            Some(Rgb(220, 40, 50)),
        ));
    }
    let n2 = vnoise(u * 8.0 + mx as f32 * 5.0, v * 8.0);
    Some((0.55 + 0.4 * n2 + 0.2 * hb, None, Some(Rgb(150, 58, 62))))
}

/// Plain wooden planks with iron bands and a brass ring.
fn plank_door(u: f32, v: f32, mx: i32, wood: Rgb) -> Texel {
    let fu = (u * 5.0).fract();
    if (0.14f32..0.22).contains(&v) || (0.78f32..0.86).contains(&v) {
        return (0.9, Some('='), Some(Rgb(140, 140, 150)));
    }
    if (0.72f32..0.82).contains(&u) && (0.47f32..0.56).contains(&v) {
        return (1.4, Some('o'), Some(Rgb(230, 190, 80)));
    }
    if fu < 0.12 {
        return (0.4, Some('|'), Some(wood.scale(0.6)));
    }
    let grain = 0.85 + 0.25 * vnoise(u * 5.0 + mx as f32 * 3.0, v * 30.0);
    (grain, None, Some(wood))
}

/// A closed door in the look of its floor's theme.
fn door_tex(style: u8, u: f32, v: f32, mx: i32, my: i32, wc: &WallCtx) -> Texel {
    let t = wc.t;
    // Every door sits in a dark frame.
    if u < 0.05 || u > 0.95 || v < 0.03 {
        return (0.3, Some(if v < 0.03 { '_' } else { '|' }), None);
    }
    match style {
        1 => {
            // Mossy: old planks swallowed by moss and creepers.
            let n = vnoise(u * 6.0 + mx as f32 * 3.0, v * 6.0 + my as f32);
            if n > 0.62 {
                return (
                    0.8 + 0.3 * n,
                    Some(if n > 0.75 { '%' } else { '"' }),
                    Some(Rgb(80, 150, 60)),
                );
            }
            plank_door(u, v, mx, Rgb(120, 95, 60))
        }
        2 => {
            // Obsidian: a black slab split by a seam of fire, with burning runes.
            if (u - 0.5).abs() < 0.025 {
                return (
                    1.2 + 0.3 * (t * 2.0).sin(),
                    Some('|'),
                    Some(Rgb(255, 110, 40)),
                );
            }
            let (ru, rv) = ((u * 3.0).fract(), (v * 4.0).fract());
            if (ru - 0.5).abs() < 0.1
                && (rv - 0.5).abs() < 0.12
                && hashf((u * 3.0) as i32 + mx * 5, (v * 4.0) as i32 + my) < 0.5
            {
                return (
                    1.35,
                    Some(['&', '$', 'X'][(hash2(mx, (v * 4.0) as i32) % 3) as usize]),
                    Some(Rgb(255, 120, 50)),
                );
            }
            (
                0.35 + 0.2 * vnoise(u * 8.0, v * 8.0 + mx as f32),
                None,
                Some(Rgb(50, 40, 60)),
            )
        }
        3 => {
            // Sanctum: a door of lashed ribs with a skull at its heart.
            let (su, sv) = ((u - 0.5) / 0.14, (v - 0.47) / 0.11);
            if su * su + sv * sv < 1.0 {
                if ((u - 0.44).abs() < 0.03 || (u - 0.56).abs() < 0.03) && (v - 0.45).abs() < 0.03 {
                    return (0.05, Some('o'), None);
                }
                return (1.0, None, Some(Rgb(225, 215, 190)));
            }
            if (v * 8.0).fract() < 0.55 {
                return (0.9, Some('='), Some(Rgb(215, 205, 175)));
            }
            (0.1, Some(' '), None)
        }
        4 => {
            // Frozen: planks under a crust of ice, icicles hanging from the lintel.
            if v < 0.12 && ((u * 9.0).fract() - 0.5).abs() < 0.5 - v * 4.0 {
                return (1.1, Some('V'), Some(Rgb(210, 235, 255)));
            }
            if hashf((u * 20.0) as i32 + mx * 40, (v * 16.0) as i32 + my * 7) > 0.92 {
                return (1.3, Some('*'), Some(Rgb(230, 245, 255)));
            }
            let (m, f, c) = plank_door(u, v, mx, Rgb(110, 120, 140));
            (m, f, c.map(|c| c.lerp(Rgb(170, 210, 240), 0.4)))
        }
        5 => {
            // Crystal: a faceted slab of glowing quartz.
            let a = ((u * 4.0 + v * 4.0).fract() - 0.5).abs();
            let b = ((u * 4.0 - v * 4.0).rem_euclid(1.0) - 0.5).abs();
            let glint = 1.0 + 0.4 * (t * 1.5 + u * 6.0).sin();
            if a < 0.05 {
                return (glint, Some('/'), Some(Rgb(110, 245, 230)));
            }
            if b < 0.05 {
                return (glint, Some('\\'), Some(Rgb(220, 130, 255)));
            }
            (0.8, None, Some(Rgb(120, 110, 170)))
        }
        9 => {
            // Fungal: a door of woven roots with a glowing cap for a handle.
            if (u - 0.78).abs() < 0.06 && (v - 0.5).abs() < 0.05 {
                return (
                    1.3 + 0.3 * (t * 2.0).sin(),
                    Some('o'),
                    Some(Rgb(120, 255, 200)),
                );
            }
            let w = (v * 10.0 + (u * 9.0).sin() * 0.8).fract();
            if w < 0.25 {
                return (0.3, Some('~'), Some(Rgb(70, 50, 35)));
            }
            (
                0.7 + 0.3 * vnoise(u * 10.0, v * 3.0 + mx as f32),
                None,
                Some(Rgb(125, 95, 70)),
            )
        }
        10 => {
            // Drowned temple: a green bronze door with a porthole full of black water.
            let (pu, pv) = ((u - 0.5) / 0.18, (v - 0.35) / 0.12);
            let pr = pu * pu + pv * pv;
            if pr < 1.0 {
                if pr > 0.7 {
                    return (0.9, Some('o'), Some(Rgb(150, 170, 140)));
                }
                let ripple = ((v * 30.0 + t * 2.0).sin() * 0.5 + 0.5) * 0.2;
                return (0.2 + ripple, Some('~'), Some(Rgb(40, 90, 110)));
            }
            if ((u * 4.0).fract() - 0.5).abs() < 0.07 && ((v * 6.0).fract() - 0.5).abs() < 0.08 {
                return (0.9, Some('o'), Some(Rgb(150, 160, 130)));
            }
            if hashf((u * 25.0) as i32 + mx * 30, (v * 20.0) as i32 + my) < 0.06 {
                return (0.6, Some('.'), Some(Rgb(170, 175, 150)));
            }
            (
                0.75 + 0.2 * vnoise(u * 5.0, v * 5.0 + mx as f32),
                None,
                Some(Rgb(80, 140, 120)),
            )
        }
        11 => {
            // Mines: a rough timber door with an X brace and iron bands.
            if (u - v).abs() < 0.06 || (u - (1.0 - v)).abs() < 0.06 {
                return (
                    0.75,
                    Some(if (u - v).abs() < 0.06 { '\\' } else { '/' }),
                    Some(Rgb(110, 75, 45)),
                );
            }
            plank_door(u, v, mx, Rgb(145, 105, 65))
        }
        12 => {
            // Ossuary: thigh bones lashed side by side.
            let fu = (u * 6.0).fract();
            if v < 0.08 || v > 0.92 {
                return if (0.15f32..0.85).contains(&fu) {
                    (0.9, Some('o'), Some(Rgb(220, 210, 180)))
                } else {
                    (0.1, Some(' '), None)
                };
            }
            if (0.25f32..0.75).contains(&fu) {
                if (v * 5.0).fract() < 0.1 {
                    return (0.5, Some('='), Some(Rgb(120, 90, 60)));
                }
                return (0.95, Some('|'), Some(Rgb(220, 210, 180)));
            }
            (0.08, Some(' '), None)
        }
        13 => {
            // The abyss: no door at all, only a slowly turning spiral of nothing.
            let (du, dv) = (u - 0.5, (v - 0.5) * 1.4);
            let r = (du * du + dv * dv).sqrt();
            let a = dv.atan2(du);
            let k = ((a / std::f32::consts::TAU + r * 3.0 - t * 0.25).fract() * 4.0) as i32;
            if r > 0.48 {
                return (0.3, Some('.'), Some(Rgb(90, 60, 130)));
            }
            let glow = (1.0 - r * 1.8).max(0.0);
            (
                0.2 + 1.1 * glow * if k % 2 == 0 { 1.0 } else { 0.4 },
                Some(['@', '*', 'o', '.'][k.rem_euclid(4) as usize]),
                Some(Rgb(170, 110, 255)),
            )
        }
        14 => {
            // Blood temple: crimson lacquer studded with gold.
            if u < 0.1 || u > 0.9 || v < 0.08 || v > 0.92 {
                return (
                    1.05,
                    Some(if u < 0.1 || u > 0.9 { '|' } else { '=' }),
                    Some(Rgb(220, 170, 70)),
                );
            }
            if ((u * 4.0).fract() - 0.5).abs() < 0.08 && ((v * 5.0).fract() - 0.5).abs() < 0.08 {
                return (1.3, Some('o'), Some(Rgb(255, 210, 90)));
            }
            (
                0.75 + 0.15 * vnoise(u * 3.0, v * 9.0),
                None,
                Some(Rgb(150, 25, 35)),
            )
        }
        _ => plank_door(u, v, mx, Rgb(150, 100, 55)),
    }
}

/// The bars of a gate. Returns a texel where there is iron, and None where
/// you can see through to whatever lies beyond.
fn gate_tex(style: u8, u: f32, v: f32, t: f32) -> Option<Texel> {
    if u < 0.04 || u > 0.96 || v < 0.03 {
        return Some((
            0.35,
            Some(if v < 0.03 { '_' } else { '|' }),
            Some(Rgb(70, 70, 75)),
        ));
    }
    match style {
        6 => {
            // Crypt: a wrought iron gate with spear-tipped bars and a scrolled band.
            let bar = ((u * 6.0).fract() - 0.5).abs() < 0.15;
            if bar && v < 0.12 {
                return Some((0.8, Some('^'), Some(Rgb(80, 80, 90))));
            }
            if bar {
                return Some((0.7, Some('|'), Some(Rgb(70, 70, 80))));
            }
            if (0.45f32..0.56).contains(&v) {
                return Some((
                    0.75,
                    Some(if (u * 12.0).fract() < 0.5 { 'S' } else { '=' }),
                    Some(Rgb(80, 80, 90)),
                ));
            }
            None
        }
        7 => {
            // Gaol: thick cell bars with a heavy lock plate.
            if (0.42f32..0.58).contains(&u) && (0.44f32..0.58).contains(&v) {
                let hole = (u - 0.5).abs() < 0.02 && (v - 0.51).abs() < 0.03;
                return Some(if hole {
                    (0.05, Some('o'), None)
                } else {
                    (0.85, Some('#'), Some(Rgb(110, 105, 100)))
                });
            }
            if ((u * 7.0).fract() - 0.5).abs() < 0.13 {
                return Some((0.75, Some('|'), Some(Rgb(95, 95, 100))));
            }
            if (0.18f32..0.24).contains(&v) || (0.78f32..0.84).contains(&v) {
                return Some((0.75, Some('='), Some(Rgb(95, 95, 100))));
            }
            None
        }
        8 => {
            // Sewer: a rusted grate dripping with slime.
            let bar = ((u * 5.0).fract() - 0.5).abs() < 0.15;
            if bar {
                if v > 0.8 && ((u * 37.0 + v * 11.0) as i32) % 3 == 0 {
                    return Some((0.8, Some('~'), Some(Rgb(110, 170, 70))));
                }
                return Some((0.7, Some('|'), Some(Rgb(130, 85, 50))));
            }
            if (v - 0.1).abs() < 0.05 || (v - 0.5).abs() < 0.05 || (v - 0.9).abs() < 0.05 {
                return Some((0.7, Some('='), Some(Rgb(130, 85, 50))));
            }
            None
        }
        15 => {
            // Foundry: a portcullis of iron, red-hot along its spikes.
            if v > 0.9 && ((u * 5.0).fract() - 0.5).abs() < 0.12 {
                return Some((
                    1.2 + 0.3 * (t * 3.0).sin(),
                    Some('v'),
                    Some(Rgb(255, 120, 40)),
                ));
            }
            if ((u * 5.0).fract() - 0.5).abs() < 0.14 {
                return Some((0.75, Some('|'), Some(Rgb(90, 85, 85))));
            }
            if ((v * 5.0).fract() - 0.5).abs() < 0.1 {
                return Some((0.75, Some('='), Some(Rgb(90, 85, 85))));
            }
            None
        }
        _ => {
            if ((u * 6.0).fract() - 0.5).abs() < 0.1 {
                return Some((0.7, Some('|'), Some(Rgb(90, 90, 95))));
            }
            None
        }
    }
}

/// Where a ray passed a gate, or a door still opening, on its way to a wall.
#[derive(Clone, Copy)]
struct GateHit {
    perp: f32,
    u: f32,
    light: [f32; 3],
    side_f: f32,
    mx: i32,
    my: i32,
    /// A door caught mid-opening, and how far open it is.
    anim: Option<(crate::fx::DoorOpening, f32)>,
}

/// What a see-through surface shows at height `v`: gate bars, or a door
/// partway through opening. None where you can see past it.
fn overlay_tex(style: u8, gh: &GateHit, v: f32, wc: &WallCtx) -> Option<Texel> {
    use crate::fx::DoorOpening::*;
    let u = gh.u;
    match gh.anim {
        None => gate_tex(style, u, v, wc.t),
        // Bars slide up into the lintel.
        Some((Rise, p)) => {
            let v2 = v + p;
            if v2 >= 1.0 {
                None
            } else {
                gate_tex(style, u, v2, wc.t)
            }
        }
        // The door turns on its hinge: it narrows toward the hinge side and
        // darkens as it turns away from you.
        Some((Swing, p)) => {
            let w = 1.0 - p * 0.92;
            if u > w {
                None
            } else {
                let (m, f, c) = door_tex(style, (u / w).min(0.999), v, gh.mx, gh.my, wc);
                Some((m * (1.0 - 0.55 * p), f, c))
            }
        }
        // Two leaves slide apart from the middle.
        Some((Split, p)) => {
            let half = (1.0 - p) * 0.5;
            if u < half {
                Some(door_tex(style, u + p * 0.5, v, gh.mx, gh.my, wc))
            } else if u > 1.0 - half {
                Some(door_tex(style, u - p * 0.5, v, gh.mx, gh.my, wc))
            } else {
                None
            }
        }
        // It comes apart speck by speck, glowing as it goes.
        Some((Dissolve, p)) => {
            let h = hashf(
                (u * 28.0) as i32 + gh.mx * 97,
                (v * 18.0) as i32 + gh.my * 53,
            );
            if h < p {
                None
            } else if h < p + 0.08 {
                Some((
                    1.5,
                    Some(if h < p + 0.04 { '*' } else { '.' }),
                    Some(Rgb(235, 225, 255)),
                ))
            } else {
                let (m, f, c) = door_tex(style, u, v, gh.mx, gh.my, wc);
                Some((m * (1.0 + 0.6 * p), f, c))
            }
        }
    }
}

/// Wall texture: (brightness multiplier, forced character, colour override).
fn wall_tex(style: u8, tile: Tile, u: f32, v: f32, mx: i32, my: i32, wc: &WallCtx) -> Texel {
    if tile == Tile::Crystal {
        // The Guardian's walls: glowing facets of crystal.
        let a = ((u * 3.0 + v * 3.0).fract() - 0.5).abs();
        let b = ((u * 3.0 - v * 3.0).rem_euclid(1.0) - 0.5).abs();
        let glint = 1.0 + 0.35 * (wc.t * 2.0 + u * 5.0 + mx as f32).sin();
        if a < 0.05 || b < 0.05 {
            return (
                glint + 0.2,
                Some(if a < 0.05 { '/' } else { '\\' }),
                Some(prismatic(wc.t, (mx + my) as f32)),
            );
        }
        return (
            0.7 + 0.2 * vnoise(u * 6.0, v * 6.0 + my as f32),
            None,
            Some(Rgb(120, 200, 235)),
        );
    }
    if tile == Tile::Sinew {
        // Taut red cords of living muscle, throbbing with the dungeon's pulse.
        let hb = heartbeat(wc.t + (mx + my) as f32 * 0.05);
        let k = (u * 7.0 + (v * 3.0 + mx as f32).sin() * 0.35).fract();
        if k < 0.1 {
            return (0.2, Some('('), Some(Rgb(70, 10, 15)));
        }
        if k > 0.9 {
            return (0.2, Some(')'), Some(Rgb(70, 10, 15)));
        }
        let vn = vnoise(u * 9.0 + mx as f32 * 3.0, v * 2.0 + my as f32);
        if (vn - 0.5).abs() < 0.04 {
            return (1.0 + 0.5 * hb, Some('~'), Some(Rgb(235, 60, 70)));
        }
        return (0.55 + 0.35 * vn + 0.3 * hb, None, Some(Rgb(175, 45, 55)));
    }
    if tile == Tile::DoorClosed {
        return door_tex(style, u, v, mx, my, wc);
    }
    if let Some(f) = corrupt_tex(u, v, mx, my, wc) {
        return f;
    }
    match style {
        0 => {
            let r = (v * 5.0).floor();
            let fv = (v * 5.0).fract();
            let off = if (r as i32) % 2 == 0 { 0.0 } else { 0.5 };
            let bu = u * 2.0 + off;
            let fu = bu.fract();
            let bi = bu.floor() as i32;
            if fv < 0.14 {
                return (0.35, Some('_'), None);
            }
            if fu < 0.06 {
                return (0.35, Some('|'), None);
            }
            let var = 0.8 + 0.4 * hashf(mx * 7 + bi, my * 13 + r as i32);
            (var, None, None)
        }
        1 => {
            let n = vnoise(u * 4.0 + mx as f32 * 4.0 + my as f32 * 1.7, v * 6.0);
            let n2 = vnoise(u * 9.0 + mx as f32 * 9.0, v * 11.0 + my as f32 * 3.0);
            if n2 > 0.72 {
                return (0.9 + n * 0.3, Some('%'), Some(Rgb(70, 150, 60)));
            }
            (0.6 + 0.65 * n, None, None)
        }
        2 => {
            let r = (v * 2.0).floor();
            let fv = (v * 2.0).fract();
            let off = if (r as i32) % 2 == 0 { 0.0 } else { 0.5 };
            let bu = u + off;
            let fu = bu.fract();
            let bi = bu.floor() as i32;
            if fv < 0.05 || fu < 0.035 {
                return (0.25, Some('.'), None);
            }
            let h = hashf(mx * 31 + bi, my * 17 + r as i32);
            if h < 0.12 && (0.35f32..0.65).contains(&fu) && (0.3f32..0.7).contains(&fv) {
                let rune = ['&', '$', '%', '#', 'X'][(hash2(mx, my) % 5) as usize];
                return (1.6, Some(rune), Some(Rgb(255, 110, 50)));
            }
            (0.75 + 0.35 * h, None, None)
        }
        4 => {
            // Frozen: cracked blue ice, icicles, twinkling frost.
            let n = vnoise(u * 3.0 + mx as f32 * 3.0, v * 3.0 + my as f32 * 2.3);
            let crack =
                (vnoise(u * 6.0 + mx as f32 * 6.0 + 40.0, v * 6.0 + my as f32 * 5.0) - 0.5).abs();
            if crack < 0.025 {
                return (
                    1.15,
                    Some(if hash2(mx, my) % 2 == 0 { '/' } else { '\\' }),
                    Some(Rgb(230, 245, 255)),
                );
            }
            let ic = (u * 9.0).fract();
            let len = 0.06 + 0.14 * hashf((u * 9.0) as i32 + mx * 9, my * 3);
            if v < len * (1.0 - (ic - 0.5).abs() * 2.0) {
                return (1.1, Some('V'), Some(Rgb(200, 230, 255)));
            }
            let spark = hashf(
                (u * 14.0) as i32 + mx * 31 + (wc.t * 1.5) as i32 * 7,
                (v * 20.0) as i32 + my * 17,
            );
            if spark > 0.988 {
                return (1.4, Some('*'), Some(Rgb(225, 240, 255)));
            }
            (0.7 + 0.45 * n, None, None)
        }
        5 => {
            // Crystal: dark rock with glowing crystal clusters.
            let n = vnoise(u * 3.0 + mx as f32 * 3.0, v * 4.0 + my as f32 * 2.0);
            let gu = u * 3.0;
            let ci = gu.floor() as i32;
            let fu = gu.fract();
            let h = hashf(mx * 23 + ci, my * 29);
            if h < 0.45 {
                let height = 0.25 + 0.4 * hashf(mx * 5 + ci, my * 3 + 1);
                let tip = 1.0 - height;
                if v > tip {
                    let half = (v - tip) / height * 0.45;
                    let du = (fu - 0.5).abs();
                    if du < half {
                        let hue = if hashf(mx + ci, my * 7) < 0.5 {
                            Rgb(90, 240, 220)
                        } else {
                            Rgb(220, 110, 255)
                        };
                        let glint = 0.5 + 0.5 * (wc.t * 1.7 + h * 30.0).sin();
                        let ch = if du > half - 0.07 {
                            if fu < 0.5 {
                                '/'
                            } else {
                                '\\'
                            }
                        } else if du < 0.04 {
                            '|'
                        } else {
                            ':'
                        };
                        return (0.95 + 0.45 * glint, Some(ch), Some(hue));
                    }
                }
            }
            (0.55 + 0.5 * n, None, None)
        }
        6 => {
            // Crypt: great stone blocks with burial niches.
            let row = (v * 2.0).floor() as i32;
            let fv = (v * 2.0).fract();
            let bu = u + if row % 2 == 0 { 0.0 } else { 0.5 };
            let fu = bu.fract();
            let bi = bu.floor() as i32;
            if fv < 0.05 || fu < 0.03 {
                return (0.3, Some('.'), None);
            }
            let h = hashf(mx * 19 + bi, my * 23 + row);
            if h < 0.45 && (0.2f32..0.8).contains(&fu) && (0.25f32..0.8).contains(&fv) {
                if (fu - 0.2).abs() < 0.04 || (fu - 0.8).abs() < 0.04 || (fv - 0.25).abs() < 0.05 {
                    return (
                        0.5,
                        Some(if (fv - 0.25).abs() < 0.05 { '_' } else { '|' }),
                        None,
                    );
                }
                if h < 0.2 && (0.4f32..0.6).contains(&fu) && (0.5f32..0.7).contains(&fv) {
                    return (
                        0.9,
                        Some(if (0.45f32..0.55).contains(&fu) {
                            '"'
                        } else {
                            'o'
                        }),
                        Some(Rgb(210, 205, 185)),
                    );
                }
                return (0.08, None, None);
            }
            (
                0.8 + 0.3 * vnoise(u * 6.0 + mx as f32, v * 6.0 + my as f32),
                None,
                None,
            )
        }
        7 => {
            // Dank dungeon: rough wet stone, dripping water, iron rings.
            let row = (v * 3.0).floor() as i32;
            let fv = (v * 3.0).fract();
            let bu = u * 1.5 + if row % 2 == 0 { 0.0 } else { 0.4 };
            let fu = bu.fract();
            if fv < 0.07 || fu < 0.04 {
                return (0.3, Some('.'), None);
            }
            let col = (u * 14.0) as i32;
            let seed = hashf(col + mx * 31, my * 7);
            if seed < 0.12 {
                let drop = (wc.t * 0.35 + seed * 9.0).fract();
                if (v - drop).abs() < 0.03 {
                    return (1.2, Some('.'), Some(Rgb(170, 220, 230)));
                }
                if v < 0.2 + seed * 4.0 {
                    return (0.55, Some('|'), Some(Rgb(70, 95, 95)));
                }
            }
            if hashf(mx * 5, my * 9 + 1) < 0.25
                && ((u - 0.5).abs() < 0.06 && (v - 0.42).abs() < 0.07)
            {
                return (0.9, Some('&'), Some(Rgb(140, 135, 130)));
            }
            (
                0.65 + 0.4 * vnoise(u * 7.0 + mx as f32 * 2.0, v * 9.0 + my as f32),
                None,
                None,
            )
        }
        8 => {
            // Sewer: small red bricks, a band of slime, drain grates.
            if v > 0.8 + 0.02 * (u * 18.0 + wc.t * 1.5).sin() {
                let k = ((u * 20.0 + wc.t * 2.0) as i32 + mx) % 3;
                return (
                    0.75,
                    Some(['~', '-', '~'][k.rem_euclid(3) as usize]),
                    Some(Rgb(110, 170, 70)),
                );
            }
            if hashf(mx * 3 + 7, my * 5) < 0.3
                && (0.35f32..0.65).contains(&u)
                && (0.55f32..0.8).contains(&v)
            {
                return if ((u - 0.35) * 30.0).fract() < 0.3 {
                    (0.8, Some('|'), Some(Rgb(90, 90, 95)))
                } else {
                    (0.05, None, None)
                };
            }
            let row = (v * 6.0).floor() as i32;
            let fv = (v * 6.0).fract();
            let fu = (u * 3.0 + if row % 2 == 0 { 0.0 } else { 0.5 }).fract();
            if fv < 0.12 || fu < 0.05 {
                return (0.35, Some('_'), None);
            }
            (
                0.8 + 0.3 * hashf(mx * 3 + (u * 3.0) as i32, my + row),
                None,
                None,
            )
        }
        9 => {
            // Fungal grotto: dark rock and clusters of glowing mushrooms.
            let gu = u * 4.0;
            let ci = gu.floor() as i32;
            let fu = gu.fract();
            let h = hashf(mx * 11 + ci, my * 13);
            if h < 0.5 {
                let height = 0.2 + 0.35 * hashf(mx + ci * 7, my * 3 + 2);
                let top = 1.0 - height;
                let glow = 1.0 + 0.4 * (wc.t * 1.3 + h * 20.0).sin();
                let hue = if h < 0.25 {
                    Rgb(110, 255, 210)
                } else {
                    Rgb(220, 120, 255)
                };
                if (fu - 0.5).abs() < 0.07 && v > top {
                    return (0.8, Some('|'), Some(Rgb(200, 190, 170)));
                }
                let (cu, cv) = ((fu - 0.5) / 0.4, (v - top) / 0.08);
                if cu * cu + cv * cv < 1.0 && v <= top + 0.02 {
                    return (
                        glow,
                        Some(if cu < -0.4 {
                            '('
                        } else if cu > 0.4 {
                            ')'
                        } else {
                            '^'
                        }),
                        Some(hue),
                    );
                }
            }
            (
                0.5 + 0.45 * vnoise(u * 4.0 + mx as f32 * 3.0, v * 4.0 + my as f32 * 2.0),
                None,
                None,
            )
        }
        10 => {
            // Drowned temple: fluted columns standing in black water.
            let wl = 0.72 + 0.02 * (u * 20.0 + wc.t * 2.0 + mx as f32).sin();
            if (v - wl).abs() < 0.02 {
                return (1.05, Some('~'), Some(Rgb(150, 230, 230)));
            }
            if v > wl {
                if hashf((u * 30.0) as i32 + mx * 40, (v * 30.0) as i32 + my * 9) < 0.1 {
                    return (0.6, Some('.'), Some(Rgb(150, 160, 140)));
                }
                return (
                    0.35 + 0.15 * vnoise(u * 8.0 + wc.t * 0.3, v * 8.0),
                    None,
                    Some(Rgb(50, 90, 100)),
                );
            }
            if v < 0.1 {
                return (0.9, Some('='), None);
            }
            let fu = (u * 4.0).fract();
            if fu < 0.12 {
                return (0.35, Some('|'), None);
            }
            (0.75 + 0.3 * (fu * std::f32::consts::PI).sin(), None, None)
        }
        11 => {
            // Old mines: raw rock held up by timber, glints of ore.
            if u < 0.1 || u > 0.9 {
                let grain = 0.6 + 0.3 * vnoise(u * 40.0, v * 3.0 + mx as f32);
                return (grain, Some('|'), Some(Rgb(140, 95, 55)));
            }
            if v < 0.12 {
                return (0.7, Some('='), Some(Rgb(150, 100, 60)));
            }
            let h = hashf((u * 22.0) as i32 + mx * 50, (v * 14.0) as i32 + my * 50);
            if h > 0.975 {
                let tw = 0.9 + 0.5 * (wc.t * 3.0 + h * 50.0).sin();
                return (tw, Some('*'), Some(Rgb(255, 210, 90)));
            }
            (
                0.5 + 0.55 * vnoise(u * 5.0 + mx as f32 * 3.0, v * 5.0 + my as f32 * 2.0),
                None,
                None,
            )
        }
        12 => {
            // Ossuary: rows of skulls between layers of stacked thigh bones.
            let row = (v * 6.0).floor() as i32;
            let fv = (v * 6.0).fract();
            if row % 2 == 0 {
                let fu = (u * 5.0 + if row % 4 == 0 { 0.0 } else { 0.5 }).fract();
                if ((fu - 0.32).abs() < 0.1 || (fu - 0.68).abs() < 0.1)
                    && (0.3f32..0.6).contains(&fv)
                {
                    return (0.1, Some('o'), None);
                }
                if fu < 0.08 || fu > 0.92 {
                    return (0.2, Some(' '), None);
                }
                if fv > 0.75 && (0.35f32..0.65).contains(&fu) {
                    return (0.5, Some('"'), None);
                }
                return (0.95 + 0.15 * hashf(mx, my + row), None, None);
            }
            let fu = (u * 3.0 + row as f32 * 0.37).fract();
            if fu < 0.08 || fu > 0.92 {
                return (0.85, Some('o'), None);
            }
            if fv < 0.15 || fv > 0.85 {
                return (0.25, Some('.'), None);
            }
            (0.8, Some('='), None)
        }
        13 => {
            // The abyss: black nothing, drifting stars and a violet haze.
            let su = u * 18.0 + wc.t * 0.15 + mx as f32 * 18.0;
            let sv = v * 12.0 + my as f32 * 12.0;
            let h = hashf(su.floor() as i32, sv.floor() as i32);
            if h > 0.965 {
                let tw = 0.5 + 0.5 * (wc.t * 2.0 + h * 90.0).sin();
                return (
                    1.0 + 0.5 * tw,
                    Some(if h > 0.99 { '*' } else { '.' }),
                    Some(Rgb(220, 210, 255)),
                );
            }
            let n = vnoise(
                u * 2.0 + mx as f32 * 2.0 + wc.t * 0.03,
                v * 2.0 + my as f32 * 2.0,
            );
            (
                0.06 + 0.3 * n * n,
                None,
                Some(Rgb(70, 40, 110).lerp(Rgb(20, 10, 40), 1.0 - n)),
            )
        }
        14 => {
            // Blood temple: veined red marble framed in gold, with burning runes.
            let fu = (u * 2.0).fract();
            let fv = (v * 2.0).fract();
            if fu < 0.05 || fu > 0.95 || fv < 0.06 || fv > 0.94 {
                return (
                    1.1,
                    Some(if fv < 0.06 || fv > 0.94 { '=' } else { '|' }),
                    Some(Rgb(220, 170, 70)),
                );
            }
            let pi = (u * 2.0) as i32 + mx * 3;
            let pj = (v * 2.0) as i32 + my * 3;
            if hashf(pi, pj) < 0.3 && (fu - 0.5).abs() < 0.08 && (fv - 0.5).abs() < 0.1 {
                let rune = ['$', '&', '%', '#', 'Y'][(hash2(pi, pj) % 5) as usize];
                return (1.35, Some(rune), Some(Rgb(255, 200, 80)));
            }
            let vn = vnoise(u * 5.0 + mx as f32 * 5.0, v * 3.0 + my as f32 * 2.0);
            if (vn - 0.5).abs() < 0.03 {
                return (0.4, Some('~'), Some(Rgb(80, 10, 15)));
            }
            (0.75 + 0.3 * vn, None, None)
        }
        15 => {
            // Infernal foundry: riveted iron plates with seams glowing like a forge.
            let fu = (u * 2.0).fract();
            let row = (v * 3.0).floor() as i32;
            let fv = (v * 3.0).fract();
            if fu < 0.04 || fv < 0.07 {
                let glow = 1.05 + 0.4 * (wc.t * 2.0 + (mx + row) as f32).sin();
                return (
                    glow,
                    Some(if fv < 0.07 { '-' } else { '|' }),
                    Some(Rgb(255, 120, 30)),
                );
            }
            if ((fu - 0.12).abs() < 0.04 || (fu - 0.88).abs() < 0.04)
                && ((fv - 0.2).abs() < 0.07 || (fv - 0.85).abs() < 0.07)
            {
                return (0.9, Some('o'), Some(Rgb(150, 145, 140)));
            }
            (
                0.55 + 0.3 * hashf(mx * 2 + (u * 2.0) as i32, my * 3 + row),
                None,
                None,
            )
        }
        _ => sanctum_tex(u, v, mx, my, wc),
    }
}

/// Where the first-person camera is, for drawing solid objects in the view.
struct ChestView {
    cam: (f32, f32),
    dir: (f32, f32),
    plane: (f32, f32),
    horizon: f32,
    proj: f32,
    vx: i32,
    vy: i32,
    vw: i32,
    vh: i32,
}

/// An axis-aligned box in the world: (min x, min y, min z), (max x, max y, max z).
type Aabb = ([f32; 3], [f32; 3]);

/// The nearest point where a ray from `o` along `d` enters box `b`:
/// (distance, axis of the face struck, whether it is the face on the far, max side).
fn ray_box(o: [f32; 3], d: [f32; 3], b: &Aabb) -> Option<(f32, usize, bool)> {
    let mut tn = f32::MIN;
    let mut tf = f32::MAX;
    let mut axis = 0;
    let mut maxside = false;
    for k in 0..3 {
        if d[k].abs() < 1e-6 {
            if o[k] < b.0[k] || o[k] > b.1[k] {
                return None;
            }
            continue;
        }
        let t1 = (b.0[k] - o[k]) / d[k];
        let t2 = (b.1[k] - o[k]) / d[k];
        let (lo, hi, far_side) = if t1 < t2 { (t1, t2, false) } else { (t2, t1, true) };
        if lo > tn {
            tn = lo;
            axis = k;
            maxside = far_side;
        }
        tf = tf.min(hi);
    }
    if tn > tf || tn < 0.05 {
        return None;
    }
    Some((tn, axis, maxside))
}

/// A treasure chest, drawn as a real box so it looks right from any side:
/// the brass lock on its front, iron bands over the lid, plain boards behind.
/// An opened chest shows its dark, empty inside, with the lid thrown back.
fn draw_chest(
    c: &mut Canvas,
    g: &Game,
    v: &ChestView,
    ch: &Chest,
    zbuf: &[f32],
    barred: &dyn Fn(i32, i32, f32) -> bool,
    t: f32,
) {
    const HW: f32 = 0.36; // half its width
    const HD: f32 = 0.24; // half its depth, front to back
    const BODY: f32 = 0.30; // where the lid meets the body
    const TOP: f32 = 0.44;
    let facing = g.level.chest_facing(ch.x, ch.y) as usize;
    let (fx, fy) = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)][facing];
    // Pushed back against the wall behind it.
    let cx = ch.x as f32 + 0.5 - fx * 0.1;
    let cy = ch.y as f32 + 0.5 - fy * 0.1;
    // Extents along the world axes for a footprint `d` deep and `w` wide,
    // offset `off` toward the front.
    let span = |d0: f32, d1: f32, z0: f32, z1: f32| -> Aabb {
        let (ax0, ax1) = if fx != 0.0 {
            let (a, b) = (cx + fx * d0, cx + fx * d1);
            (a.min(b), a.max(b))
        } else {
            (cx - HW, cx + HW)
        };
        let (ay0, ay1) = if fy != 0.0 {
            let (a, b) = (cy + fy * d0, cy + fy * d1);
            (a.min(b), a.max(b))
        } else {
            (cy - HW, cy + HW)
        };
        ([ax0, ay0, z0], [ax1, ay1, z1])
    };
    let boxes: Vec<Aabb> = if ch.open {
        vec![span(-HD, HD, 0.0, BODY), span(-HD - 0.02, -HD + 0.05, BODY, BODY + 0.4)]
    } else {
        vec![span(-HD, HD, 0.0, TOP)]
    };
    // Screen bounds from the corners of everything.
    let (dirx, diry) = v.dir;
    let (plx, ply) = v.plane;
    let inv = 1.0 / (plx * diry - dirx * ply);
    let (mut c0, mut c1, mut r0, mut r1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    let mut behind = false;
    let mut any_front = false;
    for b in &boxes {
        for k in 0..8 {
            let wx = if k & 1 == 0 { b.0[0] } else { b.1[0] };
            let wy = if k & 2 == 0 { b.0[1] } else { b.1[1] };
            let wz = if k & 4 == 0 { b.0[2] } else { b.1[2] };
            let (rx, ry) = (wx - v.cam.0, wy - v.cam.1);
            let tx = inv * (diry * rx - dirx * ry);
            let ty = inv * (-ply * rx + plx * ry);
            if ty <= 0.05 {
                behind = true;
                continue;
            }
            any_front = true;
            let col = v.vw as f32 / 2.0 * (1.0 + tx / ty);
            let row = v.horizon + v.proj / ty * (0.5 - wz);
            c0 = c0.min(col);
            c1 = c1.max(col);
            r0 = r0.min(row);
            r1 = r1.max(row);
        }
    }
    if !any_front {
        return;
    }
    let (c0, c1, r0, r1) = if behind {
        (0, v.vw, 0, v.vh)
    } else {
        (
            (c0.floor() as i32).max(0),
            (c1.ceil() as i32 + 1).min(v.vw),
            (r0.floor() as i32).max(0),
            (r1.ceil() as i32 + 1).min(v.vh),
        )
    };
    let l = light_at(g, ch.x as f32 + 0.5, ch.y as f32 + 0.5, t);
    if lum(l) < 0.07 {
        return;
    }
    let l = [l[0].min(1.5), l[1].min(1.5), l[2].min(1.5)];
    let wood = Rgb(124, 76, 36);
    let iron = Rgb(100, 100, 108);
    let brass = Rgb(226, 182, 74);
    let o = [v.cam.0, v.cam.1, 0.5];
    for col in c0..c1 {
        let cam_x = 2.0 * (col as f32 + 0.5) / v.vw as f32 - 1.0;
        let rd = [dirx + plx * cam_x, diry + ply * cam_x];
        for row in r0..r1 {
            let vz = -((row as f32 + 0.5) - v.horizon) / v.proj;
            let d = [rd[0], rd[1], vz];
            let mut best: Option<(f32, usize, bool, usize)> = None;
            for (bi, b) in boxes.iter().enumerate() {
                if let Some((tn, ax, mx)) = ray_box(o, d, b) {
                    if best.map_or(true, |bb| tn < bb.0) {
                        best = Some((tn, ax, mx, bi));
                    }
                }
            }
            let Some((tn, ax, maxside, bi)) = best else { continue };
            if tn >= zbuf[col as usize] || barred(col, row, tn) {
                continue;
            }
            let (hx, hy, hz) = (o[0] + d[0] * tn, o[1] + d[1] * tn, o[2] + d[2] * tn);
            // Local coordinates: `u` across its width, `w` front (+) to back (-).
            let (lx, ly) = (hx - cx, hy - cy);
            let w = lx * fx + ly * fy;
            let u = if fx != 0.0 { ly } else { lx };
            let au = u.abs() / HW;
            // Which face: the top, the front, the back, or a side.
            let face = if ax == 2 {
                if maxside { 0 } else { 4 }
            } else {
                let n = if maxside { 1.0 } else { -1.0 };
                let along_facing = (ax == 0 && fx != 0.0) || (ax == 1 && fy != 0.0);
                if along_facing {
                    let toward = n * if ax == 0 { fx } else { fy };
                    if toward > 0.0 { 1 } else { 2 }
                } else {
                    3
                }
            };
            let shade = [1.0, 0.85, 0.62, 0.72, 0.5][face];
            // Across the face you are looking at: its width for the front,
            // back and top, its depth for the ends.
            let across = if face == 3 { w.abs() / HD } else { au };
            let band = (0.55..0.68).contains(&across);
            let edge = across > 0.88;
            let lid = bi == 1;
            let (glyph, col_rgb, bgk) = if ch.open && bi == 0 && face == 0 {
                // Looking down into the emptied chest.
                (' ', Rgb(30, 18, 10), 1.0)
            } else if lid && face == 1 {
                // The underside of the lid thrown back.
                (if band { '|' } else { ' ' }, if band { iron } else { wood.scale(0.7) }, 0.55)
            } else if face == 0 {
                // The lid's top: boards running front to back, iron straps across.
                let seam = ((w / HD + 1.0) * 2.5).fract() < 0.12;
                if band || edge {
                    ('=', iron, 0.55)
                } else if seam {
                    ('-', wood.scale(0.6), 0.55)
                } else {
                    (' ', wood.scale(1.0 - (w / HD).powi(2) * 0.2), 0.6)
                }
            } else {
                let seam_z = !lid && (BODY - 0.025..BODY + 0.02).contains(&hz) && !ch.open;
                let plank = ((hz / 0.1).fract() < 0.15) && !lid;
                let lock = face == 1 && !lid && u.abs() < 0.07 && (0.18..0.34).contains(&hz);
                if lock {
                    let hole = u.abs() < 0.025 && (0.22..0.27).contains(&hz);
                    (if hole { '.' } else { 'o' }, if hole { Rgb(20, 14, 6) } else { brass }, 0.5)
                } else if seam_z {
                    ('_', Rgb(40, 26, 14), 0.4)
                } else if band || edge {
                    ('|', iron, 0.55)
                } else if plank {
                    ('_', wood.scale(0.55), 0.6)
                } else {
                    (' ', wood, 0.6)
                }
            };
            let lit = col_rgb.scale(shade).lit(l);
            c.set(v.vx + col, v.vy + row, glyph, lit.lerp(Rgb::WHITE, 0.08), lit.scale(bgk));
        }
    }
}

/// A chest on the maps: brass-bound brown, dull once it has been emptied.
fn chest_color(lv: &Level, x: i32, y: i32) -> Rgb {
    match lv.chest_at(x, y) {
        Some(i) if lv.chests[i].open => Rgb(110, 80, 50),
        _ => Rgb(215, 160, 70),
    }
}

/// A ward prism, hanging in the air.
const ART_PRISM: &[&str] = &[
    r"   /\   ",
    r"  /  \  ",
    r" / /\ \ ",
    r"/_/__\_\",
    r"\ \  / /",
    r" \ \/ / ",
    r"  \  /  ",
    r"   \/   ",
];
/// What is left of a broken ward.
const ART_SHARDS: &[&str] = &[r"  . /\ ,  ", r" /\/__\/\ "];

/// A slowly cycling rainbow, for prisms and their Guardians.
pub fn prismatic(t: f32, phase: f32) -> Rgb {
    let k = t * 0.9 + phase;
    let c = |o: f32| (165.0 + 90.0 * (k + o).sin()) as u8;
    Rgb(c(0.0), c(2.1), c(4.2))
}

fn art_width(art: &[&str]) -> usize {
    art.iter().map(|l| l.chars().count()).max().unwrap_or(1)
}

struct Spr {
    x: f32,
    y: f32,
    art: &'static [&'static str],
    glyph: char,
    color: Rgb,
    eye: Rgb,
    size: f32,
    float: f32,
    flash: f32,
    glow: bool,
    /// Champion / unique aura colour.
    aura: Option<Rgb>,
    /// Overlords throb with a heavy, pulsing outline.
    throb: bool,
    /// Asleep, or yet to notice you: Zs drift up from its head. The value
    /// offsets the animation so sleepers don't snore in unison.
    sleep: Option<f32>,
    d2: f32,
    /// This frame's blink, stride, lunge or death throes.
    pose: Pose,
}

/// Aura colour for champions (gold) and uniques (violet).
pub fn aura_of(m: &Monster) -> Option<Rgb> {
    if m.is_unique() {
        Some(Rgb(200, 90, 255))
    } else if m.overlord {
        Some(Rgb(255, 35, 60))
    } else if m.champion {
        Some(Rgb(255, 175, 40))
    } else {
        None
    }
}

/// Should monster `m` be drawn on the maps? Ghosts inside walls are unseen,
/// unless the player has Deathsight, which reveals everything.
fn map_shows(g: &Game, m: &Monster) -> bool {
    if !m.alive {
        return false;
    }
    if g.has(UPower::Deathsight) || g.vision_shows(m) {
        return true;
    }
    g.level.is_visible(m.x, m.y) && !(m.def().ghost && g.level.tile(m.x, m.y).opaque())
}

fn map_mon_color(m: &Monster) -> Rgb {
    if m.faction == Faction::Ally {
        Rgb(120, 255, 120)
    } else if let Some(a) = aura_of(m) {
        a
    } else {
        Rgb(255, 70, 50)
    }
}

pub fn render_game(c: &mut Canvas, g: &Game, t: f32) {
    let w = c.w as i32;
    let h = c.h as i32;
    let vy = 1;
    let vh = h - HUD_H - 1;
    // Swooping between views: the camera pulls up and away into the map, or
    // dives down into it.
    if let Some((e, to_over)) = g.view_anim {
        if !g.p.torch_out {
            draw_view_swoop(
                c,
                g,
                vy,
                w,
                vh,
                t,
                (e / VIEW_SWOOP).clamp(0.0, 1.0),
                to_over,
            );
            crate::overlay::screen_overlays(c, g, 0, vy, w, vh, t);
            post_effects(c, g, 0, vy, w, vh, t);
            draw_topbar(c, g, w, t);
            draw_hud(c, g, h - HUD_H, w, t);
            return;
        }
    }
    // With the torch blown out there is no map to read, only the dark.
    if g.overhead && !g.p.torch_out {
        render_overhead(c, g, 0, vy, w, vh, t);
        crate::overlay::screen_overlays(c, g, 0, vy, w, vh, t);
    } else {
        render_fp(c, g, 0, vy, w, vh, t);
        // Webs, flames and gas fill the view, but never hide the minimap.
        crate::overlay::screen_overlays(c, g, 0, vy, w, vh, t);
        if !g.p.torch_out {
            let mw = 23.min(w / 4);
            let mh = 11.min(vh / 3);
            draw_minimap(c, g, w - mw - 1, vy + 1, mw, mh);
        }
        draw_sound_arrows(c, g, 0, vy, w, vh, t);
    }
    post_effects(c, g, 0, vy, w, vh, t);
    draw_topbar(c, g, w, t);
    draw_hud(c, g, h - HUD_H, w, t);
}

/// Keen Hearing in first person: a blue arrow at the edge of the view for
/// each sound you picked out, pointing the way it reaches you, with a rough
/// guess at how many tiles off it is.
fn draw_sound_arrows(c: &mut Canvas, g: &Game, vx: i32, vy: i32, vw: i32, vh: i32, t: f32) {
    if vw < 20 || vh < 8 {
        return;
    }
    let (dirx, diry) = (g.cam.ang.cos(), g.cam.ang.sin());
    let half = (FOV / 2.0).tan();
    let (plx, ply) = (-diry * half, dirx * half);
    let inv = 1.0 / (plx * diry - dirx * ply);
    for pg in &g.pings {
        let life = (1.0 - pg.age / Ping::LIFE).clamp(0.0, 1.0);
        let blink = 0.75 + 0.25 * (t * 6.0 + (pg.x + pg.y) as f32).sin();
        let col = Rgb(90, 160, 255).scale((0.3 + 0.7 * life) * blink);
        let (rx, ry) = (pg.ax - g.cam.x, pg.ay - g.cam.y);
        let tx = inv * (diry * rx - dirx * ry);
        let ty = inv * (-ply * rx + plx * ry);
        let label = format!("~{}", pg.dist);
        let (x, y, arrow) = if ty > 0.0 && (tx / ty).abs() <= 1.0 {
            // Ahead: along the top edge, above where it is.
            let col_x = ((vw as f32 / 2.0) * (1.0 + tx / ty)) as i32;
            (col_x.clamp(1, vw - 5), 1, '^')
        } else if ty < 0.0 && tx.abs() < -ty * 0.4 {
            // Right behind you: along the bottom edge.
            (vw / 2 - 1, vh - 2, 'v')
        } else {
            // To one side: on that edge, higher if ahead, lower if behind.
            let a = (ty / (tx.abs() + ty.abs())).clamp(-1.0, 1.0);
            let row = ((vh as f32 / 2.0) * (1.0 - a * 0.8)) as i32;
            if tx < 0.0 {
                (1, row.clamp(1, vh - 2), '<')
            } else {
                (vw - 5, row.clamp(1, vh - 2), '>')
            }
        };
        let bg = Rgb(10, 20, 50).scale(0.5 + 0.5 * life);
        if arrow == '>' {
            c.text_bg(vx + x, vy + y, &format!("{:>3}", label), col, bg);
            c.set(vx + x + 3, vy + y, arrow, col.lerp(Rgb::WHITE, 0.3), bg);
        } else {
            c.set(vx + x, vy + y, arrow, col.lerp(Rgb::WHITE, 0.3), bg);
            c.text_bg(vx + x + 1, vy + y, &label, col, bg);
        }
    }
}

/// One frame of the camera swoop between views. Heading to overhead, the
/// first-person view shrinks away as the camera rises, then the map zooms out
/// from right above your head. Heading back, the map zooms in on you and the
/// first-person view grows up out of it.
fn draw_view_swoop(
    c: &mut Canvas,
    g: &Game,
    vy: i32,
    w: i32,
    vh: i32,
    t: f32,
    p: f32,
    to_over: bool,
) {
    let ease = |x: f32| x * x * (3.0 - 2.0 * x);
    let half = 0.45;
    let (show_over, scale, fade) = if to_over {
        if p < half {
            let q = ease(p / half);
            (false, 1.0 - 0.65 * q, 1.0 - 0.7 * q)
        } else {
            let q = ease((p - half) / (1.0 - half));
            (true, 4.0 - 3.0 * q, 0.3 + 0.7 * q)
        }
    } else if p < half {
        let q = ease(p / half);
        (true, 1.0 + 3.0 * q, 1.0 - 0.7 * q)
    } else {
        let q = ease((p - half) / (1.0 - half));
        (false, 0.35 + 0.65 * q, 0.3 + 0.7 * q)
    };
    let mut src = Canvas::new(w.max(1) as usize, vh.max(1) as usize);
    if show_over {
        render_overhead(&mut src, g, 0, 0, w, vh, t);
    } else {
        render_fp(&mut src, g, 0, 0, w, vh, t);
    }
    // Scale about the middle of the view (where you stand in both views).
    let (cx, cy) = (w as f32 / 2.0, vh as f32 / 2.0);
    for y in 0..vh {
        for x in 0..w {
            let sx = cx + (x as f32 + 0.5 - cx) / scale;
            let sy = cy + (y as f32 + 0.5 - cy) / scale;
            if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= vh as f32 {
                c.set(x, vy + y, ' ', Rgb::BLACK, Rgb::BLACK);
                continue;
            }
            let cell = src.get(sx as i32, sy as i32);
            c.set(x, vy + y, cell.ch, cell.fg.scale(fade), cell.bg.scale(fade));
        }
    }
}

// ================================================================ first person


const DARK: Cell = Cell { ch: ' ', fg: Rgb::BLACK, bg: Rgb::BLACK };

/// Everything one column of the first-person view needs to know.
struct Fp<'a> {
    g: &'a Game,
    wc: WallCtx,
    pal: &'a Palette,
    hb: f32,
    corrupt: f32,
    dirx: f32,
    diry: f32,
    plx: f32,
    ply: f32,
    proj: f32,
    horizon: f32,
    camx0: f32,
    camy0: f32,
    traps: &'a [(i32, i32)],
    pulse: f32,
    vw: i32,
    t: f32,
    /// Bare-bones detail: flat lighting and no texture noise on floors and ceilings.
    minimal: bool,
}

/// Cast the ray for screen column `col` and shade its walls, floor and
/// ceiling into `out` (one cell per row). Returns the wall distance for the
/// sprite depth buffer, and any gate the ray looked through.
fn cast_column(f: &Fp, col: i32, out: &mut [Cell]) -> (f32, Option<GateHit>) {
    let Fp {
        g,
        ref wc,
        pal,
        hb,
        corrupt,
        dirx,
        diry,
        plx,
        ply,
        proj,
        horizon,
        camx0,
        camy0,
        traps,
        pulse,
        vw,
        t,
        minimal,
    } = *f;
    let lv = &g.level;
    let vh = out.len() as i32;

    let camx = 2.0 * (col as f32 + 0.5) / vw as f32 - 1.0;
    let rdx = dirx + plx * camx;
    let rdy = diry + ply * camx;
    let rlen = (rdx * rdx + rdy * rdy).sqrt();
    let mut mapx = camx0.floor() as i32;
    let mut mapy = camy0.floor() as i32;
    let ddx = if rdx.abs() < 1e-9 {
        1e30
    } else {
        (1.0 / rdx).abs()
    };
    let ddy = if rdy.abs() < 1e-9 {
        1e30
    } else {
        (1.0 / rdy).abs()
    };
    let (stepx, mut sdx) = if rdx < 0.0 {
        (-1, (camx0 - mapx as f32) * ddx)
    } else {
        (1, (mapx as f32 + 1.0 - camx0) * ddx)
    };
    let (stepy, mut sdy) = if rdy < 0.0 {
        (-1, (camy0 - mapy as f32) * ddy)
    } else {
        (1, (mapy as f32 + 1.0 - camy0) * ddy)
    };
    let mut side = 0;
    let mut hit = false;
    let mut gate: Option<GateHit> = None;
    for _ in 0..96 {
        if sdx < sdy {
            sdx += ddx;
            mapx += stepx;
            side = 0;
        } else {
            sdy += ddy;
            mapy += stepy;
            side = 1;
        }
        let tl = lv.tile(mapx, mapy);
        let anim = if tl == Tile::DoorOpen {
            g.fx.door_at(mapx, mapy)
        } else {
            None
        };
        if (tl == Tile::GateClosed || anim.is_some()) && gate.is_none() {
            // Remember the gate, and look on through its bars.
            let gp = (if side == 0 { sdx - ddx } else { sdy - ddy }).max(0.02);
            let mut gu = if side == 0 {
                camy0 + gp * rdy
            } else {
                camx0 + gp * rdx
            };
            gu -= gu.floor();
            if (side == 0 && rdx < 0.0) || (side == 1 && rdy > 0.0) {
                gu = 1.0 - gu;
            }
            let ghx = camx0 + rdx * gp - rdx / rlen * 0.05;
            let ghy = camy0 + rdy * gp - rdy / rlen * 0.05;
            gate = Some(GateHit {
                perp: gp,
                u: gu,
                light: light_at(g, ghx, ghy, t),
                side_f: if side == 1 { 0.78 } else { 1.0 },
                mx: mapx,
                my: mapy,
                anim,
            });
        }
        if tl.opaque() {
            hit = true;
            break;
        }
    }
    
    let perp = (if side == 0 { sdx - ddx } else { sdy - ddy }).max(0.02);
    let zb = if hit { perp } else { 1e9 };
    let tile = lv.tile(mapx, mapy);
    let mut u = if side == 0 {
        camy0 + perp * rdy
    } else {
        camx0 + perp * rdx
    };
    u -= u.floor();
    if (side == 0 && rdx < 0.0) || (side == 1 && rdy > 0.0) {
        u = 1.0 - u;
    }
    let hx = camx0 + rdx * perp - rdx / rlen * 0.05;
    let hy = camy0 + rdy * perp - rdy / rlen * 0.05;
    let wl = light_at(g, hx, hy, t);
    let lh = proj / perp;
    let top = horizon - lh * 0.5;
    let bot = horizon + lh * 0.5;
    let face = if side == 0 {
        (mapx - stepx, mapy)
    } else {
        (mapx, mapy - stepy)
    };
    let torch = lv
        .torches
        .iter()
        .find(|tr| tr.x == mapx && tr.y == mapy && tr.fx == face.0 && tr.fy == face.1);
    let side_f = if side == 1 { 0.78 } else { 1.0 };

    for row in 0..vh {
        let yf = row as f32 + 0.5;
        // Iron bars of a gate in front of everything else in this column.
        if let Some(gh) = gate {
            let glh = proj / gh.perp;
            let gtop = horizon - glh * 0.5;
            if yf >= gtop && yf <= gtop + glh {
                if let Some((mult, forced, tint)) =
                    overlay_tex(lv.style, &gh, (yf - gtop) / glh, wc)
                {
                    let emissive = mult > 1.1;
                    let lf = if emissive { [1.0, 1.0, 1.0] } else { gh.light };
                    let bri = (lum(lf) * mult * gh.side_f).clamp(0.0, 1.3);
                    if bri >= 0.025 {
                        let c0 = tint
                            .unwrap_or(pal.wall)
                            .lit(lf)
                            .scale(mult * gh.side_f * 1.1);
                        out[row as usize] = Cell { ch: forced.unwrap_or('|'), fg: c0, bg: c0.scale(0.3) };
                    } else {
                        out[row as usize] = DARK;
                    }
                    continue;
                }
            }
        }
        let ch: char;
        let fg: Rgb;
        let bg: Rgb;
        if hit && yf >= top && yf <= bot {
            let v = (yf - top) / lh;
            let mut torch_cell = None;
            if let Some(tr) = torch {
                if (0.38f32..0.62).contains(&u) {
                    if (0.2f32..0.36).contains(&v) {
                        let k = hash2((t * 12.0) as i32 + col, row) % 6;
                        let fc = ['^', '*', '&', '%', '(', ')'][k as usize];
                        let heat = 1.0 - (v - 0.2) / 0.16;
                        let base = Rgb(
                            (255.0 * tr.color[0].min(1.0)) as u8,
                            (230.0 * tr.color[1].min(1.0)) as u8,
                            (180.0 * tr.color[2].min(1.0)) as u8,
                        );
                        let col_f = base.lerp(Rgb(255, 255, 200), heat * 0.6);
                        torch_cell = Some((fc, col_f, col_f.scale(0.35)));
                    } else if (0.36f32..0.52).contains(&v) {
                        torch_cell = Some(('|', Rgb(120, 80, 40), Rgb(30, 20, 10)));
                    }
                }
            }
            if let Some(tc) = torch_cell {
                ch = tc.0;
                fg = tc.1;
                bg = tc.2;
            } else {
                let (mult, forced, tint) = wall_tex(lv.style, tile, u, v, mapx, mapy, wc);
                let base = tint.unwrap_or(pal.wall);
                let emissive = forced.is_some() && mult > 1.3;
                let lf = if emissive { [1.0, 1.0, 1.0] } else { wl };
                let bri = (lum(lf) * mult * side_f).clamp(0.0, 1.3);
                if bri < 0.025 {
                    ch = ' ';
                    fg = Rgb::BLACK;
                    bg = Rgb::BLACK;
                } else {
                    let c0 = base.lit(lf).scale(mult * side_f * 1.1);
                    ch = forced
                        .unwrap_or(WALL_RAMP[(bri.sqrt() / 1.15 * 8.99).min(8.0) as usize]);
                    fg = c0;
                    bg = c0.scale(0.33);
                }
            }
        } else if yf > horizon {
            let rd = 0.5 * proj / (yf - horizon).max(0.01);
            let fx = camx0 + rdx * rd;
            let fy = camy0 + rdy * rd;
            let tx = fx.floor() as i32;
            let ty = fy.floor() as i32;
            let ft = lv.tile(tx, ty);
            let l = if minimal { light_flat(g, fx, fy) } else { light_at(g, fx, fy, t) };
            let u2 = fx - tx as f32;
            let v2 = fy - ty as f32;
            let mut base = pal.floor;
            let mut mult;
            let mut forced: Option<char> = None;
            let mut emit = Rgb::BLACK;
            let su = (u2 * 2.0).fract();
            let sv = (v2 * 2.0).fract();
            if su < 0.07 || sv < 0.07 {
                mult = 0.45;
            } else {
                mult =
                    0.82 + 0.3 * hashf(tx * 2 + (u2 * 2.0) as i32, ty * 2 + (v2 * 2.0) as i32);
            }
            match ft {
                Tile::StairsDown => {
                    let r = (u2 - 0.5).abs().max((v2 - 0.5).abs()) * 2.0;
                    let k = (r * 4.0).floor();
                    mult = 0.15 + 0.2 * k;
                    forced = Some(if (r * 4.0).fract() < 0.25 { '_' } else { '=' });
                    emit = Rgb(70, 20, 50).scale(0.5 + 0.5 * pulse);
                }
                Tile::StairsUp => {
                    let r = (u2 - 0.5).abs().max((v2 - 0.5).abs()) * 2.0;
                    let k = (r * 4.0).floor();
                    mult = 1.25 - 0.18 * k;
                    forced = Some(if (r * 4.0).fract() < 0.25 { '_' } else { '=' });
                    emit = Rgb(20, 45, 80).scale(0.5 + 0.5 * pulse);
                }
                Tile::DoorOpen => {
                    base = Rgb(120, 85, 50);
                }
                _ => {}
            }
            if lv.cloud_at(tx, ty) {
                // A drifting, glowing haze of spores.
                let k = hash2((fx * 3.0 + t * 1.5) as i32, (fy * 3.0 - t) as i32) % 4;
                forced = Some([':', '*', '.', 'o'][k as usize]);
                base = Rgb(220, 150, 210);
                emit = Rgb(70, 30, 60).scale(0.6 + 0.4 * pulse);
                mult = mult.max(0.9);
            }
            if !minimal && corrupt > 0.0 && ft == Tile::Floor {
                // Meat creeping across the flagstones, veins throbbing.
                let n = vnoise(fx * 1.3, fy * 1.3);
                if n < corrupt * 0.75 - 0.1 {
                    base = Rgb(125, 48, 52);
                    let vn = vnoise(fx * 4.0, fy * 4.0);
                    if (vn - 0.5).abs() < 0.04 {
                        forced = Some('~');
                        base = Rgb(200, 40, 50);
                        emit = Rgb(70, 0, 10).scale(hb);
                    }
                }
            }
            if !minimal && lv.inb(tx, ty) {
                let bi = lv.idx(tx, ty);
                let bl = lv.blood[bi];
                if bl > 0.0
                    && hashf((fx * 5.0).floor() as i32, (fy * 5.0).floor() as i32) < bl * 0.3
                {
                    base = crate::data::stain_colors(lv.stain_at(bi)).0;
                    forced = Some(if hashf(tx * 3, ty * 5) < 0.5 {
                        '%'
                    } else {
                        '~'
                    });
                }
            }
            if traps.contains(&(tx, ty)) && (u2 - 0.5).abs() < 0.22 && (v2 - 0.5).abs() < 0.22 {
                forced = Some('^');
                emit = Rgb(150, 30, 20);
                mult = 1.0;
            }
            let bri = (lum(l) * mult).clamp(0.0, 1.3);
            if bri < 0.02 && emit == Rgb::BLACK {
                ch = ' ';
                fg = Rgb::BLACK;
                bg = Rgb::BLACK;
            } else {
                let c0 = base.lit(l).scale(mult).add(emit);
                ch = forced.unwrap_or(FLOOR_RAMP[((bri / 1.3) * 5.99).min(5.0) as usize]);
                fg = c0;
                bg = c0.scale(0.28);
            }
        } else {
            let rd = 0.5 * proj / (horizon - yf).max(0.01);
            let fx = camx0 + rdx * rd;
            let fy = camy0 + rdy * rd;
            let l0 = if minimal { light_flat(g, fx, fy) } else { light_at(g, fx, fy, t) };
            let l = [l0[0] * 0.55, l0[1] * 0.55, l0[2] * 0.55];
            let beam = (fx.fract() < 0.06) || (fy.fract() < 0.06);
            let mult = if beam {
                0.5
            } else {
                0.85 + 0.25 * hashf(fx.floor() as i32, fy.floor() as i32)
            };
            let bri = (lum(l) * mult).clamp(0.0, 1.0);
            if bri < 0.02 {
                ch = ' ';
                fg = Rgb::BLACK;
                bg = Rgb::BLACK;
            } else {
                let c0 = pal.ceil.lit(l).scale(mult * 1.2);
                ch = if beam {
                    '='
                } else {
                    CEIL_RAMP[(bri * 4.99).min(4.0) as usize]
                };
                fg = c0;
                bg = c0.scale(0.25);
            }
        }
        out[row as usize] = Cell { ch, fg, bg };
    }
    (zb, gate)
}

/// Lighting for bare-bones detail: the level's light at the nearest tile and
/// a steady torch, without interpolation, flicker or spell lights.
fn light_flat(g: &Game, x: f32, y: f32) -> [f32; 3] {
    let lv = &g.level;
    if g.p.torch_out {
        return [0.0; 3];
    }
    let (ix, iy) = (x as i32, y as i32);
    let mut l = if lv.inb(ix, iy) { lv.light[lv.idx(ix, iy)] } else { [0.0; 3] };
    let d2 = (x - g.cam.x).powi(2) + (y - g.cam.y).powi(2);
    let s2 = (g.sight() as f32).powi(2);
    if s2 > 0.0 {
        let torch = 1.25 / (1.0 + d2 * 7.5 / s2);
        let tc = if g.p.invis > 0 { [0.45, 0.5, 0.8] } else { [1.0, 0.8, 0.55] };
        for k in 0..3 {
            l[k] += tc[k] * torch;
        }
    }
    l
}

pub fn render_fp(c: &mut Canvas, g: &Game, vx: i32, vy: i32, vw: i32, vh: i32, t: f32) {
    let lv = &g.level;
    let corrupt = g.corruption();
    let wc = WallCtx {
        t,
        dread: if lv.style == 3 && g.depth >= 10 {
            if g.p.has_amulet {
                2
            } else {
                1
            }
        } else {
            0
        },
        corrupt,
        stare: g.stare.filter(|s| s.2 > g.time).map(|s| (s.0, s.1)),
    };
    let mut pal = palette(lv.style);
    if corrupt > 0.0 {
        pal.wall = pal.wall.lerp(Rgb(150, 70, 70), corrupt * 0.35);
        pal.floor = pal.floor.lerp(Rgb(120, 60, 60), corrupt * 0.3);
        pal.ceil = pal.ceil.lerp(Rgb(90, 30, 35), corrupt * 0.45);
    }
    let hb = heartbeat(t);
    let shake = g.fx.shake;
    let ang = g.cam.ang + (t * 61.0).sin() * shake * 0.025;
    let (dirx, diry) = (ang.cos(), ang.sin());
    let half = (FOV / 2.0).tan();
    let (plx, ply) = (-diry * half, dirx * half);
    let proj = (vw as f32 / 2.0) / half * 0.5;
    let horizon = vh as f32 / 2.0 + g.cam.walk.sin() * 0.35 + (t * 47.0).cos() * shake * 0.8;
    let (camx0, camy0) = (g.cam.x, g.cam.y);
    let mut zbuf = vec![f32::MAX; vw.max(1) as usize];
    let mut gates: Vec<Option<GateHit>> = vec![None; vw.max(1) as usize];
    let traps: Vec<(i32, i32)> = lv
        .traps
        .iter()
        .filter(|tr| tr.revealed)
        .map(|tr| (tr.x, tr.y))
        .collect();
    let pulse = 0.5 + 0.5 * (t * 2.5).sin();

    // ---------- walls, floor and ceiling, one ray per column (or per pair
    // of columns at lower detail), spread over the CPU cores.
    let detail = crate::perf::detail();
    let step = detail.col_step() as usize;
    let fp = Fp {
        g,
        wc,
        pal: &pal,
        hb,
        corrupt,
        dirx,
        diry,
        plx,
        ply,
        proj,
        horizon,
        camx0,
        camy0,
        traps: &traps,
        pulse,
        vw,
        t,
        minimal: detail == crate::perf::Detail::Minimal,
    };
    let vwu = vw.max(0) as usize;
    let vhu = vh.max(0) as usize;
    let ncast = vwu.div_ceil(step);
    if vhu > 0 && ncast > 0 {
        let mut buf = vec![DARK; ncast * vhu];
        let mut res: Vec<(f32, Option<GateHit>)> = vec![(1e9, None); ncast];
        let threads = crate::perf::threads().min(ncast);
        if threads > 1 && ncast * vhu >= 5000 {
            let per = ncast.div_ceil(threads);
            let fp = &fp;
            std::thread::scope(|s| {
                for (k, (bc, rc)) in buf.chunks_mut(per * vhu).zip(res.chunks_mut(per)).enumerate() {
                    s.spawn(move || {
                        for (j, (o, r)) in bc.chunks_mut(vhu).zip(rc.iter_mut()).enumerate() {
                            *r = cast_column(fp, ((k * per + j) * step) as i32, o);
                        }
                    });
                }
            });
        } else {
            for (i, (o, r)) in buf.chunks_mut(vhu).zip(res.iter_mut()).enumerate() {
                *r = cast_column(&fp, (i * step) as i32, o);
            }
        }
        for col in 0..vwu {
            let i = col / step;
            zbuf[col] = res[i].0;
            gates[col] = res[i].1;
            for (row, cell) in buf[i * vhu..(i + 1) * vhu].iter().enumerate() {
                c.set(vx + col as i32, vy + row as i32, cell.ch, cell.fg, cell.bg);
            }
        }
    }
    let wc = fp.wc;

    // ---------- sprites
    let mut sprs: Vec<Spr> = Vec::new();
    let d2 = |x: f32, y: f32| (x - camx0).powi(2) + (y - camy0).powi(2);
    for corpse in &lv.corpses {
        // Still falling: the body shows once its death has played out.
        if g
            .fx
            .dying
            .iter()
            .any(|d| d.kind == corpse.kind && d.x == corpse.x && d.y == corpse.y)
        {
            continue;
        }
        let def = &MONSTERS[corpse.kind];
        let undead = def.death == DeathKind::Undead;
        let (x, y) = (corpse.x as f32 + 0.5, corpse.y as f32 + 0.5);
        // A dead Mimic lies gaping, just as it died.
        let mimic = corpse.kind == M_MIMIC;
        sprs.push(Spr {
            x,
            y,
            art: if mimic {
                ART_MIMIC_DEAD
            } else if undead {
                ART_BONES
            } else {
                ART_CORPSE
            },
            glyph: '%',
            color: if mimic {
                def.color.scale(0.8)
            } else if undead {
                Rgb(200, 195, 175)
            } else {
                def.color.lerp(Rgb(120, 15, 15), 0.6)
            },
            eye: Rgb::BLACK,
            size: if mimic { def.size } else { 0.14 },
            float: 0.0,
            flash: 0.0,
            glow: false,
            aura: None,
            throb: false,
            sleep: None,
            // The Mimic's gaping body lies over whatever it spilled.
            d2: d2(x, y) - if mimic { 0.01 } else { 0.0 },
            pose: Pose::default(),
        });
    }
    for it in &lv.items {
        let (x, y) = (it.x as f32 + 0.5, it.y as f32 + 0.5);
        let amulet = it.item == Item::Amulet;
        let special = it.item.unique().is_some();
        sprs.push(Spr {
            x,
            y,
            art: it.item.art(),
            glyph: it.item.glyph(),
            color: it.item.color(),
            eye: Rgb(255, 60, 40),
            size: if amulet { 0.3 } else { 0.2 },
            float: 0.04 + (t * 2.0 + x).sin() * 0.03 + if amulet { 0.15 } else { 0.0 },
            flash: 0.0,
            glow: amulet || special,
            aura: if special { Some(it.item.color()) } else { None },
            throb: false,
            sleep: None,
            d2: d2(x, y),
            pose: Pose::default(),
        });
    }
    for m in &lv.monsters {
        if !m.alive {
            continue;
        }
        let def = m.def();
        let lunge = m.lunge.2 * (1.0 - m.lunge.2) * 4.0 * 0.35;
        let x = m.rx + 0.5 + m.lunge.0 * lunge;
        let y = m.ry + 0.5 + m.lunge.1 * lunge;
        let mut color = def.color;
        if m.kind == M_GUARDIAN {
            color = prismatic(t, m.id as f32);
        }
        if m.faction == Faction::Ally {
            color = color.lerp(Rgb(120, 255, 140), 0.3);
        }
        let bob = if def.float > 0.0 {
            (t * 6.0 + m.id as f32).sin() * 0.05
        } else {
            0.0
        };
        let moving = ((m.x as f32 - m.rx).powi(2) + (m.y as f32 - m.ry).powi(2)).sqrt();
        let pose = live_pose(&Live {
            kind: m.kind,
            id: m.id,
            awake: m.awake,
            atk: m.atk,
            moving,
            t,
        });
        sprs.push(Spr {
            x,
            y,
            art: def.art,
            glyph: def.glyph,
            color,
            eye: def.eye,
            size: def.size,
            float: def.float + bob,
            flash: m.flash,
            glow: false,
            aura: aura_of(m),
            throb: m.overlord,
            sleep: (!m.awake).then_some(m.id as f32 * 0.37),
            d2: d2(x, y),
            pose,
        });
    }
    // Monsters dying: they play out their deaths, then lie as corpses.
    for d in &g.fx.dying {
        let def = &MONSTERS[d.kind];
        let (x, y) = (d.rx + 0.5, d.ry + 0.5);
        let k = d.age / crate::anim::DEATH_TIME;
        let mut color = def.color;
        if d.kind == M_GUARDIAN {
            color = prismatic(t, d.id as f32);
        }
        if d.ally {
            color = color.lerp(Rgb(120, 255, 140), 0.3);
        }
        sprs.push(Spr {
            x,
            y,
            art: def.art,
            glyph: def.glyph,
            // Life drains out of the colour as it goes down.
            color: color.lerp(color.scale(0.6), k.min(1.0)),
            eye: def.eye.scale((1.0 - k * 1.5).max(0.0)),
            size: def.size,
            float: def.float,
            flash: 0.0,
            glow: false,
            aura: None,
            throb: false,
            sleep: None,
            d2: d2(x, y),
            pose: death_pose(d.kind, d.id, k, d.corpse, d.rite == Some(false)),
        });
    }
    // The ward prism, or the shards left of it.
    if let Some(w) = lv.ward {
        let (x, y) = (w.x as f32 + 0.5, w.y as f32 + 0.5);
        if w.state == WardState::Broken {
            sprs.push(Spr {
                x,
                y,
                art: ART_SHARDS,
                glyph: '*',
                color: Rgb(160, 175, 190),
                eye: Rgb::BLACK,
                size: 0.14,
                float: 0.0,
                flash: 0.0,
                glow: false,
                aura: None,
                throb: false,
                sleep: None,
                d2: d2(x, y),
            pose: Pose::default(),
            });
        } else {
            let shield = w.state == WardState::Shielded;
            sprs.push(Spr {
                x,
                y,
                art: ART_PRISM,
                glyph: '^',
                color: prismatic(t, w.x as f32),
                eye: Rgb::BLACK,
                size: 0.45,
                float: 0.3 + 0.05 * (t * 1.6).sin(),
                flash: 0.0,
                glow: true,
                aura: if shield {
                    Some(Rgb(150, 230, 255).lerp(Rgb::WHITE, 0.4 * pulse))
                } else {
                    None
                },
                throb: shield,
                sleep: None,
                d2: d2(x, y),
            pose: Pose::default(),
            });
        }
    }
    // Figments look exactly like the real thing.
    for f in &g.figments {
        let def = &MONSTERS[f.kind];
        let (x, y) = (f.rx + 0.5, f.ry + 0.5);
        let moving = ((f.x as f32 - f.rx).powi(2) + (f.y as f32 - f.ry).powi(2)).sqrt();
        let fpose = live_pose(&Live {
            kind: f.kind,
            id: f.id,
            awake: true,
            atk: 0.0,
            moving,
            t,
        });
        let bob = if def.float > 0.0 {
            (t * 6.0 + f.id as f32).sin() * 0.05
        } else {
            0.0
        };
        sprs.push(Spr {
            x,
            y,
            art: def.art,
            glyph: def.glyph,
            color: def.color,
            eye: def.eye,
            size: def.size,
            float: def.float + bob,
            flash: 0.0,
            glow: false,
            aura: None,
            throb: false,
            sleep: None,
            d2: d2(x, y),
            pose: fpose,
        });
    }
    sprs.sort_by(|a, b| b.d2.partial_cmp(&a.d2).unwrap_or(std::cmp::Ordering::Equal));
    // Treasure chests (and the Mimics among them) are solid boxes, drawn in
    // the same far-to-near order as the sprites.
    let mut chests: Vec<(f32, Chest)> = lv
        .chests
        .iter()
        .map(|ch| (d2(ch.x as f32 + 0.5, ch.y as f32 + 0.5), *ch))
        .filter(|(dd, _)| *dd < 30.0 * 30.0)
        .collect();
    chests.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut next_chest = 0;
    let view = ChestView {
        cam: (camx0, camy0),
        dir: (dirx, diry),
        plane: (plx, ply),
        horizon,
        proj,
        vx,
        vy,
        vw,
        vh,
    };
    let inv = 1.0 / (plx * diry - dirx * ply);
    let project = |wx: f32, wy: f32| -> (f32, f32) {
        let rx = wx - camx0;
        let ry = wy - camy0;
        let tx = inv * (diry * rx - dirx * ry);
        let ty = inv * (-ply * rx + plx * ry);
        (tx, ty)
    };

    // Is this screen cell covered by the bars of a gate standing in front of depth `ty`?
    let style = lv.style;
    let barred = |col: i32, row: i32, ty: f32| -> bool {
        match gates.get(col as usize).copied().flatten() {
            Some(gh) if gh.perp < ty => {
                let glh = proj / gh.perp;
                let gtop = horizon - glh * 0.5;
                let yf = row as f32 + 0.5;
                yf >= gtop
                    && yf <= gtop + glh
                    && overlay_tex(style, &gh, (yf - gtop) / glh, &wc).is_some()
            }
            _ => false,
        }
    };
    for s in &sprs {
        while next_chest < chests.len() && chests[next_chest].0 >= s.d2 {
            let ch = chests[next_chest].1;
            draw_chest(c, g, &view, &ch, &zbuf, &barred, t);
            next_chest += 1;
        }
        let (tx, ty) = project(s.x, s.y);
        if ty <= 0.2 {
            continue;
        }
        let scx = (vw as f32 / 2.0) * (1.0 + tx / ty);
        let scale = proj / ty;
        let l = light_at(g, s.x, s.y, t);
        let lu = lum(l);
        let dark = lu < 0.07 && !s.glow;
        let pose = &s.pose;
        let art = pose.art.unwrap_or(s.art);
        let ah = art.len().max(1) as f32;
        let h_rows = s.size * scale;
        let bottom = horizon + scale * 0.5 - (s.float + pose.lift) * scale;
        if h_rows < 1.3 {
            // Too far off to show any shape: a dying one just winks out.
            if pose.fade > 0.5 {
                continue;
            }
            let col = scx as i32;
            let row = (bottom - 0.5) as i32;
            if col >= 0
                && col < vw
                && row >= 0
                && row < vh
                && ty < zbuf[col as usize]
                && !barred(col, row, ty)
            {
                if let Some(a) = s.aura {
                    c.set(
                        vx + col,
                        vy + row,
                        s.glyph,
                        a.lerp(Rgb::WHITE, 0.3 * pulse),
                        a.scale(0.25 + 0.15 * pulse),
                    );
                } else if dark {
                    if s.eye != Rgb::BLACK && s.size > 0.3 {
                        c.put(vx + col, vy + row, '.', s.eye.scale(0.8));
                    }
                } else {
                    c.put(vx + col, vy + row, s.glyph, s.color.lit(l));
                }
            }
            continue;
        }
        let k = h_rows / ah;
        let art = &crate::anim::Grid::new(art);
        // The art, read through this frame's pose: screen cell to art cell.
        let at = |col: i32, row: i32| -> Option<crate::anim::Cell> {
            let ax = (col as f32 + 0.5 - scx) / k;
            let ay = (bottom - (row as f32 + 0.5)) / k;
            crate::anim::sample(art, pose, ax, ay)
        };
        let (side, up, down) = crate::anim::reach(art, pose);
        let c0 = ((scx - side * k).floor() as i32).max(0);
        let c1 = ((scx + side * k).ceil() as i32).min(vw);
        let r0 = ((bottom - up * k).floor() as i32).max(0);
        let r1 = ((bottom + down * k).ceil() as i32).min(vh);
        // Read the posed art once for every cell it (and its halo) can touch.
        let pad = if s.aura.is_some() {
            (k * 0.8).ceil().max(1.0) as i32 + if s.throb { 1 } else { 0 }
        } else {
            0
        };
        let (bx0, by0) = (c0 - pad, r0 - pad);
        let (bw, bh) = ((c1 + pad - bx0).max(0) as usize, (r1 + pad - by0).max(0) as usize);
        let cells: Vec<Option<crate::anim::Cell>> = (0..bw * bh)
            .map(|i| at(bx0 + (i % bw) as i32, by0 + (i / bw) as i32))
            .collect();
        let cell_at = |col: i32, row: i32| -> Option<crate::anim::Cell> {
            let (i, j) = (col - bx0, row - by0);
            if i < 0 || j < 0 || i as usize >= bw || j as usize >= bh {
                return None;
            }
            cells[j as usize * bw + i as usize]
        };
        let lit = s.color.lit([l[0].min(1.6), l[1].min(1.6), l[2].min(1.6)]);
        let lit = match pose.tint {
            Some((tc, a)) => lit.lerp(tc, a),
            None => lit,
        };
        let shown = if s.glow {
            s.color.lerp(Rgb::WHITE, 0.2 + 0.2 * pulse)
        } else {
            lit.lerp(Rgb::WHITE, s.flash)
        };
        if let Some(a) = s.aura {
            // A flickering halo hugging the outline of the sprite. It glows,
            // so it shows even in total darkness.
            // Which cells lie within `pad` of the body: a box dilation, done
            // across and then down.
            let mut across = vec![false; bw * bh];
            for j in 0..bh {
                let mut last: i32 = -1_000_000;
                for i in 0..bw {
                    if cells[j * bw + i].is_some() {
                        last = i as i32;
                    }
                    across[j * bw + i] = i as i32 - last <= pad;
                }
                let mut next: i32 = 1_000_000;
                for i in (0..bw).rev() {
                    if cells[j * bw + i].is_some() {
                        next = i as i32;
                    }
                    across[j * bw + i] |= next - i as i32 <= pad;
                }
            }
            let near_at = |col: i32, row: i32| -> bool {
                let (i, j) = (col - bx0, row - by0);
                if i < 0 || j < 0 || i as usize >= bw || j as usize >= bh {
                    return false;
                }
                let lo = (j - pad).max(0) as usize;
                let hi = ((j + pad) as usize).min(bh - 1);
                (lo..=hi).any(|jj| across[jj * bw + i as usize])
            };
            // Overlords: a slow, heavy heartbeat of light.
            let beat = if s.throb {
                (0.5 + 0.5 * (t * 5.0).sin()).powf(2.0)
            } else {
                1.0
            };
            for col in (c0 - pad).max(0)..(c1 + pad).min(vw) {
                if ty >= zbuf[col as usize] {
                    continue;
                }
                for row in (r0 - pad).max(0)..(r1 + pad).min(vh) {
                    if cell_at(col, row).is_some() || barred(col, row, ty) || !near_at(col, row) {
                        continue;
                    }
                    let hsh = hash2(col * 31 + (t * 9.0) as i32, row * 17 + (t * 7.0) as i32);
                    if s.throb {
                        let k2 = 0.25 + 0.75 * beat;
                        let ch = ['#', '%', '*', '+'][(hsh % 4) as usize];
                        c.set(
                            vx + col,
                            vy + row,
                            ch,
                            a.scale(k2).lerp(Rgb::WHITE, 0.25 * beat),
                            a.scale(0.4 * k2),
                        );
                        continue;
                    }
                    let flick = 0.55 + 0.45 * ((hsh % 100) as f32 / 100.0) * pulse;
                    let ch = ['.', '\'', '*', '`', ':'][(hsh % 5) as usize];
                    c.set(
                        vx + col,
                        vy + row,
                        ch,
                        a.scale(flick).lerp(Rgb::WHITE, 0.15 * pulse),
                        a.scale(0.22 * flick),
                    );
                }
            }
        }
        let shown = match s.aura {
            Some(a) if dark => a.scale(0.55),
            Some(a) => shown.lerp(a, 0.25),
            None => shown,
        };
        let dark = dark && s.aura.is_none();
        for col in c0..c1 {
            if ty >= zbuf[col as usize] {
                continue;
            }
            for row in r0..r1 {
                let Some((ch, own)) = cell_at(col, row) else { continue };
                if barred(col, row, ty) {
                    continue;
                }
                if let Some(oc) = own {
                    // Teeth, tongue and throat keep their own colours, lit like the rest.
                    if !dark {
                        let oc = oc.lit([l[0].min(1.4), l[1].min(1.4), l[2].min(1.4)]);
                        c.set(vx + col, vy + row, ch, oc, oc.scale(0.25));
                    }
                    continue;
                }
                if ch == '§' {
                    let glow = s.eye.lerp(Rgb::WHITE, 0.15 * pulse);
                    c.set(
                        vx + col,
                        vy + row,
                        if k > 1.5 { '@' } else { 'o' },
                        glow,
                        s.eye.scale(0.3),
                    );
                } else if !dark {
                    c.set(vx + col, vy + row, ch, shown, shown.scale(0.18));
                }
            }
        }
    }

    for &(_, ch) in &chests[next_chest..] {
        draw_chest(c, g, &view, &ch, &zbuf, &barred, t);
    }

    // ---------- sleepers: Zs drifting up from the heads of monsters that are
    // asleep or have not noticed you. Only where you can see the sleeper.
    for s in &sprs {
        let Some(ph) = s.sleep else { continue };
        let (tx, ty) = project(s.x, s.y);
        if ty <= 0.2 || ty > 14.0 {
            continue;
        }
        if lum(light_at(g, s.x, s.y, t)) < 0.07 {
            continue;
        }
        let scx = (vw as f32 / 2.0) * (1.0 + tx / ty);
        let scale = proj / ty;
        let top = horizon + scale * 0.5 - s.float * scale - s.size * scale;
        let head_w = (s.size * scale * 0.35).max(1.0);
        // Three Zs climb up and to the right, growing and fading as they go.
        for k in 0..3 {
            let p = (t * 0.55 + ph + k as f32 / 3.0).fract();
            let rise = 1.0 + p * (2.0 + scale * 0.25).min(5.0);
            let col = (scx + head_w * 0.5 + p * 2.5) as i32;
            let row = (top - rise) as i32;
            if col < 0 || col >= vw || row < 0 || row >= vh || ty >= zbuf[col as usize] {
                continue;
            }
            let ch = if p < 0.4 { 'z' } else { 'Z' };
            let fade = (1.0 - p).powf(0.7) * (1.0 - (ty / 14.0).powi(2)).max(0.2);
            c.put(vx + col, vy + row, ch, Rgb(170, 190, 255).scale(0.35 + 0.65 * fade));
        }
    }

    // ---------- Monster Vision: a marker and a name for every creature nearby,
    // seen straight through the walls.
    if g.p.vision > 0 {
        for m in &lv.monsters {
            if !g.vision_shows(m) {
                continue;
            }
            let (wx, wy) = (m.rx + 0.5, m.ry + 0.5);
            let (tx, ty) = project(wx, wy);
            if ty <= 0.2 {
                continue;
            }
            let col = ((vw as f32 / 2.0) * (1.0 + tx / ty)) as i32;
            let scale = proj / ty;
            let def = m.def();
            let row = (horizon + scale * 0.5 - (def.float + def.size * 0.5) * scale) as i32;
            if col < 0 || col >= vw || row < 1 || row >= vh {
                continue;
            }
            let mc = map_mon_color(m);
            let blink = 0.7 + 0.3 * (t * 6.0 + m.id as f32).sin();
            c.set(
                vx + col,
                vy + row,
                'o',
                mc.lerp(Rgb::WHITE, 0.3 * blink),
                mc.scale(0.35 * blink),
            );
            let name = m.label();
            let name = if def.boss || def.unique > 0 {
                def.name.to_string()
            } else {
                name
            };
            let x0 = col - name.chars().count() as i32 / 2;
            for (i, chr) in name.chars().enumerate() {
                let xx = x0 + i as i32;
                if xx >= 0 && xx < vw {
                    c.set(vx + xx, vy + row - 1, chr, mc.scale(0.9), Rgb(10, 5, 5));
                }
            }
        }
    }

    // ---------- particles, projectiles, combat text
    // (particles are pure decoration, so bare-bones detail skips them)
    let parts: &[_] = if fp.minimal { &[] } else { &g.fx.parts };
    for p in parts {
        let (tx, ty) = project(p.x, p.y);
        if ty <= 0.2 {
            continue;
        }
        let col = ((vw as f32 / 2.0) * (1.0 + tx / ty)) as i32;
        if col < 0 || col >= vw || ty >= zbuf[col as usize] {
            continue;
        }
        let scale = proj / ty;
        let row = (horizon + (0.5 - p.z) * scale) as i32;
        if row < 0 || row >= vh {
            continue;
        }
        let fade = (p.life / p.max).clamp(0.0, 1.0);
        let color = if p.glow {
            p.color.scale(0.35 + 0.65 * fade)
        } else {
            let l = light_at(g, p.x, p.y, t);
            p.color
                .lit([l[0].max(0.25), l[1].max(0.25), l[2].max(0.25)])
        };
        c.put(vx + col, vy + row, p.ch, color);
    }
    for b in &g.fx.bolts {
        for k in 0..5 {
            let tt = b.t - k as f32 * 0.02;
            if tt < 0.0 {
                break;
            }
            let (bx, by) = b.pos_at(tt);
            let (tx, ty) = project(bx, by);
            if ty <= 0.2 {
                continue;
            }
            let col = ((vw as f32 / 2.0) * (1.0 + tx / ty)) as i32;
            if col < 0 || col >= vw || ty >= zbuf[col as usize] {
                continue;
            }
            let scale = proj / ty;
            let row = (horizon + (0.5 - b.z) * scale) as i32;
            if row < 0 || row >= vh {
                continue;
            }
            let ch = if k == 0 {
                b.ch
            } else if b.light.is_some() {
                '*'
            } else {
                '.'
            };
            c.set(
                vx + col,
                vy + row,
                ch,
                b.color.scale(1.0 - k as f32 * 0.18),
                b.color.scale(0.25 * (1.0 - k as f32 * 0.2)),
            );
            if k == 0 && b.light.is_some() && scale > 6.0 {
                for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let (cx2, cy2) = (col + ox, row + oy);
                    if cx2 >= 0 && cx2 < vw && cy2 >= 0 && cy2 < vh {
                        c.put(vx + cx2, vy + cy2, '*', b.color.scale(0.7));
                    }
                }
            }
        }
    }
    for ft in &g.fx.texts {
        let (tx, ty) = project(ft.x, ft.y);
        if ty <= 0.2 {
            continue;
        }
        let col = ((vw as f32 / 2.0) * (1.0 + tx / ty)) as i32;
        if col < 0 || col >= vw || ty >= zbuf[col as usize] {
            continue;
        }
        let scale = proj / ty;
        let row = (horizon + (0.5 - ft.z) * scale) as i32;
        if row < 0 || row >= vh {
            continue;
        }
        let a = ft.life.clamp(0.0, 1.0);
        let x0 = col - ft.text.len() as i32 / 2;
        for (i, chr) in ft.text.chars().enumerate() {
            let xx = x0 + i as i32;
            if xx >= 0 && xx < vw {
                c.set(
                    vx + xx,
                    vy + row,
                    chr,
                    ft.color.scale(0.4 + 0.6 * a),
                    Rgb::BLACK,
                );
            }
        }
    }

    draw_torch_hand(c, g, vx, vy, vw, vh, t);
    draw_viewmodel(c, g, vx, vy, vw, vh, t);
}

fn draw_viewmodel(c: &mut Canvas, g: &Game, vx: i32, vy: i32, vw: i32, vh: i32, t: f32) {
    let w = g.wdef();
    let art = view_art(w.art);
    let bobx = (g.cam.walk * 0.5).sin() * 2.0;
    let boby = g.cam.walk.sin().abs() * 1.0;
    let ph = 1.0 - g.cam.swing;
    let swinging = g.cam.swing > 0.0;
    let sw = if swinging { (ph * PI).sin() } else { 0.0 };
    let ah = art.len() as i32;
    let aw = art_width(art) as i32;
    let bx = vx + vw * 2 / 3 - aw / 2 + bobx as i32 - (sw * 14.0) as i32;
    let by = vy + vh - ah + 1 + boby as i32 - (sw * 4.0) as i32;
    let l = light_at(g, g.cam.x, g.cam.y, t);
    let shade = [
        l[0].min(1.2) * 0.75,
        l[1].min(1.2) * 0.75,
        l[2].min(1.2) * 0.75,
    ];
    let fade = if g.p.invis > 0 { 0.45 } else { 1.0 };
    let pulse = 0.5 + 0.5 * (t * 3.0).sin();
    // Unique weapons take their own colour and always glow.
    let uniq = g.p.wpn.uniq.map(|u| &UNIQUES[u as usize]);
    let wcolor = uniq.map_or(w.color, |u| u.color);
    let wglow = w.glow || uniq.is_some();
    let mut blade: Vec<(i32, i32)> = Vec::new();
    for (r, line) in art.iter().enumerate() {
        for (ci, ch) in line.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let x = bx + ci as i32;
            let y = by + r as i32;
            if y < vy || y >= vy + vh {
                continue;
            }
            let handle = r as f32 >= ah as f32 * 0.62 && w.art != WArt::Bow;
            if !handle {
                blade.push((x, y));
            }
            let mut col = if handle { Rgb(150, 105, 60) } else { wcolor };
            let mut ch2 = ch;
            if ch == '§' {
                ch2 = '*';
                col = wcolor.lerp(Rgb(150, 200, 255), 0.5 + 0.5 * pulse);
            } else {
                col = col.lit(shade);
                if wglow && !handle {
                    col = col.lerp(wcolor.lerp(Rgb::WHITE, 0.3), 0.4 + 0.3 * pulse);
                }
            }
            c.set(x, y, ch2, col.scale(fade), col.scale(0.22 * fade));
        }
    }
    if let Some(u) = uniq {
        weapon_aura(c, u.power, &blade, vy, vh, t, fade);
    }
    if swinging && sw > 0.3 {
        for i in 0..6 {
            let x = bx - 2 - i * 2 + (ph * 10.0) as i32;
            let y = by + i;
            if y >= vy && y < vy + vh {
                c.put(
                    x,
                    y,
                    '\\',
                    Rgb(255, 255, 255).scale(sw * (1.0 - i as f32 * 0.12)),
                );
            }
        }
    }
}

/// Living effects that play over a unique weapon held in view.
fn weapon_aura(
    c: &mut Canvas,
    power: UPower,
    blade: &[(i32, i32)],
    vy: i32,
    vh: i32,
    t: f32,
    fade: f32,
) {
    if blade.is_empty() {
        return;
    }
    let n = blade.len() as u32;
    let fract = |v: f32| v - v.floor();
    let put = |c: &mut Canvas, x: i32, y: i32, ch: char, fg: Rgb| {
        if y >= vy && y < vy + vh && x >= 0 && x < c.w as i32 {
            c.put(x, y, ch, fg.scale(fade));
        }
    };
    match power {
        // Stormcaller: every so often, arcs of lightning dance across the blade.
        UPower::Storm => {
            let cycle = t * 0.7;
            let ph = fract(cycle);
            if ph < 0.22 {
                let f = (t * 22.0) as i32;
                let seed = cycle.floor() as i32;
                let (mut x, mut y) = blade[(hash2(seed, 7) % n) as usize];
                for k in 0..9 {
                    let hh = hash2(f * 13 + k, seed * 31 + k);
                    x += (hh % 3) as i32 - 1;
                    y += ((hh / 3) % 3) as i32 - 1;
                    let ch = ['~', '/', '\\', '-', '*', '\''][(hh % 6) as usize];
                    let col = Rgb(150, 190, 255).lerp(Rgb::WHITE, ((hh % 10) as f32) / 10.0);
                    put(c, x, y, ch, col);
                }
            } else if hash2((t * 15.0) as i32, 3) % 9 == 0 {
                let (x, y) = blade[(hash2((t * 15.0) as i32, 5) % n) as usize];
                put(c, x, y, '\'', Rgb(200, 220, 255));
            }
        }
        // Cinderbrand: embers rise from the blade.
        UPower::Flame => {
            for i in 0..7 {
                let v = t * 0.9 + i as f32 * 0.37;
                let ph = fract(v);
                let (bx, by) = blade[(hash2(v.floor() as i32, i * 11) % n) as usize];
                let x = bx + ((ph * 6.0 + i as f32).sin() * 1.2) as i32;
                let y = by - (ph * 5.0) as i32;
                let col = Rgb(255, 230, 120).lerp(Rgb(200, 40, 0), ph);
                put(
                    c,
                    x,
                    y,
                    ['*', '\'', '.', '^'][(i % 4) as usize],
                    col.scale(1.0 - ph * 0.6),
                );
            }
        }
        // Tidecaller: frost glitters and a cold mist drifts down.
        UPower::Frost => {
            for i in 0..6 {
                let v = t * 1.6 + i as f32 * 0.53;
                let (bx, by) = blade[(hash2(v.floor() as i32, i * 7) % n) as usize];
                let tw = (fract(v) * PI).sin();
                if tw > 0.3 {
                    put(
                        c,
                        bx,
                        by,
                        if tw > 0.8 { '*' } else { '+' },
                        Rgb(170, 230, 255).lerp(Rgb::WHITE, tw * 0.6),
                    );
                }
                let m = fract(t * 0.5 + i as f32 * 0.29);
                let (mx, my) =
                    blade[(hash2((t * 0.5 + i as f32 * 0.29).floor() as i32, i) % n) as usize];
                put(
                    c,
                    mx + (i % 3) - 1,
                    my + (m * 4.0) as i32,
                    '.',
                    Rgb(140, 200, 240).scale(1.0 - m),
                );
            }
        }
        // Vyrmatha's Fang: drops of blood slide off and fall.
        UPower::Vampiric => {
            for i in 0..4 {
                let v = t * 0.6 + i as f32 * 0.5;
                let ph = fract(v);
                let (bx, by) = blade[(hash2(v.floor() as i32, i * 5) % n) as usize];
                let y = by + (ph * ph * 6.0) as i32;
                put(
                    c,
                    bx,
                    y,
                    if ph < 0.3 { ',' } else { '.' },
                    Rgb(220, 20, 40).scale(1.0 - ph * 0.5),
                );
            }
        }
        // Grutch's Cleaver: a cruel glint sweeps along the edge.
        UPower::Executioner => {
            let ph = fract(t * 0.45);
            if ph < 0.35 {
                let i = ((ph / 0.35) * n as f32) as usize;
                let (x, y) = blade[i.min(blade.len() - 1)];
                put(c, x, y, '*', Rgb::WHITE);
                put(c, x + 1, y, '-', Rgb(255, 240, 200));
                put(c, x - 1, y, '-', Rgb(255, 240, 200));
            }
        }
        _ => {}
    }
}

fn draw_torch_hand(c: &mut Canvas, g: &Game, vx: i32, vy: i32, vw: i32, vh: i32, t: f32) {
    if g.p.invis > 0 {
        return;
    }
    let bobx = (g.cam.walk * 0.5 + 1.0).sin() * 1.5;
    let boby = (g.cam.walk + 0.7).sin().abs();
    let x0 = vx + vw / 7 + bobx as i32;
    let y0 = vy + vh - 7 + boby as i32;
    let frame = (t * 14.0) as i32;
    // Smoke first, so the flame burns in front of it.
    for s in &g.smoke {
        let a = (s.life / s.max).clamp(0.0, 1.0);
        let sx = x0 + s.x.round() as i32;
        let sy = y0 + s.y.round() as i32;
        if sx < vx || sx >= vx + vw || sy < vy || sy >= vy + vh {
            continue;
        }
        let ch = ['o', 'o', '~', '(', ')', '~', '.', '`']
            [((a * 7.99) as usize + (s.max * 10.0) as usize % 2).min(7)];
        let grey = Rgb(150, 145, 140).scale(0.95 - 0.75 * a);
        let col = if a < 0.12 && !g.p.torch_out {
            Rgb(255, 150, 60).lerp(grey, a / 0.12)
        } else {
            grey
        };
        c.put(sx, sy, ch, col);
    }
    if g.p.torch_out && g.snuff_age >= 0.3 {
        // A dying ember on the wick, then nothing.
        let glow = (1.0 - (g.snuff_age - 0.3) / 3.0).max(0.0);
        if glow > 0.0 {
            c.set(
                x0,
                y0 + 3,
                '.',
                Rgb(255, 90, 30).scale(0.3 + 0.7 * glow),
                Rgb(40, 10, 0).scale(glow),
            );
        }
    }
    // Flint sparks as the torch is relit.
    if !g.p.torch_out && g.light_age < 0.3 {
        for k in 0..6 {
            let h = hash2(frame * 13 + k, k * 7 + frame);
            let sx = x0 + (h % 5) as i32 - 2;
            let sy = y0 + 1 + ((h / 5) % 3) as i32;
            let ch = ['*', '\'', '`', '.', '+'][(h % 5) as usize];
            c.set(sx, sy, ch, Rgb(255, 250, 200), Rgb(70, 50, 10));
        }
    }
    // How much of the flame shows: it gutters and leans in the wind before
    // dying, and grows from the wick once the sparks catch.
    let (visible, lean) = if g.p.torch_out {
        if g.snuff_age < 0.3 {
            let s = 1.0 - g.snuff_age / 0.3;
            (s, (1.0 - s) * 2.0)
        } else {
            (0.0, 0.0)
        }
    } else if g.light_age < 0.3 {
        (0.0, 0.0)
    } else {
        (((g.light_age - 0.3) / 0.35).clamp(0.0, 1.0), 0.0)
    };
    let flames = ['(', ')', '*', '^', '\'', ',', '"', '&'];
    let shape: [i32; 4] = [1, 2, 2, 1];
    let first_row = ((1.0 - visible) * 4.0).ceil() as usize;
    for (r, &half) in shape.iter().enumerate() {
        if visible <= 0.0 || r < first_row {
            continue;
        }
        let shift = ((3 - r as i32) as f32 * lean).round() as i32;
        for dx in -half..=half {
            let h = hash2(frame * 7 + dx, r as i32 * 13 + frame);
            if r == 0 && h % 3 == 0 {
                continue;
            }
            let ch = flames[(h % flames.len() as u32) as usize];
            let heat = r as f32 / 3.0;
            let mut col =
                Rgb(255, 80, 20).lerp(Rgb(255, 240, 150), heat + 0.2 * ((h % 5) as f32 / 5.0));
            if visible < 1.0 {
                col = col.lerp(Rgb(200, 40, 10), 1.0 - visible);
            }
            c.set(
                x0 + dx + shift,
                y0 + r as i32,
                ch,
                col,
                Rgb(90, 30, 5).scale(0.6 + 0.4 * heat),
            );
        }
    }
    for r in 4..8 {
        let y = y0 + r;
        if y < vy + vh {
            c.set(x0, y, '|', Rgb(130, 90, 50), Rgb(40, 25, 12));
            if r == 4 {
                c.set(x0 - 1, y, '[', Rgb(110, 110, 120), Rgb(30, 30, 35));
                c.set(x0 + 1, y, ']', Rgb(110, 110, 120), Rgb(30, 30, 35));
            }
        }
    }
}

/// What colour a found trap shows on the maps, by what it does.
fn trap_color(k: TrapKind) -> Rgb {
    match k {
        TrapKind::Spikes => Rgb(220, 220, 230),
        TrapKind::Gas => Rgb(140, 240, 80),
        TrapKind::Fire => Rgb(255, 120, 30),
        TrapKind::Alarm => Rgb(255, 220, 70),
        TrapKind::Maw => Rgb(240, 60, 80),
    }
}

fn draw_minimap(c: &mut Canvas, g: &Game, x0: i32, y0: i32, w: i32, h: i32) {
    if w < 7 || h < 5 {
        return;
    }
    let lv = &g.level;
    let (px, py) = (g.p.x, g.p.y);
    let bg = Rgb(8, 8, 12);
    c.frame(x0 - 1, y0 - 1, w + 2, h + 2, Rgb(70, 60, 80), bg);
    for j in 0..h {
        for i in 0..w {
            let tx = px - w / 2 + i;
            let ty = py - h / 2 + j;
            if !lv.inb(tx, ty) || !lv.explored[lv.idx(tx, ty)] {
                continue;
            }
            let vis = lv.visible[lv.idx(tx, ty)];
            let k = if vis { 1.0 } else { 0.55 };
            let (ch, col) = match lv.tile(tx, ty) {
                Tile::Wall => ('#', Rgb(110, 100, 90)),
                Tile::Floor => ('.', Rgb(80, 80, 80)),
                Tile::DoorClosed => ('+', Rgb(170, 120, 60)),
                Tile::DoorOpen => ('\'', Rgb(170, 120, 60)),
                Tile::StairsUp => ('<', Rgb(120, 180, 255)),
                Tile::StairsDown => ('>', Rgb(220, 120, 255)),
                Tile::Sinew => ('%', Rgb(210, 50, 60)),
                Tile::GateClosed => ('#', Rgb(150, 150, 160)),
                Tile::Ward => ('^', Rgb(200, 160, 255)),
                Tile::Crystal => ('#', Rgb(120, 230, 255)),
                Tile::Chest => ('=', chest_color(lv, tx, ty)),
            };
            c.set(x0 + i, y0 + j, ch, col.scale(k), bg);
        }
    }
    // Traps you have found.
    for tr in lv.traps.iter().filter(|tr| tr.revealed) {
        let (i, j) = (tr.x - px + w / 2, tr.y - py + h / 2);
        if i >= 0 && i < w && j >= 0 && j < h && lv.explored[lv.idx(tr.x, tr.y)] {
            let k = if lv.is_visible(tr.x, tr.y) { 1.0 } else { 0.6 };
            c.set(x0 + i, y0 + j, '^', trap_color(tr.kind).scale(k), bg);
        }
    }
    // Glinting Eye: items in sight, as the full map shows them.
    if g.bonus.glint {
        for it in &lv.items {
            if lv.is_visible(it.x, it.y) {
                let (i, j) = (it.x - px + w / 2, it.y - py + h / 2);
                if i >= 0 && i < w && j >= 0 && j < h {
                    c.set(x0 + i, y0 + j, it.item.glyph(), it.item.color(), bg);
                }
            }
        }
    }
    for m in &lv.monsters {
        if map_shows(g, m) {
            let (i, j) = (m.x - px + w / 2, m.y - py + h / 2);
            if i >= 0 && i < w && j >= 0 && j < h {
                c.set(x0 + i, y0 + j, m.def().glyph, map_mon_color(m), bg);
            }
        }
    }
    for f in &g.figments {
        if lv.is_visible(f.x, f.y) {
            let (i, j) = (f.x - px + w / 2, f.y - py + h / 2);
            if i >= 0 && i < w && j >= 0 && j < h {
                c.set(x0 + i, y0 + j, MONSTERS[f.kind].glyph, Rgb(255, 70, 50), bg);
            }
        }
    }
    c.set(
        x0 + w / 2,
        y0 + h / 2,
        ARROWS[g.p.facing as usize],
        Rgb(255, 255, 120),
        Rgb(60, 50, 0),
    );
}

// ================================================================ overhead

pub fn render_overhead(c: &mut Canvas, g: &Game, vx: i32, vy: i32, vw: i32, vh: i32, t: f32) {
    let lv = &g.level;
    let pal = palette(lv.style);
    let corrupt = g.corruption();
    let (ox, oy) = (g.p.x, g.p.y);
    let hw = vw / 2 - 1;
    let hh = vh / 2;
    let pulse = 0.5 + 0.5 * (t * 3.0).sin();
    let to_screen = |wx: f32, wy: f32| -> (i32, i32) {
        (
            ((wx - ox as f32) * 2.0).round() as i32 + hw,
            (wy - oy as f32).round() as i32 + hh,
        )
    };
    let traps: Vec<(i32, i32)> = lv
        .traps
        .iter()
        .filter(|tr| tr.revealed)
        .map(|tr| (tr.x, tr.y))
        .collect();

    for ty in (oy - hh - 1)..=(oy + hh + 1) {
        for tx in (ox - hw / 2 - 1)..=(ox + hw / 2 + 1) {
            if !lv.inb(tx, ty) {
                continue;
            }
            let i = lv.idx(tx, ty);
            if !lv.explored[i] {
                continue;
            }
            let sx = (tx - ox) * 2 + hw;
            let sy = (ty - oy) + hh;
            let vis = lv.visible[i];
            let l = if vis {
                light_at(g, tx as f32 + 0.5, ty as f32 + 0.5, t)
            } else {
                [0.16, 0.18, 0.28]
            };
            let tile = lv.tile(tx, ty);
            let (c1, c2, base, solid) = match tile {
                Tile::Wall => {
                    let h = hash2(tx, ty);
                    if corrupt > 0.0 && hashf(tx * 7 + 3, ty * 11 + 5) * 0.6 < corrupt * 1.3 - 0.45
                    {
                        let beat = heartbeat(t + (tx + ty) as f32 * 0.03);
                        (
                            if h % 5 == 0 { 'o' } else { '%' },
                            '~',
                            Rgb(150, 58, 62).lerp(Rgb(220, 40, 50), beat * 0.5),
                            true,
                        )
                    } else {
                        let ch = if h % 7 == 0 { '%' } else { '#' };
                        (ch, '#', pal.wall, true)
                    }
                }
                Tile::Floor => {
                    let h = hashf(tx, ty);
                    (if h < 0.15 { ',' } else { '.' }, ' ', pal.floor, false)
                }
                Tile::DoorClosed => ('+', '+', Rgb(170, 115, 60), true),
                Tile::DoorOpen => match g.fx.door_at(tx, ty) {
                    // A door still opening.
                    Some((k, p)) => {
                        use crate::fx::DoorOpening::*;
                        let ch = match k {
                            Rise => {
                                if p < 0.5 {
                                    '#'
                                } else {
                                    '^'
                                }
                            }
                            Swing => {
                                if p < 0.5 {
                                    '+'
                                } else {
                                    '/'
                                }
                            }
                            Split => {
                                if p < 0.5 {
                                    '|'
                                } else {
                                    '\''
                                }
                            }
                            Dissolve => {
                                if p < 0.5 {
                                    '*'
                                } else {
                                    '.'
                                }
                            }
                        };
                        (
                            ch,
                            ch,
                            Rgb(190, 150, 100).lerp(Rgb::WHITE, 0.3 * (1.0 - p)),
                            false,
                        )
                    }
                    None => ('\'', ' ', Rgb(170, 115, 60), false),
                },
                Tile::StairsUp => (
                    '<',
                    '<',
                    Rgb(130, 190, 255).lerp(Rgb::WHITE, 0.3 * pulse),
                    false,
                ),
                Tile::StairsDown => (
                    '>',
                    '>',
                    Rgb(210, 120, 255).lerp(Rgb::WHITE, 0.3 * pulse),
                    false,
                ),
                Tile::Sinew => (
                    '%',
                    '}',
                    Rgb(200, 50, 60).lerp(Rgb(255, 90, 90), 0.4 * pulse),
                    true,
                ),
                Tile::GateClosed => ('#', '#', Rgb(150, 150, 160), false),
                Tile::Ward => (
                    '/',
                    '\\',
                    Rgb(200, 160, 255).lerp(Rgb(120, 240, 255), pulse),
                    false,
                ),
                Tile::Crystal => (
                    '#',
                    '#',
                    Rgb(120, 230, 255).lerp(Rgb(230, 140, 255), pulse),
                    true,
                ),
                Tile::Chest => ('[', ']', chest_color(lv, tx, ty), false),
            };
            let stairs = matches!(tile, Tile::StairsUp | Tile::StairsDown);
            let mut fg = if stairs {
                base.scale(if vis { 1.0 } else { 0.6 })
            } else {
                base.lit(l).scale(1.1)
            };
            let mut ch1 = c1;
            let mut ch2 = c2;
            if !vis && !stairs {
                fg = base.lerp(Rgb(70, 80, 120), 0.6).scale(0.45);
            }
            let mut bg = if solid {
                fg.scale(0.35)
            } else {
                fg.scale(0.12)
            };
            if tile == Tile::Floor || tile == Tile::DoorOpen {
                let bl = lv.blood[i];
                if bl > 0.0 && vis {
                    let (sf, sb) = crate::data::stain_colors(lv.stain_at(i));
                    fg = fg.lerp(sf.scale(1.2), (bl * 0.4).min(0.8));
                    bg = bg.lerp(sb, (bl * 0.3).min(0.6));
                    if bl > 1.0 {
                        ch2 = ',';
                    }
                }
                if traps.contains(&(tx, ty)) {
                    ch1 = '^';
                    fg = Rgb(255, 70, 40);
                }
            }
            if vis && lv.torches.iter().any(|tr| tr.x == tx && tr.y == ty) {
                let k = hash2((t * 10.0) as i32, tx * 7 + ty) % 3;
                ch1 = ['*', '^', '&'][k as usize];
                fg = Rgb(255, 200, 90).lerp(Rgb(255, 90, 20), (k as f32) / 3.0);
            }
            if sx >= 0 && sx < vw && sy >= 0 && sy < vh {
                c.set(vx + sx, vy + sy, ch1, fg, bg);
            }
            if sx + 1 >= 0 && sx + 1 < vw && sy >= 0 && sy < vh {
                c.set(vx + sx + 1, vy + sy, ch2, fg, bg);
            }
        }
    }

    let put = |c: &mut Canvas, sx: i32, sy: i32, ch: char, fg: Rgb| {
        if sx >= 0 && sx < vw && sy >= 0 && sy < vh {
            c.put(vx + sx, vy + sy, ch, fg);
        }
    };
    for corpse in &lv.corpses {
        if lv.is_visible(corpse.x, corpse.y) {
            let (sx, sy) = to_screen(corpse.x as f32, corpse.y as f32);
            put(c, sx, sy, '%', MONSTERS[corpse.kind].color.scale(0.6));
        }
    }
    for it in &lv.items {
        if lv.is_visible(it.x, it.y) {
            let (sx, sy) = to_screen(it.x as f32, it.y as f32);
            let mut col = it.item.color();
            if it.item == Item::Amulet {
                col = col.lerp(Rgb::WHITE, pulse * 0.6);
            }
            put(c, sx, sy, it.item.glyph(), col);
        }
    }
    for m in &lv.monsters {
        if !map_shows(g, m) {
            continue;
        }
        let def = m.def();
        let lunge = m.lunge.2 * (1.0 - m.lunge.2) * 4.0 * 0.35;
        let (sx, sy) = to_screen(m.rx + m.lunge.0 * lunge, m.ry + m.lunge.1 * lunge);
        let mut col = def.color.lerp(Rgb::WHITE, 0.25).lerp(Rgb::WHITE, m.flash);
        if !m.awake {
            col = col.scale(0.6);
        }
        if sx >= 0 && sx < vw && sy >= 0 && sy < vh {
            let bg = if m.faction == Faction::Ally {
                Rgb(0, 60, 0)
            } else if m.flash > 0.3 {
                Rgb(120, 0, 0)
            } else if let Some(a) = aura_of(m) {
                a.scale(0.25 + 0.2 * pulse)
            } else {
                c.get(vx + sx, vy + sy).bg
            };
            c.set(vx + sx, vy + sy, def.glyph, col, bg);
            if !m.awake {
                // A z beside it that rises into a Z above, then starts again.
                let p = (t * 0.55 + m.id as f32 * 0.37).fract();
                let col = Rgb(170, 190, 255).scale(1.0 - 0.6 * p);
                if p < 0.5 {
                    put(c, sx + 1, sy, 'z', col);
                } else {
                    put(c, sx + 2, sy - 1, 'Z', col);
                }
            } else if m.stun > 0 {
                put(c, sx + 1, sy, '*', Rgb(255, 255, 120));
            }
        }
    }
    for cl in &lv.clouds {
        if !lv.is_visible(cl.x, cl.y) {
            continue;
        }
        let (sx, sy) = to_screen(cl.x as f32, cl.y as f32);
        let k = ((hash2(cl.x, cl.y) % 7) as f32 + t * 3.0) as i32;
        for (ox, ch) in [(0, ':'), (1, '*')] {
            if sx + ox >= 0 && sx + ox < vw && sy >= 0 && sy < vh {
                let col = Rgb(220, 150, 210).scale(0.7 + 0.3 * ((k + ox) % 2) as f32);
                c.set(vx + sx + ox, vy + sy, ch, col, Rgb(50, 20, 45));
            }
        }
    }
    for f in &g.figments {
        if !lv.is_visible(f.x, f.y) {
            continue;
        }
        let def = &MONSTERS[f.kind];
        let (sx, sy) = to_screen(f.rx, f.ry);
        if sx >= 0 && sx < vw && sy >= 0 && sy < vh {
            let bg = c.get(vx + sx, vy + sy).bg;
            c.set(
                vx + sx,
                vy + sy,
                def.glyph,
                def.color.lerp(Rgb::WHITE, 0.25),
                bg,
            );
        }
    }
    // Keen Hearing: sound waves rippling out from what you heard, washing
    // along the corridors and flaring where they strike the walls.
    for pg in &g.pings {
        let life = 1.0 - pg.age / Ping::LIFE;
        // Distances are in half-tiles, muffling included: a wave seems to
        // hang at a door or a sharp corner before it carries on, dimmer.
        let speed = 9.0;
        let reach = pg.reach as f32 / 2.0;
        let period = reach / speed + 0.7;
        // A ring every period; two rings in flight at once.
        for ring in 0..2 {
            let r = ((pg.age + ring as f32 * period * 0.5) % period) * speed;
            for &(x, y, d2) in &pg.wave {
                let d = d2 as f32 / 2.0;
                let off = (d - r).abs();
                if off > 0.75 {
                    continue;
                }
                let (sx, sy) = to_screen(x as f32, y as f32);
                if sx < 0 || sx + 1 >= vw || sy < 0 || sy >= vh {
                    continue;
                }
                let k = (1.0 - off / 0.75) * life * (1.0 - d / (reach + 1.0)).max(0.15);
                if lv.tile(x, y).opaque() {
                    // The wave strikes stone and bounces back.
                    let col = Rgb(150, 200, 255).scale(0.35 + 0.65 * k);
                    c.set(vx + sx, vy + sy, '#', col, Rgb(20, 40, 90).scale(0.4 + 0.6 * k));
                    c.set(vx + sx + 1, vy + sy, '#', col, Rgb(20, 40, 90).scale(0.4 + 0.6 * k));
                } else {
                    let col = Rgb(90, 150, 255).scale(0.3 + 0.7 * k);
                    let ch = if d2 == 0 { '*' } else { '~' };
                    put(c, sx, sy, ch, col);
                    put(c, sx + 1, sy, if d2 == 0 { ' ' } else { '~' }, col.scale(0.7));
                }
            }
        }
    }
    // the player
    let (psx, psy) = (hw, hh);
    let pcol = if g.p.invis > 0 {
        Rgb(120, 120, 200)
    } else {
        Rgb(255, 255, 200)
    };
    if psx < vw && psy < vh {
        c.set(vx + psx, vy + psy, '@', pcol, Rgb(60, 50, 20));
        c.put(
            vx + psx + 1,
            vy + psy,
            ARROWS[g.p.facing as usize],
            Rgb(255, 220, 90),
        );
    }
    // Only what happens where you can see it: a fight beyond the walls is
    // something you hear, not something you watch.
    let seen = |x: f32, y: f32| lv.is_visible(x.floor() as i32, y.floor() as i32);
    for p in &g.fx.parts {
        if !seen(p.x, p.y) {
            continue;
        }
        let (sx, sy) = to_screen(p.x - 0.5, p.y - 0.5 - p.z * 0.6);
        let fade = (p.life / p.max).clamp(0.0, 1.0);
        put(c, sx, sy, p.ch, p.color.scale(0.35 + 0.65 * fade));
    }
    for b in &g.fx.bolts {
        for k in 0..4 {
            let tt = b.t - k as f32 * 0.025;
            if tt < 0.0 {
                break;
            }
            let (bx, by) = b.pos_at(tt);
            if !seen(bx, by) {
                continue;
            }
            let (sx, sy) = to_screen(bx - 0.5, by - 0.5);
            let ch = if k == 0 { b.ch } else { '.' };
            put(c, sx, sy, ch, b.color.scale(1.0 - k as f32 * 0.22));
        }
    }
    for ft in &g.fx.texts {
        if !seen(ft.x, ft.y) {
            continue;
        }
        let (sx, sy) = to_screen(ft.x - 0.5, ft.y - 0.5 - ft.z * 1.5);
        let a = ft.life.clamp(0.0, 1.0);
        for (i, chr) in ft.text.chars().enumerate() {
            put(c, sx + i as i32, sy, chr, ft.color.scale(0.4 + 0.6 * a));
        }
    }
}

// ================================================================ HUD

fn post_effects(c: &mut Canvas, g: &Game, vx: i32, vy: i32, vw: i32, vh: i32, t: f32) {
    if g.fx.flash_amt > 0.0 {
        c.tint_region(vx, vy, vw, vh, g.fx.flash, (g.fx.flash_amt * 0.6).min(0.7));
    }
    let frac = g.p.hp as f32 / g.p.max_hp.max(1) as f32;
    if frac < 0.3 && g.over.is_none() && crate::perf::detail() != crate::perf::Detail::Minimal {
        let pulse = (0.5 + 0.5 * (t * 4.0).sin()) * (0.3 - frac) / 0.3;
        let edge = 4;
        for y in vy..vy + vh {
            for x in vx..vx + vw {
                let d = (x - vx)
                    .min(vx + vw - 1 - x)
                    .min((y - vy) * 2)
                    .min((vy + vh - 1 - y) * 2);
                if d < edge {
                    let a = pulse * (1.0 - d as f32 / edge as f32) * 0.7;
                    c.tint_region(x, y, 1, 1, Rgb(160, 0, 0), a);
                }
            }
        }
    }
    if g.p.poison > 0 {
        c.tint_region(
            vx,
            vy,
            vw,
            vh,
            Rgb(60, 200, 40),
            0.06 + 0.04 * (t * 3.0).sin(),
        );
    }
    if g.p.invis > 0 {
        c.tint_region(vx, vy, vw, vh, Rgb(40, 40, 110), 0.18);
    }
    if g.p.grit && g.p.blind > 0 && !g.grit_landing() {
        // Strongest while the grit is fresh, easing on the last turn.
        let s = (g.p.blind as f32 / 2.0).min(1.0);
        blur_region(c, vx, vy, vw, vh, s, t);
    }
}

/// How much of a cell its glyph covers, so a blur can treat a cell as one
/// colour: a space shows only its background, a full block only ink.
fn ink(ch: char) -> f32 {
    match ch {
        ' ' => 0.0,
        '.' | ',' | '\'' | '`' | '·' => 0.12,
        '░' => 0.25,
        '▒' => 0.5,
        '▓' => 0.75,
        '█' => 1.0,
        '▀' | '▄' | '▌' | '▐' => 0.5,
        _ => 0.35,
    }
}

/// Grit in the eyes: colours bleed into their neighbours, fine detail
/// smears into soft shapes, and the picture swims a little as the eyes water.
fn blur_region(c: &mut Canvas, vx: i32, vy: i32, vw: i32, vh: i32, s: f32, t: f32) {
    let x0 = vx.max(0);
    let y0 = vy.max(0);
    let x1 = (vx + vw).min(c.w as i32);
    let y1 = (vy + vh).min(c.h as i32);
    let (w, h) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
    if w == 0 || h == 0 || s <= 0.0 {
        return;
    }
    let src: Vec<crate::canvas::Cell> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| c.get(x0 + x as i32, y0 + y as i32))
        .collect();
    // Each cell as the single colour the eye sees from a distance.
    let flat: Vec<[f32; 3]> = src
        .iter()
        .map(|cl| {
            let k = ink(cl.ch);
            let m = cl.bg.lerp(cl.fg, k);
            [m.0 as f32, m.1 as f32, m.2 as f32]
        })
        .collect();
    // Box blur, wider across than down: terminal cells are about twice as
    // tall as they are wide.
    let rx = 1 + (2.0 * s).round() as i32;
    let ry = 1;
    let mut tmp = vec![[0.0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 3];
            let mut n = 0.0;
            for dx in -rx..=rx {
                let xx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                let v = flat[y * w + xx];
                acc[0] += v[0];
                acc[1] += v[1];
                acc[2] += v[2];
                n += 1.0;
            }
            tmp[y * w + x] = [acc[0] / n, acc[1] / n, acc[2] / n];
        }
    }
    let mut soft = vec![[0.0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 3];
            let mut n = 0.0;
            for dy in -ry..=ry {
                let yy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                let v = tmp[yy * w + x];
                acc[0] += v[0];
                acc[1] += v[1];
                acc[2] += v[2];
                n += 1.0;
            }
            soft[y * w + x] = [acc[0] / n, acc[1] / n, acc[2] / n];
        }
    }
    let rgb = |v: [f32; 3]| Rgb(v[0] as u8, v[1] as u8, v[2] as u8);
    // The eyes water: a slow sideways swim, so glyphs ghost from a neighbour.
    let swim = (t * 1.7).sin() * 0.5 + (t * 2.9).sin() * 0.5;
    let ghost = if swim > 0.35 { 1 } else if swim < -0.35 { -1 } else { 0 };
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let cell = src[i];
            let b = rgb(soft[i]);
            // Double vision: an empty cell catches a faint ghost of its
            // neighbour's glyph; real glyphs stay where they are.
            let gx = (x as i32 + ghost).clamp(0, w as i32 - 1) as usize;
            let gcell = src[y * w + gx];
            let (ch, fg) = if s > 0.6 && ink(cell.ch) == 0.0 && ink(gcell.ch) > 0.0 {
                (gcell.ch, gcell.fg.lerp(b, 0.5))
            } else {
                (cell.ch, cell.fg)
            };
            c.set(
                x0 + x as i32,
                y0 + y as i32,
                ch,
                fg.lerp(b, 0.55 * s),
                cell.bg.lerp(b, 0.85 * s),
            );
        }
    }
}

fn bar(c: &mut Canvas, x: i32, y: i32, w: i32, frac: f32, full: Rgb, empty: Rgb) {
    let n = ((frac.clamp(0.0, 1.0)) * w as f32).round() as i32;
    for i in 0..w {
        if i < n {
            let k = 0.75 + 0.25 * (i as f32 / w as f32);
            c.put(x + i, y, '#', full.scale(k));
        } else {
            c.put(x + i, y, '-', empty);
        }
    }
}

fn draw_topbar(c: &mut Canvas, g: &Game, w: i32, t: f32) {
    let bg = Rgb(14, 10, 18);
    c.fill(0, 0, w, 1, ' ', Rgb::WHITE, bg);
    c.text(
        1,
        0,
        "GRAZZARK",
        Rgb(200, 60, 50).lerp(Rgb(255, 150, 80), 0.5 + 0.5 * (t * 1.3).sin()),
    );
    let cd = g.cdef();
    c.text(11, 0, &format!("{} lv{}", cd.name, g.p.level), cd.color);
    let labels = ["E", "SE", "S", "SW", "W", "NW", "N", "NE"];
    let strip = 41.min(w - 54).max(0);
    let sx0 = w / 2 - strip / 2;
    let ang = if g.overhead {
        facing_angle(g.p.facing)
    } else {
        g.cam.ang
    };
    for i in 0..strip {
        let a = ang + (i - strip / 2) as f32 * (PI / 40.0);
        let mut a2 = a % (2.0 * PI);
        if a2 < 0.0 {
            a2 += 2.0 * PI;
        }
        let slot = a2 / (PI / 4.0);
        let near = (slot - slot.round()).abs() < 0.08;
        if near {
            let li = (slot.round() as i32).rem_euclid(8) as usize;
            let col = if labels[li] == "N" {
                Rgb(255, 80, 60)
            } else {
                Rgb(220, 200, 150)
            };
            c.text(sx0 + i, 0, labels[li], col);
        } else if c.get(sx0 + i, 0).ch == ' ' {
            c.put(
                sx0 + i,
                0,
                if i == strip / 2 { '|' } else { '.' },
                Rgb(80, 70, 90),
            );
        }
    }
    let help = "?:help M:map Shift+R:mode";
    c.text(w - help.len() as i32 - 1, 0, help, Rgb(110, 100, 120));
}

fn draw_hud(c: &mut Canvas, g: &Game, y0: i32, w: i32, t: f32) {
    let bg = Rgb(12, 10, 16);
    c.fill(0, y0, w, HUD_H, ' ', Rgb(180, 180, 180), bg);
    let pal = palette(g.level.style);
    for x in 0..w {
        c.put(x, y0, '=', pal.accent.scale(0.35));
    }
    let name = g.floor_name();
    let head = format!(" Depth {}: {} ", g.depth, name);
    c.text(2, y0, &head, pal.accent);
    // The living dungeon's torments.
    let mut cx = 2 + head.chars().count() as i32 + 1;
    for a in &g.curses {
        let tag = format!(" {} {} ", a.kind.name(), a.turns);
        if cx + tag.len() as i32 > w - 40 {
            break;
        }
        let col = a
            .kind
            .color()
            .lerp(Rgb::WHITE, 0.25 + 0.25 * (t * 4.0).sin());
        c.text_bg(cx, y0, &tag, Rgb::BLACK, col);
        cx += tag.len() as i32 + 1;
    }
    let (mode, mcol) = if g.realtime {
        (
            " REAL-TIME ",
            Rgb(255, 80, 60).lerp(Rgb(255, 200, 120), 0.5 + 0.5 * (t * 5.0).sin()),
        )
    } else {
        (" TURN-BASED ", Rgb(120, 190, 255))
    };
    let view = if g.overhead { " MAP " } else { " 3D " };
    let tag_x = w - mode.len() as i32 - view.len() as i32 - 3;
    if g.difficulty > 0 {
        let dd = g.diff();
        let tag = format!(" {} ", dd.name.to_uppercase());
        c.text_bg(tag_x - tag.len() as i32 - 1, y0, &tag, Rgb::BLACK, dd.color);
    }
    c.text_bg(tag_x, y0, mode, Rgb::BLACK, mcol);
    c.text_bg(
        tag_x + mode.len() as i32 + 1,
        y0,
        view,
        Rgb::BLACK,
        Rgb(180, 170, 140),
    );

    // row 1: vitals
    let y = y0 + 1;
    let hpf = g.p.hp as f32 / g.p.max_hp.max(1) as f32;
    let hpc = Rgb(220, 40, 40).lerp(Rgb(60, 220, 60), hpf);
    c.text(1, y, "HP", Rgb(255, 120, 120));
    bar(c, 4, y, 20, hpf, hpc, Rgb(60, 30, 30));
    c.text(25, y, &format!("{}/{}", g.p.hp, g.p.max_hp), hpc);
    c.text(34, y, "MP", Rgb(120, 150, 255));
    bar(
        c,
        37,
        y,
        14,
        g.p.mp as f32 / g.p.max_mp.max(1) as f32,
        Rgb(70, 110, 255),
        Rgb(30, 30, 60),
    );
    c.text(
        52,
        y,
        &format!("{}/{}", g.p.mp, g.p.max_mp),
        Rgb(120, 150, 255),
    );
    let need = Game::xp_needed(g.p.level);
    let prev = if g.p.level > 1 {
        Game::xp_needed(g.p.level - 1)
    } else {
        0
    };
    let xpf = (g.p.xp - prev) as f32 / (need - prev).max(1) as f32;
    c.text(60, y, "XP", Rgb(255, 220, 100));
    bar(c, 63, y, 10, xpf, Rgb(255, 210, 80), Rgb(60, 50, 20));
    c.text(74, y, &format!("{}/{}", g.p.xp, need), Rgb(220, 200, 120));
    let mut sx = 86;
    if g.p.poison > 0 && sx < w - 10 {
        c.text(sx, y, "POISONED", Rgb(120, 255, 80));
        sx += 9;
    }
    if g.p.ward > 0 && sx < w - 10 {
        c.text(sx, y, &format!("WARD {}", g.p.ward), Rgb(140, 180, 255));
        sx += 9;
    }
    if g.p.invis > 0 && sx < w - 10 {
        c.text(sx, y, "HIDDEN", Rgb(150, 150, 220));
        sx += 7;
    }
    if g.p.vision > 0 && sx < w - 10 {
        c.text(sx, y, &format!("VISION {}", g.p.vision), Rgb(255, 120, 100));
        sx += 10;
    }
    if g.p.fury > 0 && sx < w - 8 {
        c.text(
            sx,
            y,
            &format!("FURY {}", g.p.fury),
            Rgb(255, 110, 60).lerp(Rgb(255, 220, 120), 0.5 + 0.5 * (t * 6.0).sin()),
        );
        sx += 8;
    }
    if g.p.rooted > 0 && sx < w - 10 {
        c.text(sx, y, &format!("HELD {}", g.p.rooted), Rgb(230, 220, 170));
        sx += 8;
    }
    if g.p.blind > 0 && sx < w - 10 {
        c.text(sx, y, &format!("BLIND {}", g.p.blind), Rgb(190, 170, 130));
        sx += 9;
    }
    if g.p.torch_out && sx < w - 18 {
        let col = Rgb(200, 200, 220).lerp(Rgb(255, 255, 255), 0.5 + 0.5 * (t * 3.0).sin());
        c.text(
            sx,
            y,
            if g.cursed(Curse::Gale) {
                "TORCH OUT"
            } else {
                "TORCH OUT (T)"
            },
            col,
        );
        sx += 14;
    }
    if g.last_stand() && sx < w - 11 {
        c.text(sx, y, "LAST STAND", Rgb(255, 60, 60));
    }

    // row 2: gear
    let y = y0 + 2;
    let wd = g.wdef();
    let wname = gear_label(wd.name, &g.p.wpn);
    let b = g.dmg_bonus();
    c.text(
        1,
        y,
        &format!(
            "Wpn: {} ({}-{}{})",
            wname,
            wd.dmg.0 + b,
            wd.dmg.1 + b,
            if wd.range > 0 { " ranged" } else { "" }
        ),
        Rgb(200, 200, 210),
    );
    let aname = gear_label(ARMORS[g.p.arm.def].name, &g.p.arm);
    c.text(
        42,
        y,
        &format!("Arm: {} [AC {}]", aname, g.player_ac()),
        Rgb(180, 170, 150),
    );
    let pots =
        g.p.inv
            .iter()
            .filter(|i| **i == Item::Potion(Potion::Healing))
            .count();
    c.text(76, y, &format!("Heal:{}", pots), Rgb(255, 100, 100));
    c.text(
        85,
        y,
        &format!("Kills:{} Turn:{}", g.kills, g.turns),
        Rgb(150, 150, 150),
    );
    if g.p.has_amulet {
        let col = Rgb(255, 200, 40).lerp(Rgb::WHITE, 0.5 + 0.5 * (t * 4.0).sin());
        c.text_bg(w - 11, y, " AMULET ", Rgb::BLACK, col);
    }

    // row 3: skills
    let y = y0 + 3;
    let mut x = 1;
    for (i, s) in g.skills().iter().enumerate() {
        let wait = g.skill_ready_in(*s);
        let cost = g.skill_cost(*s);
        let can = wait <= 0.0 && g.p.mp >= cost;
        let label = if wait > 0.0 {
            format!("[{}] {} ({})", i + 1, s.name(), wait)
        } else if cost > 0 {
            format!("[{}] {} {}mp", i + 1, s.name(), cost)
        } else {
            format!("[{}] {}", i + 1, s.name())
        };
        let col = if can {
            Rgb(160, 255, 160)
        } else {
            Rgb(100, 100, 100)
        };
        if x + label.len() as i32 > w - 2 {
            break;
        }
        c.text(x, y, &label, col);
        x += label.len() as i32 + 2;
    }
    if x + 14 < w {
        c.text(x, y, "[P] quaff heal", Rgb(140, 120, 120));
    }

    // rows 4..6: messages
    let n = g.msgs.len();
    for k in 0..3 {
        if n < 1 + k {
            break;
        }
        let m = &g.msgs[n - 1 - k];
        let y = y0 + HUD_H - 1 - k as i32;
        let fade = if k == 0 {
            1.0
        } else if k == 1 {
            0.7
        } else {
            0.5
        };
        let fresh = if m.age < 0.4 {
            0.3 * (1.0 - m.age / 0.4)
        } else {
            0.0
        };
        let text: String = m.text.chars().take((w - 2).max(0) as usize).collect();
        c.text(1, y, &text, m.color.scale(fade).lerp(Rgb::WHITE, fresh));
    }
}

// ================================================================ overlays

pub fn draw_inventory(c: &mut Canvas, g: &Game) {
    // Wider with Appraiser's Eye, to fit what better gear would add.
    let w = if g.bonus.appraise { 76 } else { 60 }.min(c.w as i32 - 4);
    let worn = g.equipped_uniques();
    let worn_rows = if worn.is_empty() {
        0
    } else {
        worn.len() as i32 + 2
    };
    let slots = g.inv_slots();
    let h = (slots.len() as i32 + 7 + worn_rows)
        .max(10)
        .min(c.h as i32 - 2);
    let x0 = (c.w as i32 - w) / 2;
    let y0 = (c.h as i32 - h) / 2;
    c.frame(x0, y0, w, h, Rgb(180, 150, 100), Rgb(16, 12, 10));
    c.text(x0 + 2, y0, " INVENTORY ", Rgb(255, 220, 150));
    if g.p.inv.is_empty() {
        c.text(x0 + 3, y0 + 2, "Your pack is empty.", Rgb(150, 150, 150));
    }
    for (i, &(first, count)) in slots.iter().enumerate() {
        let it = &g.p.inv[first];
        let y = y0 + 2 + i as i32;
        if y >= y0 + h - 3 {
            break;
        }
        let letter = (b'a' + i as u8) as char;
        let extra = match it {
            Item::Weapon(gr) => {
                let d = &WEAPONS[gr.def];
                format!(
                    "  {}-{}{}",
                    d.dmg.0 + gr.bonus,
                    d.dmg.1 + gr.bonus,
                    if d.range > 0 { " ranged" } else { "" }
                )
            }
            Item::Armor(gr) => format!("  AC {}", ARMORS[gr.def].ac + gr.bonus),
            Item::Trinket(u) => match UNIQUES[*u as usize].slot {
                USlot::Ring => "  (ring)".to_string(),
                _ => "  (amulet)".to_string(),
            },
            _ => String::new(),
        };
        c.text(x0 + 3, y, &format!("{}) ", letter), Rgb(255, 230, 150));
        c.text(x0 + 6, y, &it.glyph().to_string(), it.color());
        let name_col = if it.unique().is_some() {
            it.color()
        } else {
            Rgb(220, 220, 220)
        };
        let stack = if count > 1 { format!(" x{}", count) } else { String::new() };
        let line = format!("{}{}{}", it.name(), stack, extra);
        c.text(x0 + 8, y, &line, name_col);
        // Appraiser's Eye: a green ^ and what it adds over your own gear.
        if g.bonus.appraise {
            if let Some((a, b)) = g.upgrade_over_worn(it) {
                let green = Rgb(90, 230, 90);
                c.text(x0 + 2, y, "^", green);
                let gain = match it {
                    Item::Weapon(_) => format!("  ({:+} min / {:+} max dmg)", a, b),
                    _ => format!("  ({:+} AC)", a),
                };
                let gx = x0 + 8 + line.chars().count() as i32;
                let room = (x0 + w - 1 - gx).max(0) as usize;
                let gain: String = gain.chars().take(room).collect();
                c.text(gx, y, &gain, green);
            }
        }
    }
    if !worn.is_empty() {
        let mut y = y0 + h - 3 - worn.len() as i32;
        c.text(x0 + 3, y - 1, "UNIQUE POWERS", Rgb(220, 160, 255));
        for u in worn {
            let d = &UNIQUES[u];
            let line: String = format!("{}: {}", d.name, d.desc)
                .chars()
                .take((w - 6).max(0) as usize)
                .collect();
            c.text(x0 + 3, y, &line, d.color);
            y += 1;
        }
    }
    c.text(
        x0 + 2,
        y0 + h - 2,
        &format!(
            "a-r: use/equip   SHIFT+letter: drop   `: sort   [{} free]",
            g.free_slots()
        ),
        Rgb(150, 140, 120),
    );
}

pub const HELP_LINES: &[&str] = &[
    "THE QUEST: Descend to depth 10, claim the Amulet of GRAZZARK from its",
    "Warden, then climb all the way back to the surface alive.",
    "",
    "FIRST-PERSON VIEW            OVERHEAD VIEW (Shift+M)",
    "  W / Up      step forward     W A S D / arrows   move N W S E",
    "  S / Down    step back        Y U B N / Home PgUp End PgDn  diagonals",
    "  A D / <- -> turn 45 deg      Q / E              turn",
    "  Q / E       strafe",
    "  Z / C       diagonal step",
    "",
    "ACTIONS",
    "  Space / F   attack ahead (or shoot)    G or ,   pick up",
    "  1 2         class skills               3-6      altar skills",
    "  > < Enter   use stairs                 . or 5   wait / search",
    "  I           inventory                  P        quaff healing potion",
    "  Shift+R     toggle TURN-BASED / REAL-TIME combat",
    "  Shift+M     switch between first-person and overhead play",
    "  Shift+C     Last Rites on / off (a Soul Altar boon)",
    "  Shift+Q     listen intently (Keen Hearing): beside a chest, a Mimic creaks",
    "  Tab or M    show the full map of the floor",
    "  L           message log (last 100 turns)  T  relight a blown-out torch",
    "  - / +       volume                     Esc      menu",
    "",
    "Monsters fight each other, and victors may become glowing CHAMPIONS.",
    "Each floor may hide a unique monster guarding its signature treasure.",
    "The dead rise again over time. Listen:",
    "sounds bend around corners, so echoes tell you where danger lies.",
    "Dying earns Soul Points: spend them at the Soul Altar for permanent power.",
];

pub fn draw_help(c: &mut Canvas) {
    let w = 78.min(c.w as i32 - 2);
    let h = (HELP_LINES.len() as i32 + 4).min(c.h as i32 - 2);
    let x0 = (c.w as i32 - w) / 2;
    let y0 = (c.h as i32 - h) / 2;
    c.frame(x0, y0, w, h, Rgb(120, 160, 200), Rgb(10, 12, 18));
    c.text(x0 + 2, y0, " HOW TO PLAY ", Rgb(180, 220, 255));
    for (i, l) in HELP_LINES.iter().enumerate() {
        let y = y0 + 2 + i as i32;
        if y >= y0 + h - 1 {
            break;
        }
        let col = if l
            .chars()
            .all(|ch| ch.is_uppercase() || ch == ' ' || ch == '(' || ch == ')' || ch == '-')
            && !l.trim().is_empty()
        {
            Rgb(255, 210, 120)
        } else {
            Rgb(200, 200, 200)
        };
        let s: String = l.chars().take((w - 4) as usize).collect();
        c.text(x0 + 2, y, &s, col);
    }
}

pub const PAUSE_ITEMS: [&str; 5] = [
    "Resume",
    "How to play",
    "Detail",
    "Abandon run (earn soul points)",
    "Save and quit to desktop",
];

/// `detail` is the current detail setting, shown beside its menu item.
pub fn draw_pause(c: &mut Canvas, sel: usize, detail: &str) {
    let w = 40;
    let h = 10;
    let x0 = (c.w as i32 - w) / 2;
    let y0 = (c.h as i32 - h) / 2;
    c.frame(x0, y0, w, h, Rgb(200, 120, 90), Rgb(16, 8, 8));
    c.text(x0 + 2, y0, " PAUSED ", Rgb(255, 180, 140));
    for (i, it) in PAUSE_ITEMS.iter().enumerate() {
        let y = y0 + 2 + i as i32;
        let label;
        let it = if *it == "Detail" {
            label = format!("Detail: < {} >", detail);
            label.as_str()
        } else {
            it
        };
        if i == sel {
            c.text_bg(
                x0 + 2,
                y,
                &format!("> {:<34}", it),
                Rgb::BLACK,
                Rgb(255, 180, 120),
            );
        } else {
            c.text(x0 + 4, y, it, Rgb(200, 190, 180));
        }
    }
}

// ================================================================ raise menu

/// Choose a corpse to raise: its max HP upkeep, and whatever the bestiary
/// has taught you about it.
pub fn draw_raise(c: &mut Canvas, g: &Game, sel: usize, prof: &crate::profile::Profile) {
    let opts = &g.raise_menu;
    let w = (c.w as i32 - 4).min(84).max(30);
    let per = 4;
    let h = (opts.len() as i32 * per + 5).min(c.h as i32 - 2).max(8);
    let x0 = (c.w as i32 - w) / 2;
    let y0 = (c.h as i32 - h) / 2;
    c.frame(x0, y0, w, h, Rgb(120, 200, 130), Rgb(8, 14, 9));
    c.text(
        x0 + 2,
        y0,
        " RAISE DEAD: which body answers your call? ",
        Rgb(170, 255, 170),
    );
    let rows = ((h - 4) / per).max(1) as usize;
    let start = if sel >= rows { sel + 1 - rows } else { 0 };
    let tw = (w - 8) as usize;
    for (row, i) in (start..opts.len()).enumerate() {
        if row >= rows {
            break;
        }
        let o = opts[i];
        let m = &MONSTERS[o.kind];
        let known = prof.kills_of(m.name) + g.slain.get(m.name).copied().unwrap_or(0);
        let lt = m.lore();
        let y = y0 + 2 + row as i32 * per;
        let letter = (b'a' + i as u8) as char;
        let again = if o.again { "  (raised again)" } else { "" };
        let head = format!("{}) {}{}", letter, cap(m.name), again);
        let cost = format!("-{} max HP", o.cost);
        if i == sel {
            c.fill(x0 + 1, y, w - 2, 1, ' ', Rgb::BLACK, Rgb(120, 200, 130));
            c.text_bg(x0 + 2, y, &head, Rgb::BLACK, Rgb(120, 200, 130));
            c.text_bg(
                x0 + w - 3 - cost.len() as i32,
                y,
                &cost,
                Rgb(90, 0, 0),
                Rgb(120, 200, 130),
            );
        } else {
            c.text(x0 + 2, y, &head, m.color.lerp(Rgb::WHITE, 0.3));
            c.text(x0 + w - 3 - cost.len() as i32, y, &cost, Rgb(255, 110, 110));
        }
        // Only what the bestiary has taught you.
        let mut lines: Vec<String> = Vec::new();
        if known >= lt.basic {
            lines.push(format!(
                "Health {}  Damage {}-{}  Armour {}  Speed: {}",
                m.hp,
                m.dmg.0,
                m.dmg.1,
                m.ac,
                crate::screens::speed_word(m.speed)
            ));
        }
        if known >= lt.adv {
            let mut traits: Vec<String> = Vec::new();
            if let Some(r) = &m.ranged {
                traits.push(r.verb.to_string());
            }
            if m.poison {
                traits.push("poisonous".into());
            }
            if m.regen {
                traits.push("regenerates".into());
            }
            if m.ghost {
                traits.push("passes through walls".into());
            }
            for a in m.abil.iter() {
                traits.push(a.describe());
            }
            if !traits.is_empty() {
                lines.push(format!("Traits: {}", traits.join(", ")));
            }
        }
        for (k, l) in lines.iter().take((per - 1) as usize).enumerate() {
            let l: String = l.chars().take(tw).collect();
            c.text(x0 + 5, y + 1 + k as i32, &l, Rgb(170, 200, 170));
        }
    }
    c.text(
        x0 + 2,
        y0 + h - 1,
        " letter or Enter: raise   Up/Down: choose   Esc: cancel ",
        Rgb(140, 180, 140),
    );
}

// ================================================================ message log (L)

/// Every message from the last 100 turns, newest at the bottom.
pub fn draw_log(c: &mut Canvas, g: &Game, scroll: usize) {
    let w = (c.w as i32 - 4).min(110).max(20);
    let h = (c.h as i32 - 2).max(6);
    let x0 = (c.w as i32 - w) / 2;
    let y0 = (c.h as i32 - h) / 2;
    c.frame(x0, y0, w, h, Rgb(150, 140, 110), Rgb(10, 10, 8));
    c.text(
        x0 + 2,
        y0,
        &format!(" MESSAGE LOG: the last {} turns ", LOG_TURNS),
        Rgb(230, 210, 150),
    );
    let tw = (w - 12).max(8) as usize;
    let cutoff = g.turns.saturating_sub(LOG_TURNS);
    // Lay every message out as wrapped lines.
    let mut lines: Vec<(Option<u64>, String, Rgb)> = Vec::new();
    for m in g.msgs.iter().filter(|m| m.turn >= cutoff) {
        for (i, l) in crate::screens::wrap(&m.text, tw).into_iter().enumerate() {
            lines.push((if i == 0 { Some(m.turn) } else { None }, l, m.color));
        }
    }
    let rows = (h - 4).max(1) as usize;
    let max_scroll = lines.len().saturating_sub(rows);
    let scroll = scroll.min(max_scroll);
    let end = lines.len() - scroll;
    let start = end.saturating_sub(rows);
    if lines.is_empty() {
        c.text(x0 + 3, y0 + 2, "Nothing has happened yet.", GREY);
    }
    for (row, (turn, text, col)) in lines[start..end].iter().enumerate() {
        let y = y0 + 2 + row as i32;
        if let Some(t) = turn {
            c.text(
                x0 + 2,
                y,
                &format!("{:>6}", format!("T{}", t)),
                Rgb(110, 105, 90),
            );
        }
        c.text(x0 + 10, y, text, *col);
    }
    let hint = if max_scroll > 0 {
        format!(
            " Up/Down, PgUp/PgDn: scroll ({} more above)   L or Esc: close ",
            start
        )
    } else {
        " L or Esc: close ".to_string()
    };
    let hint: String = hint.chars().take((w - 4) as usize).collect();
    c.text(x0 + 2, y0 + h - 1, &hint, Rgb(150, 140, 120));
}

// ================================================================ full map (M)

/// The whole floor as a plain map: explored tiles, stairs, what you can see,
/// and you. No lighting or effects, like the minimap.
pub fn draw_map(c: &mut Canvas, g: &Game) {
    let lv = &g.level;
    let (cw, ch) = (c.w as i32, c.h as i32);
    let bg = Rgb(8, 8, 12);
    c.fill(0, 0, cw, ch, ' ', Rgb::WHITE, Rgb(4, 4, 7));
    if g.p.torch_out {
        c.center(
            ch / 2 - 1,
            "It is far too dark to read your map.",
            Rgb(170, 170, 190),
        );
        c.center(
            ch / 2 + 1,
            "Relight your torch first (T).",
            Rgb(120, 120, 140),
        );
        return;
    }
    // Two columns per tile when there is room, so the map is not squashed.
    let sx = if cw - 4 >= lv.w * 2 { 2 } else { 1 };
    let vw = (lv.w * sx).min(cw - 4);
    let vh = lv.h.min(ch - 5);
    let x0 = (cw - vw) / 2;
    let y0 = 2 + (ch - 5 - vh) / 2;
    // Scroll to keep the player in view on small terminals.
    let ox = (g.p.x - vw / sx / 2).clamp(0, (lv.w - vw / sx).max(0));
    let oy = (g.p.y - vh / 2).clamp(0, (lv.h - vh).max(0));
    c.frame(x0 - 1, y0 - 1, vw + 2, vh + 2, Rgb(70, 60, 80), bg);
    let name = g.floor_name();
    c.center(
        0,
        &format!("MAP: Depth {}, {}", g.depth, name),
        Rgb(220, 200, 150),
    );
    for j in 0..vh {
        for i in 0..vw / sx {
            let (tx, ty) = (ox + i, oy + j);
            if !lv.inb(tx, ty) || !lv.explored[lv.idx(tx, ty)] {
                continue;
            }
            let vis = lv.visible[lv.idx(tx, ty)];
            let k = if vis { 1.0 } else { 0.55 };
            let (chr, col) = match lv.tile(tx, ty) {
                Tile::Wall => ('#', Rgb(110, 100, 90)),
                Tile::Floor => ('.', Rgb(80, 80, 80)),
                Tile::DoorClosed => ('+', Rgb(170, 120, 60)),
                Tile::DoorOpen => ('\'', Rgb(170, 120, 60)),
                Tile::StairsUp => ('<', Rgb(120, 180, 255)),
                Tile::StairsDown => ('>', Rgb(220, 120, 255)),
                Tile::Sinew => ('%', Rgb(210, 50, 60)),
                Tile::GateClosed => ('#', Rgb(150, 150, 160)),
                Tile::Ward => ('^', Rgb(200, 160, 255)),
                Tile::Crystal => ('#', Rgb(120, 230, 255)),
                Tile::Chest => ('=', chest_color(lv, tx, ty)),
            };
            for s in 0..sx {
                let ch2 = if s == 0 || chr == '#' { chr } else { ' ' };
                c.set(x0 + i * sx + s, y0 + j, ch2, col.scale(k), bg);
            }
        }
    }
    let to_scr = |x: i32, y: i32| -> Option<(i32, i32)> {
        let (i, j) = (x - ox, y - oy);
        if i >= 0 && i < vw / sx && j >= 0 && j < vh {
            Some((x0 + i * sx, y0 + j))
        } else {
            None
        }
    };
    for tr in lv.traps.iter().filter(|tr| tr.revealed) {
        if lv.explored[lv.idx(tr.x, tr.y)] {
            if let Some((x, y)) = to_scr(tr.x, tr.y) {
                c.set(x, y, '^', trap_color(tr.kind), bg);
            }
        }
    }
    for it in &lv.items {
        if lv.is_visible(it.x, it.y) {
            if let Some((x, y)) = to_scr(it.x, it.y) {
                c.set(x, y, it.item.glyph(), it.item.color(), bg);
            }
        }
    }
    for m in &lv.monsters {
        if map_shows(g, m) {
            if let Some((x, y)) = to_scr(m.x, m.y) {
                c.set(x, y, m.def().glyph, map_mon_color(m), bg);
            }
        }
    }
    if let Some((x, y)) = to_scr(g.p.x, g.p.y) {
        c.set(x, y, '@', Rgb(255, 255, 120), Rgb(60, 50, 0));
        if sx == 2 {
            c.set(
                x + 1,
                y,
                ARROWS[g.p.facing as usize],
                Rgb(255, 220, 90),
                Rgb(60, 50, 0),
            );
        }
    }
    c.center(
        ch - 2,
        "@ you   < up   > down   + door   ^ trap   red: foe   gold: champion   violet: unique",
        Rgb(150, 140, 120),
    );
    c.center(ch - 1, "M / Esc: close", Rgb(120, 120, 140));
}

// ================================================================ pickup menu

pub fn draw_pickup(c: &mut Canvas, g: &Game) {
    let here = g.items_here();
    let w = 56.min(c.w as i32 - 4);
    let h = (here.len() as i32 + 6).min(c.h as i32 - 2);
    let x0 = (c.w as i32 - w) / 2;
    let y0 = (c.h as i32 - h) / 2;
    c.frame(x0, y0, w, h, Rgb(200, 180, 110), Rgb(16, 14, 8));
    c.text(x0 + 2, y0, " PICK UP ", Rgb(255, 230, 150));
    for (k, &ii) in here.iter().enumerate() {
        let y = y0 + 2 + k as i32;
        if y >= y0 + h - 2 || k >= 26 {
            break;
        }
        let it = &g.level.items[ii].item;
        let letter = (b'a' + k as u8) as char;
        c.text(x0 + 3, y, &format!("{}) ", letter), Rgb(255, 230, 150));
        c.text(x0 + 6, y, &it.glyph().to_string(), it.color());
        let col = if it.unique().is_some() {
            it.color()
        } else {
            Rgb(220, 220, 220)
        };
        let name: String = it.name().chars().take((w - 10).max(0) as usize).collect();
        c.text(x0 + 8, y, &name, col);
    }
    let free = g.free_slots();
    c.text(
        x0 + 2,
        y0 + h - 2,
        &format!(
            "letter: take one   Enter/g: take all (1 turn)   [{} free]",
            free
        ),
        Rgb(150, 140, 120),
    );
}

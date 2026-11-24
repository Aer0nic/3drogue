//! Effects drawn over the whole play window rather than in the world: a web
//! stuck across your view, gas jetting up from a trap, flames whooshing
//! round you, spores drifting before your eyes.
//!
//! Everything here is worked out from the effect's age and a seed, so it
//! needs no state beyond what `Fx` and the player already keep.

use crate::canvas::Canvas;
use crate::color::{hash2, hashf, vnoise, Rgb};
use crate::fx::{ScreenFx, ScreenKind, WEB_FALL};
use crate::game::Game;
use std::f32::consts::{PI, TAU};

/// The play window in cells, and the same in "square" units (a cell is
/// about half as wide as it is tall, so x counts half).
#[derive(Clone, Copy)]
struct View {
    vx: i32,
    vy: i32,
    vw: i32,
    vh: i32,
}

impl View {
    fn w(&self) -> f32 {
        self.vw as f32 * 0.5
    }
    fn h(&self) -> f32 {
        self.vh as f32
    }
    /// Square-unit position of a cell's centre.
    fn sq(&self, x: i32, y: i32) -> (f32, f32) {
        ((x - self.vx) as f32 * 0.5 + 0.25, (y - self.vy) as f32 + 0.5)
    }
    /// The cell holding a square-unit point.
    fn cell(&self, sx: f32, sy: f32) -> (i32, i32) {
        (self.vx + (sx * 2.0).floor() as i32, self.vy + sy.floor() as i32)
    }
    fn inside(&self, x: i32, y: i32) -> bool {
        x >= self.vx && x < self.vx + self.vw && y >= self.vy && y < self.vy + self.vh
    }
}

/// A random number in 0..1 fixed by a seed and an index.
fn rnd(seed: u32, i: i32) -> f32 {
    hashf(seed as i32, i.wrapping_mul(7919))
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Comes up over the first `rise` of its life and dies away over the last `fall`.
fn envelope(k: f32, rise: f32, fall: f32) -> f32 {
    (k / rise).min(1.0) * ((1.0 - k) / fall).min(1.0)
}

pub fn screen_overlays(c: &mut Canvas, g: &Game, vx: i32, vy: i32, vw: i32, vh: i32, t: f32) {
    if vw < 8 || vh < 4 {
        return;
    }
    let v = View { vx, vy, vw, vh };
    for s in &g.fx.screen {
        match s.kind {
            ScreenKind::Gas => gas(c, v, s, false),
            ScreenKind::Spores => gas(c, v, s, true),
            ScreenKind::Fire => fire(c, v, s),
            ScreenKind::Spikes => spikes(c, v, s),
            ScreenKind::Maw => maw(c, v, s),
            ScreenKind::Alarm => alarm(c, v, s),
            ScreenKind::Frost => frost(c, v, s),
        }
    }
    if g.p.seized > 0 {
        seized(c, v, g.p.seized, t);
    }
    web(c, g, v, t);
}

// ================================================================ the web

/// One strand of a web: a straight run between two points (square units),
/// with the point in its tearing-free (0..1) at which it snaps.
struct Strand {
    a: (f32, f32),
    b: (f32, f32),
    key: f32,
    /// How far out from its corner it reaches (0..1), so a fresh web can
    /// spread outward from the corners.
    reach: f32,
}

const SPOKES: usize = 7;
const RINGS: usize = 7;

/// The webs strung across the corners of the view.
fn web_strands(v: View, seed: u32) -> Vec<Strand> {
    let (w, h) = (v.w(), v.h());
    let mut out = Vec::new();
    // Each corner, and the quarter it fans into.
    let corners = [
        ((0.0, 0.0), 0.0f32),
        ((w, 0.0), PI * 0.5),
        ((w, h), PI),
        ((0.0, h), PI * 1.5),
    ];
    let big = w.min(h * 1.4);
    for (ci, &((cx, cy), a0)) in corners.iter().enumerate() {
        let s = seed.wrapping_add(ci as u32 * 977);
        // The top corners hang bigger webs than the bottom ones.
        let r_max = big * (0.46 + 0.12 * rnd(s, 1)) * if ci < 2 { 1.0 } else { 0.8 };
        let ang: Vec<f32> = (0..SPOKES)
            .map(|i| {
                let f = (i as f32 + 0.15 + 0.7 * rnd(s, 10 + i as i32)) / SPOKES as f32;
                a0 + f * PI * 0.5
            })
            .collect();
        let len: Vec<f32> =
            (0..SPOKES).map(|i| r_max * (0.8 + 0.2 * rnd(s, 20 + i as i32))).collect();
        let radius = |k: usize| (k as f32 + 1.0) / RINGS as f32;
        let point = |i: usize, f: f32| {
            let r = len[i] * f;
            (cx + ang[i].cos() * r, cy + ang[i].sin() * r)
        };
        // Spokes, in pieces between the rings so each piece can snap alone.
        // They are the strongest strands, so they tend to go last.
        for i in 0..SPOKES {
            let mut prev = 0.0;
            for k in 0..RINGS {
                let f = radius(k);
                let key = rnd(s, 100 + (i * RINGS + k) as i32) * 0.9;
                out.push(Strand { a: point(i, prev), b: point(i, f), key, reach: f });
                prev = f;
            }
        }
        // The spiral: rings joining the spokes, sagging toward the corner.
        for k in 0..RINGS {
            let f = radius(k) * (0.92 + 0.06 * rnd(s, 300 + k as i32));
            for i in 0..SPOKES - 1 {
                let (p, q) = (point(i, f), point(i + 1, f));
                let key = rnd(s, 400 + (i * RINGS + k) as i32);
                let mid = ((p.0 + q.0) * 0.5, (p.1 + q.1) * 0.5);
                let pull = 0.12;
                let sag = (mid.0 + (cx - mid.0) * pull, mid.1 + (cy - mid.1) * pull);
                out.push(Strand { a: p, b: sag, key, reach: f });
                out.push(Strand { a: sag, b: q, key, reach: f });
            }
        }
        // A couple of loose threads trailing in toward the middle.
        for j in 0..2 {
            let i = (rnd(s, 500 + j) * SPOKES as f32) as usize % SPOKES;
            let a = point(i, 1.0);
            let drift = 0.2 + 0.15 * rnd(s, 510 + j);
            let b = (a.0 + (w * 0.5 - a.0) * drift, a.1 + (h * 0.5 - a.1) * drift);
            out.push(Strand { a, b, key: rnd(s, 520 + j) * 0.6 + 0.4, reach: 1.0 });
        }
    }
    out
}

/// The glyph that draws a line running in this direction (square units).
fn line_glyph(dx: f32, dy: f32) -> char {
    let a = dy.atan2(dx).rem_euclid(PI);
    if !(PI / 8.0..PI * 7.0 / 8.0).contains(&a) {
        '-'
    } else if a < PI * 3.0 / 8.0 {
        '\\'
    } else if a < PI * 5.0 / 8.0 {
        '|'
    } else {
        '/'
    }
}

/// How one strand is drawn.
struct Look<'a> {
    /// Draw only this much of it (0..1), for a web still spreading.
    upto: f32,
    /// 0..1: how frayed it is (gaps and wisps).
    fray: f32,
    col: Rgb,
    seed: i32,
    /// Moves each point (for the shudder and for falling).
    warp: &'a dyn Fn(f32, f32, f32) -> (f32, f32),
    /// A torn, falling thread: drawn as curls of string.
    loose: bool,
}

fn draw_strand(c: &mut Canvas, v: View, st: &Strand, look: &Look) {
    let (dx, dy) = (st.b.0 - st.a.0, st.b.1 - st.a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.01 {
        return;
    }
    let n = (len / 0.3).ceil() as i32;
    let glyph = line_glyph(dx, dy);
    let mut last = (i32::MIN, i32::MIN);
    for i in 0..=n {
        let f = i as f32 / n as f32;
        if f > look.upto {
            break;
        }
        let (px, py) = (look.warp)(st.a.0 + dx * f, st.a.1 + dy * f, f);
        let (x, y) = v.cell(px, py);
        if (x, y) == last || !v.inside(x, y) {
            continue;
        }
        last = (x, y);
        let h = hashf(look.seed.wrapping_add(i), 17);
        // A fraying strand thins out and curls.
        if look.fray > 0.0 && h < look.fray * 0.55 {
            continue;
        }
        let ch = if look.loose {
            ['~', '(', ')', '\'', '`', ',', 's', '~'][(h * 8.0) as usize % 8]
        } else if look.fray > 0.0 && h < look.fray * 0.85 {
            [',', '`', '\'', '~', '.'][(h * 13.0) as usize % 5]
        } else {
            glyph
        };
        let cell = c.get(x, y);
        let k = 0.85 - 0.35 * look.fray + 0.15 * h;
        c.set(x, y, ch, look.col.scale(k), cell.bg.lerp(look.col, 0.07));
    }
}

fn web(c: &mut Canvas, g: &Game, v: View, t: f32) {
    let held = g.p.rooted > 0 && g.p.web_max > 0 && g.web_flight <= 0.0;
    if !held && g.fx.web_falls.is_empty() {
        return;
    }
    let strands = web_strands(v, g.fx.web_seed);
    let silk = Rgb(228, 228, 220);
    let jolt = g.fx.web_jolt;
    if held {
        let frac = (g.p.rooted as f32 / g.p.web_max.max(1) as f32).min(1.0);
        // A fresh web spreads out from the corners in a blink.
        let spread = (g.fx.web_age / 0.3).min(1.0);
        let shudder = move |x: f32, y: f32, f: f32| {
            let q = jolt * jolt * (t * 38.0 + x * 0.4).sin() * 0.9;
            (x, y + q * (0.4 + f * 0.6))
        };
        let step = 1.0 / RINGS as f32;
        for (i, st) in strands.iter().enumerate() {
            if st.key >= frac {
                continue;
            }
            let upto = if spread >= 1.0 {
                1.0
            } else if st.reach - step > spread {
                continue;
            } else {
                ((spread - (st.reach - step)) / step).clamp(0.0, 1.0)
            };
            // The next strands to go are already fraying.
            let fray = smooth(frac - 0.3, frac, st.key);
            let look = Look { upto, fray, col: silk, seed: i as i32 * 131, warp: &shudder, loose: false };
            draw_strand(c, v, st, &look);
        }
    }
    // Torn strands go limp, curl into loose threads and drop out of view.
    let hgt = v.h();
    for fall in &g.fx.web_falls {
        for (i, st) in strands.iter().enumerate() {
            if st.key < fall.lo || st.key >= fall.hi {
                continue;
            }
            let delay = rnd(i as u32, 7) * 0.25;
            let a = (fall.age - delay).max(0.0);
            // Fast enough that even the top strands clear the bottom in time.
            let grav = 2.0 * (hgt + 4.0) / (WEB_FALL - 0.3).powi(2);
            let drop = 0.5 * grav * a * a;
            let sway = rnd(i as u32, 9) * TAU;
            let fade = 1.0 - (fall.age / WEB_FALL).powi(2);
            let warp = move |x: f32, y: f32, f: f32| {
                let curl = (f * 6.0 + a * 7.0 + sway).sin() * (0.5 + a * 2.0);
                (x + curl + (a * 2.0 + sway).sin() * a * 3.0, y + drop + curl * 0.3)
            };
            let look = Look {
                upto: 1.0,
                fray: 0.25,
                col: silk.scale(fade.max(0.3)),
                seed: i as i32 * 97,
                warp: &warp,
                loose: a > 0.0,
            };
            draw_strand(c, v, st, &look);
        }
    }
}

// ================================================================ traps

/// Gas (or a puff of spores) jetting up from the floor in billowing plumes,
/// then hanging and thinning away.
fn gas(c: &mut Canvas, v: View, s: &ScreenFx, spores: bool) {
    let k = s.k();
    let rise = smooth(0.0, 0.3, k);
    let fade = ((1.0 - k) / 0.45).min(1.0);
    let (ink, haze) = if spores {
        (Rgb(225, 170, 235), Rgb(80, 40, 90))
    } else {
        (Rgb(150, 235, 80), Rgb(40, 95, 20))
    };
    let plumes = if spores { 5 } else { 7 };
    let jets: Vec<(f32, f32, f32)> = (0..plumes)
        .map(|i| {
            let u = (i as f32 + 0.2 + 0.6 * rnd(s.seed, i)) / plumes as f32;
            let tall = 0.55 + 0.4 * rnd(s.seed, 50 + i);
            let phase = rnd(s.seed, 90 + i) * TAU;
            (u, tall, phase)
        })
        .collect();
    let age = s.age;
    for y in v.vy..v.vy + v.vh {
        let fy = (y - v.vy) as f32 / v.vh as f32;
        for x in v.vx..v.vx + v.vw {
            let fx = (x - v.vx) as f32 / v.vw as f32;
            let mut d: f32 = 0.0;
            for &(u, tall, phase) in &jets {
                let top = 1.0 - tall * rise;
                if fy < top {
                    continue;
                }
                // Wider as it climbs, like a mushrooming jet.
                let up = (1.0 - fy) / tall.max(0.1);
                let half = 0.03 + 0.10 * up * rise;
                let wob = (fy * 9.0 + age * 4.0 + phase).sin() * 0.025;
                let off = ((fx - u - wob) / half).abs();
                if off < 1.0 {
                    let cap = smooth(top, top + 0.1, fy);
                    d = d.max((1.0 - off * off) * cap);
                }
            }
            // A low haze settles over everything as the jets spend themselves.
            let pool = smooth(0.4, 1.0, fy) * smooth(0.2, 0.6, k) * 0.5;
            let n = vnoise(fx * 18.0, fy * 10.0 + age * 3.0);
            let d = (d.max(pool) * (0.55 + 0.6 * n)).min(1.0) * fade;
            if d < 0.12 {
                continue;
            }
            let cell = c.get(x, y);
            let h = hashf(x + (age * 9.0) as i32, y + (age * 14.0) as i32);
            let ch = if spores {
                ['.', '\'', 'o', '°', '*', '.'][(h * 6.0) as usize % 6]
            } else if d > 0.7 {
                '▒'
            } else if d > 0.4 {
                '░'
            } else {
                [':', '.', '~', '\''][(h * 4.0) as usize % 4]
            };
            let bg = cell.bg.lerp(haze, d * 0.55);
            if !spores || h < d * 0.7 {
                c.set(x, y, ch, ink.scale(0.6 + 0.4 * d), bg);
            } else {
                c.set(x, y, cell.ch, cell.fg.lerp(ink, d * 0.3), bg);
            }
        }
    }
}

/// Distance (square units) to the nearest edge of the view, where on the rim
/// that is (0..1 along the bottom, 1..2 up the right, 2..3 along the top,
/// 3..4 down the left), and which edge (bottom, right, top, left).
fn rim(v: View, x: i32, y: i32) -> (f32, f32, usize) {
    let (sx, sy) = v.sq(x, y);
    let (w, h) = (v.w(), v.h());
    let edges = [h - sy, w - sx, sy, sx];
    let mut e = 0;
    for i in 1..4 {
        if edges[i] < edges[e] {
            e = i;
        }
    }
    let along = match e {
        0 => sx / w,
        1 => 1.0 + (h - sy) / h,
        2 => 2.0 + (w - sx) / w,
        _ => 3.0 + sy / h,
    };
    (edges[e], along, e)
}

/// Flames roar up from the floor and sweep round the edges of the view.
fn fire(c: &mut Canvas, v: View, s: &ScreenFx) {
    let k = s.k();
    let env = envelope(k, 0.12, 0.45);
    let age = s.age;
    let h = v.h();
    // Two walls of flame racing round you in opposite directions.
    let start = rnd(s.seed, 1) * 4.0;
    let sweep = [(start + k * 4.5).rem_euclid(4.0), (start + 2.0 - k * 4.5).rem_euclid(4.0)];
    for y in v.vy..v.vy + v.vh {
        for x in v.vx..v.vx + v.vw {
            let (d, along, edge) = rim(v, x, y);
            let base = [0.34, 0.2, 0.1, 0.2][edge] * h;
            let flick = 0.55
                + 0.25 * (along * 23.0 + age * 13.0).sin()
                + 0.2 * (along * 41.0 - age * 19.0).sin();
            let mut bump: f32 = 0.0;
            for &p in &sweep {
                let dd = (along - p).abs();
                let dd = dd.min(4.0 - dd);
                bump = bump.max((-dd * dd / 0.04).exp());
            }
            let tall = (base * flick + bump * 0.3 * h) * env;
            // Tongues of flame licking upward.
            let d = d + hashf(x, y + (age * 24.0) as i32) * 1.6;
            if d >= tall || tall < 0.5 {
                continue;
            }
            let i = 1.0 - d / tall;
            let col = if i > 0.66 {
                Rgb(255, 150, 30).lerp(Rgb(255, 235, 140), (i - 0.66) * 3.0)
            } else if i > 0.33 {
                Rgb(220, 60, 10).lerp(Rgb(255, 150, 30), (i - 0.33) * 3.0)
            } else {
                Rgb(120, 25, 5).lerp(Rgb(220, 60, 10), i * 3.0)
            };
            let hh = hashf(x * 3 + (age * 15.0) as i32, y);
            let ch = if i > 0.75 {
                ['▓', '█', '▒'][(hh * 3.0) as usize % 3]
            } else if i > 0.4 {
                ['*', '^', '&', '%'][(hh * 4.0) as usize % 4]
            } else {
                ['\'', '`', '^', ',', '.'][(hh * 5.0) as usize % 5]
            };
            let cell = c.get(x, y);
            c.set(x, y, ch, col, cell.bg.lerp(Rgb(110, 30, 0), i * 0.7));
        }
    }
    c.tint_region(v.vx, v.vy, v.vw, v.vh, Rgb(255, 110, 20), 0.14 * env);
}

/// Steel spikes stab up from the bottom of the view, then sink back.
fn spikes(c: &mut Canvas, v: View, s: &ScreenFx) {
    let k = s.k();
    let ext = if k < 0.1 {
        smooth(0.0, 0.1, k)
    } else if k < 0.55 {
        1.0
    } else {
        1.0 - smooth(0.55, 1.0, k)
    };
    if ext <= 0.0 {
        return;
    }
    let (w, h) = (v.w(), v.h());
    let n = ((w / 7.0) as i32).max(5);
    let gap = w / n as f32;
    let row: Vec<(f32, f32, f32)> = (0..n)
        .map(|i| {
            let x = (i as f32 + 0.5 + (rnd(s.seed, i) - 0.5) * 0.4) * gap;
            let tall = h * (0.3 + 0.3 * rnd(s.seed, 40 + i)) * ext;
            let half = gap * (0.32 + 0.12 * rnd(s.seed, 80 + i));
            (x, tall, half)
        })
        .collect();
    for y in v.vy..v.vy + v.vh {
        for x in v.vx..v.vx + v.vw {
            let (sx, sy) = v.sq(x, y);
            let up = h - sy;
            for &(cx, tall, half) in &row {
                if up > tall {
                    continue;
                }
                let span = half * (1.0 - up / tall);
                let off = sx - cx;
                if off.abs() > span + 0.25 {
                    continue;
                }
                let edge = off.abs() > span - 0.5;
                let (ch, col) = if up > tall - 1.5 {
                    ('^', Rgb(200, 40, 35))
                } else if edge {
                    (if off < 0.0 { '/' } else { '\\' }, Rgb(200, 200, 210))
                } else if off < 0.0 {
                    ('▓', Rgb(165, 165, 178))
                } else {
                    ('▒', Rgb(105, 105, 118))
                };
                // Blood running down from the tips.
                let blood = up > tall * 0.6 && hashf(x, 77) < 0.3;
                let col = if blood { col.lerp(Rgb(170, 20, 20), 0.6) } else { col };
                let cell = c.get(x, y);
                c.set(x, y, ch, col, cell.bg.lerp(Rgb(30, 30, 36), 0.7));
                break;
            }
        }
    }
}

/// The floor's maw: rows of fangs snap shut from above and below, hold, and
/// slowly part.
fn maw(c: &mut Canvas, v: View, s: &ScreenFx) {
    let k = s.k();
    let bite = if k < 0.14 {
        smooth(0.0, 0.14, k)
    } else if k < 0.45 {
        1.0
    } else {
        1.0 - smooth(0.45, 1.0, k)
    };
    if bite <= 0.0 {
        return;
    }
    let (w, h) = (v.w(), v.h());
    let gum = h * 0.13 * bite;
    let gap = 4.5;
    let n = (w / gap).ceil() as i32 + 1;
    let fangs = |jaw: i32| -> Vec<(f32, f32, f32)> {
        (0..n)
            .map(|i| {
                let x = (i as f32 + if jaw == 0 { 0.0 } else { 0.5 }) * gap;
                let long = if (i + jaw) % 4 == 1 { 1.5 } else { 1.0 };
                let len = h * 0.2 * long * (0.85 + 0.3 * rnd(s.seed, jaw * 100 + i)) * bite;
                (x, len, gap * 0.42)
            })
            .collect()
    };
    let jaws = [fangs(0), fangs(1)];
    for y in v.vy..v.vy + v.vh {
        for x in v.vx..v.vx + v.vw {
            let (sx, sy) = v.sq(x, y);
            'jaws: for (jaw, set) in jaws.iter().enumerate() {
                // Depth into this jaw from its own edge of the view.
                let d = if jaw == 0 { sy } else { h - sy };
                let cell = c.get(x, y);
                if d < gum {
                    let vein = vnoise(sx * 0.8, sy * 1.5 + jaw as f32 * 9.0);
                    let ch = if vein > 0.62 {
                        '~'
                    } else if vein < 0.3 {
                        '%'
                    } else {
                        ' '
                    };
                    let bg = Rgb(90, 12, 22).lerp(Rgb(140, 30, 40), vein);
                    c.set(x, y, ch, Rgb(200, 60, 70), bg);
                    break;
                }
                let into = d - gum;
                for &(fx, len, half) in set {
                    if into > len {
                        continue;
                    }
                    let span = half * (1.0 - into / len.max(0.1));
                    let off = sx - fx;
                    if off.abs() > span + 0.25 {
                        continue;
                    }
                    let ch = if into > len - 1.2 {
                        if jaw == 0 {
                            'V'
                        } else {
                            'A'
                        }
                    } else if off.abs() > span - 0.5 {
                        match (jaw == 0, off < 0.0) {
                            (true, true) | (false, false) => '\\',
                            _ => '/',
                        }
                    } else {
                        '█'
                    };
                    let shade = 0.75 + 0.25 * (1.0 - off.abs() / half.max(0.1));
                    let ivory = Rgb(238, 228, 195).scale(shade);
                    c.set(x, y, ch, ivory, cell.bg.lerp(Rgb(60, 50, 40), 0.6));
                    break 'jaws;
                }
            }
        }
    }
}

/// An alarm: hard pulses of red and amber beating in from the edges.
fn alarm(c: &mut Canvas, v: View, s: &ScreenFx) {
    let k = s.k();
    let ph = k * 4.0;
    let wave = ph.fract();
    let col = if (ph as i32) % 2 == 0 {
        Rgb(255, 50, 30)
    } else {
        Rgb(255, 200, 50)
    };
    let fade = 1.0 - k;
    let reach = v.w().min(v.h()) * 0.5;
    for y in v.vy..v.vy + v.vh {
        for x in v.vx..v.vx + v.vw {
            let (d, _, _) = rim(v, x, y);
            let e = d / reach;
            let front = (-(e - wave * 0.8).powi(2) / 0.006).exp() * (1.0 - wave);
            let edge = (1.0 - e / 0.12).max(0.0) * (0.5 + 0.5 * (wave * TAU).cos());
            let a = (front * 0.55 + edge * 0.45) * fade;
            if a > 0.02 {
                let cell = c.get(x, y);
                let ch = if e < 0.03 && (x + y) % 2 == 0 { '!' } else { cell.ch };
                c.set(x, y, ch, cell.fg.lerp(col, a), cell.bg.lerp(col.scale(0.6), a));
            }
        }
    }
}

/// Frost: ice crystals creep in from the edges and melt away.
fn frost(c: &mut Canvas, v: View, s: &ScreenFx) {
    let k = s.k();
    let env = envelope(k, 0.25, 0.4);
    let h = v.h();
    let off = rnd(s.seed, 3) * 50.0;
    for y in v.vy..v.vy + v.vh {
        for x in v.vx..v.vx + v.vw {
            let (d, along, _) = rim(v, x, y);
            // Jagged, crystalline fingers.
            let spike = (along * 37.0 + off).sin().abs() * (along * 13.0 + off * 0.5).sin().abs();
            let reach = h * (0.08 + 0.17 * spike) * env;
            let n = vnoise(x as f32 * 0.6 + off, y as f32 * 1.1);
            let d = d + n * 1.5;
            if d >= reach {
                continue;
            }
            let i = 1.0 - d / reach;
            let hh = hashf(x, y * 7 + 3);
            let ch = if i > 0.65 {
                ['▓', '#', '▒'][(hh * 3.0) as usize % 3]
            } else if i > 0.3 {
                ['*', '+', 'x', '*'][(hh * 4.0) as usize % 4]
            } else {
                ['.', '\'', '`', '·'][(hh * 4.0) as usize % 4]
            };
            let cell = c.get(x, y);
            let fg = Rgb(170, 215, 255).lerp(Rgb(240, 250, 255), i);
            c.set(x, y, ch, fg, cell.bg.lerp(Rgb(60, 100, 145), i * 0.6));
        }
    }
    c.tint_region(v.vx, v.vy, v.vw, v.vh, Rgb(120, 180, 255), 0.1 * env);
}

// ================================================================ spores

/// Muscles seized by spores: motes drift before your eyes and the edges of
/// the view pulse a sickly lilac.
fn seized(c: &mut Canvas, v: View, left: i32, t: f32) {
    let s = (left as f32 / 3.0).min(1.0);
    let pulse = 0.6 + 0.4 * (t * 2.2).sin();
    let rim_w = 5.0;
    for y in v.vy..v.vy + v.vh {
        for x in v.vx..v.vx + v.vw {
            let (d, _, _) = rim(v, x, y);
            if d < rim_w {
                let a = (1.0 - d / rim_w) * 0.4 * pulse * s;
                let cell = c.get(x, y);
                c.set(x, y, cell.ch, cell.fg, cell.bg.lerp(Rgb(110, 55, 130), a));
            }
        }
    }
    let n = (24.0 * s) as i32 + 6;
    for i in 0..n {
        let u = (rnd(11, i) + 0.03 * (t * 0.7 + i as f32).sin()).rem_euclid(1.0);
        let w = (rnd(12, i) - t * 0.025 * (1.0 + rnd(13, i))).rem_euclid(1.0);
        let x = v.vx + (u * v.vw as f32) as i32;
        let y = v.vy + (w * v.vh as f32) as i32;
        let tw = 0.5 + 0.5 * (t * 3.0 + i as f32 * 1.7).sin();
        let ch = ['·', '°', 'o', '*'][(hash2(i, 5) % 4) as usize];
        let cell = c.get(x, y);
        c.set(x, y, ch, Rgb(230, 180, 240).scale(0.5 + 0.5 * tw), cell.bg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webs_hang_in_the_corners_and_break_in_stages() {
        let v = View { vx: 0, vy: 0, vw: 120, vh: 40 };
        let st = web_strands(v, 1234);
        assert!(st.len() > 200, "a full web has plenty of strands");
        for s in &st {
            for p in [s.a, s.b] {
                assert!(p.0 >= -1.0 && p.0 <= v.w() + 1.0 && p.1 >= -1.0 && p.1 <= v.h() + 1.0);
            }
        }
        // The middle of the view stays clear so you can still see to fight.
        let (mx, my) = (v.w() * 0.5, v.h() * 0.5);
        let near = st
            .iter()
            .filter(|s| ((s.b.0 - mx).powi(2) + (s.b.1 - my).powi(2)).sqrt() < 4.0)
            .count();
        assert!(near < 4, "{} strands in the very middle", near);
        // As the web weakens, fewer strands hold.
        let holding = |f: f32| st.iter().filter(|s| s.key < f).count();
        assert!(holding(1.0) > holding(0.66) && holding(0.66) > holding(0.33));
        assert!(holding(0.33) > 0);
    }

    #[test]
    fn line_glyphs_follow_the_strand() {
        assert_eq!(line_glyph(1.0, 0.0), '-');
        assert_eq!(line_glyph(0.0, 1.0), '|');
        assert_eq!(line_glyph(1.0, 1.0), '\\');
        assert_eq!(line_glyph(1.0, -1.0), '/');
    }
}

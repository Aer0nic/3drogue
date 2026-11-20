//! Rendering of the game view: an ASCII raycaster with textured walls,
//! floor/ceiling casting, coloured dynamic lighting and scaled ASCII sprites;
//! the overhead map; the HUD and in-game overlays.

use crate::canvas::Canvas;
use crate::color::{hash2, hashf, vnoise, Rgb};
use crate::data::*;
use crate::entity::Faction;
use crate::game::*;
use crate::map::*;
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
        0 => Palette { wall: Rgb(160, 115, 85), floor: Rgb(125, 118, 105), ceil: Rgb(80, 72, 65), accent: Rgb(220, 160, 90) },
        1 => Palette { wall: Rgb(100, 128, 88), floor: Rgb(95, 108, 88), ceil: Rgb(55, 66, 55), accent: Rgb(130, 220, 120) },
        2 => Palette { wall: Rgb(88, 80, 108), floor: Rgb(92, 84, 100), ceil: Rgb(45, 38, 55), accent: Rgb(255, 110, 60) },
        _ => Palette { wall: Rgb(205, 195, 165), floor: Rgb(125, 92, 82), ceil: Rgb(65, 42, 42), accent: Rgb(180, 110, 255) },
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
    let torch = 1.25 / (1.0 + d2 * 7.5 / s2) * flicker(t);
    let tc = if g.p.invis > 0 { [0.45, 0.5, 0.8] } else { [1.0, 0.8, 0.55] };
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

/// Wall texture: (brightness multiplier, forced character, colour override).
fn wall_tex(style: u8, tile: Tile, u: f32, v: f32, mx: i32, my: i32) -> (f32, Option<char>, Option<Rgb>) {
    if tile == Tile::DoorClosed {
        let fu = (u * 5.0).fract();
        if (0.14f32..0.22).contains(&v) || (0.78f32..0.86).contains(&v) {
            return (0.9, Some('='), Some(Rgb(140, 140, 150)));
        }
        if (0.72f32..0.82).contains(&u) && (0.47f32..0.56).contains(&v) {
            return (1.4, Some('o'), Some(Rgb(230, 190, 80)));
        }
        if fu < 0.12 {
            return (0.4, Some('|'), Some(Rgb(90, 60, 35)));
        }
        let grain = 0.85 + 0.25 * vnoise(u * 5.0 + mx as f32 * 3.0, v * 30.0);
        return (grain, None, Some(Rgb(150, 100, 55)));
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
        _ => {
            let r = (v * 4.0).floor();
            let fv = (v * 4.0).fract();
            let bu = u * 2.0 + if (r as i32) % 2 == 0 { 0.0 } else { 0.5 };
            let fu = bu.fract();
            if fv < 0.08 || fu < 0.05 {
                return (0.3, Some('.'), Some(Rgb(120, 40, 40)));
            }
            if ((0.22f32..0.4).contains(&fu) || (0.6f32..0.78).contains(&fu)) && (0.3f32..0.55).contains(&fv) {
                return (0.15, Some('o'), None);
            }
            if (0.3f32..0.7).contains(&fu) && fv > 0.75 {
                return (0.6, Some('"'), None);
            }
            (0.9 + 0.2 * hashf(mx, my + r as i32), None, None)
        }
    }
}

fn art_char(art: &[&str], r: usize, c: usize) -> char {
    art.get(r).and_then(|line| line.chars().nth(c)).unwrap_or(' ')
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
    d2: f32,
}

pub fn render_game(c: &mut Canvas, g: &Game, t: f32) {
    let w = c.w as i32;
    let h = c.h as i32;
    let vy = 1;
    let vh = h - HUD_H - 1;
    if g.overhead {
        render_overhead(c, g, 0, vy, w, vh, t);
    } else {
        render_fp(c, g, 0, vy, w, vh, t);
        let mw = 23.min(w / 4);
        let mh = 11.min(vh / 3);
        draw_minimap(c, g, w - mw - 1, vy + 1, mw, mh);
    }
    post_effects(c, g, 0, vy, w, vh, t);
    draw_topbar(c, g, w, t);
    draw_hud(c, g, h - HUD_H, w, t);
}

// ================================================================ first person

pub fn render_fp(c: &mut Canvas, g: &Game, vx: i32, vy: i32, vw: i32, vh: i32, t: f32) {
    let lv = &g.level;
    let pal = palette(lv.style);
    let shake = g.fx.shake;
    let ang = g.cam.ang + (t * 61.0).sin() * shake * 0.025;
    let (dirx, diry) = (ang.cos(), ang.sin());
    let half = (FOV / 2.0).tan();
    let (plx, ply) = (-diry * half, dirx * half);
    let proj = (vw as f32 / 2.0) / half * 0.5;
    let horizon = vh as f32 / 2.0 + g.cam.walk.sin() * 0.35 + (t * 47.0).cos() * shake * 0.8;
    let (camx0, camy0) = (g.cam.x, g.cam.y);
    let mut zbuf = vec![f32::MAX; vw.max(1) as usize];
    let traps: Vec<(i32, i32)> = lv.traps.iter().filter(|tr| tr.revealed).map(|tr| (tr.x, tr.y)).collect();
    let pulse = 0.5 + 0.5 * (t * 2.5).sin();

    for col in 0..vw {
        let camx = 2.0 * (col as f32 + 0.5) / vw as f32 - 1.0;
        let rdx = dirx + plx * camx;
        let rdy = diry + ply * camx;
        let rlen = (rdx * rdx + rdy * rdy).sqrt();
        let mut mapx = camx0.floor() as i32;
        let mut mapy = camy0.floor() as i32;
        let ddx = if rdx.abs() < 1e-9 { 1e30 } else { (1.0 / rdx).abs() };
        let ddy = if rdy.abs() < 1e-9 { 1e30 } else { (1.0 / rdy).abs() };
        let (stepx, mut sdx) = if rdx < 0.0 { (-1, (camx0 - mapx as f32) * ddx) } else { (1, (mapx as f32 + 1.0 - camx0) * ddx) };
        let (stepy, mut sdy) = if rdy < 0.0 { (-1, (camy0 - mapy as f32) * ddy) } else { (1, (mapy as f32 + 1.0 - camy0) * ddy) };
        let mut side = 0;
        let mut hit = false;
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
            if lv.tile(mapx, mapy).opaque() {
                hit = true;
                break;
            }
        }
        let perp = (if side == 0 { sdx - ddx } else { sdy - ddy }).max(0.02);
        zbuf[col as usize] = if hit { perp } else { 1e9 };
        let tile = lv.tile(mapx, mapy);
        let mut u = if side == 0 { camy0 + perp * rdy } else { camx0 + perp * rdx };
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
        let face = if side == 0 { (mapx - stepx, mapy) } else { (mapx, mapy - stepy) };
        let torch = lv.torches.iter().find(|tr| tr.x == mapx && tr.y == mapy && tr.fx == face.0 && tr.fy == face.1);
        let side_f = if side == 1 { 0.78 } else { 1.0 };

        for row in 0..vh {
            let yf = row as f32 + 0.5;
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
                            let base = Rgb((255.0 * tr.color[0].min(1.0)) as u8, (230.0 * tr.color[1].min(1.0)) as u8, (180.0 * tr.color[2].min(1.0)) as u8);
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
                    let (mult, forced, tint) = wall_tex(lv.style, tile, u, v, mapx, mapy);
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
                        ch = forced.unwrap_or(WALL_RAMP[(bri.sqrt() / 1.15 * 8.99).min(8.0) as usize]);
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
                let l = light_at(g, fx, fy, t);
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
                    mult = 0.82 + 0.3 * hashf(tx * 2 + (u2 * 2.0) as i32, ty * 2 + (v2 * 2.0) as i32);
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
                if lv.inb(tx, ty) {
                    let bl = lv.blood[lv.idx(tx, ty)];
                    if bl > 0.0 && hashf((fx * 5.0).floor() as i32, (fy * 5.0).floor() as i32) < bl * 0.3 {
                        base = Rgb(130, 12, 12);
                        forced = Some(if hashf(tx * 3, ty * 5) < 0.5 { '%' } else { '~' });
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
                let l0 = light_at(g, fx, fy, t);
                let l = [l0[0] * 0.55, l0[1] * 0.55, l0[2] * 0.55];
                let beam = (fx.fract() < 0.06) || (fy.fract() < 0.06);
                let mult = if beam { 0.5 } else { 0.85 + 0.25 * hashf(fx.floor() as i32, fy.floor() as i32) };
                let bri = (lum(l) * mult).clamp(0.0, 1.0);
                if bri < 0.02 {
                    ch = ' ';
                    fg = Rgb::BLACK;
                    bg = Rgb::BLACK;
                } else {
                    let c0 = pal.ceil.lit(l).scale(mult * 1.2);
                    ch = if beam { '=' } else { CEIL_RAMP[(bri * 4.99).min(4.0) as usize] };
                    fg = c0;
                    bg = c0.scale(0.25);
                }
            }
            c.set(vx + col, vy + row, ch, fg, bg);
        }
    }

    // ---------- sprites
    let mut sprs: Vec<Spr> = Vec::new();
    let d2 = |x: f32, y: f32| (x - camx0).powi(2) + (y - camy0).powi(2);
    for corpse in &lv.corpses {
        let def = &MONSTERS[corpse.kind];
        let undead = def.death == DeathKind::Undead;
        let (x, y) = (corpse.x as f32 + 0.5, corpse.y as f32 + 0.5);
        sprs.push(Spr {
            x,
            y,
            art: if undead { ART_BONES } else { ART_CORPSE },
            glyph: '%',
            color: if undead { Rgb(200, 195, 175) } else { def.color.lerp(Rgb(120, 15, 15), 0.6) },
            eye: Rgb::BLACK,
            size: 0.14,
            float: 0.0,
            flash: 0.0,
            glow: false,
            d2: d2(x, y),
        });
    }
    for it in &lv.items {
        let (x, y) = (it.x as f32 + 0.5, it.y as f32 + 0.5);
        let amulet = it.item == Item::Amulet;
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
            glow: amulet,
            d2: d2(x, y),
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
        if m.faction == Faction::Ally {
            color = color.lerp(Rgb(120, 255, 140), 0.3);
        }
        let bob = if def.float > 0.0 { (t * 6.0 + m.id as f32).sin() * 0.05 } else { 0.0 };
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
            d2: d2(x, y),
        });
    }
    sprs.sort_by(|a, b| b.d2.partial_cmp(&a.d2).unwrap_or(std::cmp::Ordering::Equal));
    let inv = 1.0 / (plx * diry - dirx * ply);
    let project = |wx: f32, wy: f32| -> (f32, f32) {
        let rx = wx - camx0;
        let ry = wy - camy0;
        let tx = inv * (diry * rx - dirx * ry);
        let ty = inv * (-ply * rx + plx * ry);
        (tx, ty)
    };

    for s in &sprs {
        let (tx, ty) = project(s.x, s.y);
        if ty <= 0.2 {
            continue;
        }
        let scx = (vw as f32 / 2.0) * (1.0 + tx / ty);
        let scale = proj / ty;
        let l = light_at(g, s.x, s.y, t);
        let lu = lum(l);
        let dark = lu < 0.07 && !s.glow;
        let ah = s.art.len().max(1) as f32;
        let aw = art_width(s.art) as f32;
        let h_rows = s.size * scale;
        let bottom = horizon + scale * 0.5 - s.float * scale;
        let top = bottom - h_rows;
        if h_rows < 1.3 {
            let col = scx as i32;
            let row = (bottom - 0.5) as i32;
            if col >= 0 && col < vw && row >= 0 && row < vh && ty < zbuf[col as usize] {
                if dark {
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
        let w_cols = aw * k;
        let left = scx - w_cols / 2.0;
        let c0 = (left.floor() as i32).max(0);
        let c1 = ((left + w_cols).ceil() as i32).min(vw);
        let r0 = (top.floor() as i32).max(0);
        let r1 = (bottom.ceil() as i32).min(vh);
        let lit = s.color.lit([l[0].min(1.6), l[1].min(1.6), l[2].min(1.6)]);
        let shown = if s.glow { s.color.lerp(Rgb::WHITE, 0.2 + 0.2 * pulse) } else { lit.lerp(Rgb::WHITE, s.flash) };
        for col in c0..c1 {
            if ty >= zbuf[col as usize] {
                continue;
            }
            let ac = ((col as f32 + 0.5 - left) / k) as usize;
            for row in r0..r1 {
                let ar = ((row as f32 + 0.5 - top) / k) as usize;
                let ch = art_char(s.art, ar, ac);
                if ch == ' ' {
                    continue;
                }
                if ch == '§' {
                    let glow = s.eye.lerp(Rgb::WHITE, 0.15 * pulse);
                    c.set(vx + col, vy + row, if k > 1.5 { '@' } else { 'o' }, glow, s.eye.scale(0.3));
                } else if !dark {
                    c.set(vx + col, vy + row, ch, shown, shown.scale(0.18));
                }
            }
        }
    }

    // ---------- particles, projectiles, combat text
    for p in &g.fx.parts {
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
            p.color.lit([l[0].max(0.25), l[1].max(0.25), l[2].max(0.25)])
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
            let ch = if k == 0 { b.ch } else if b.light.is_some() { '*' } else { '.' };
            c.set(vx + col, vy + row, ch, b.color.scale(1.0 - k as f32 * 0.18), b.color.scale(0.25 * (1.0 - k as f32 * 0.2)));
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
                c.set(vx + xx, vy + row, chr, ft.color.scale(0.4 + 0.6 * a), Rgb::BLACK);
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
    let shade = [l[0].min(1.2) * 0.75, l[1].min(1.2) * 0.75, l[2].min(1.2) * 0.75];
    let fade = if g.p.invis > 0 { 0.45 } else { 1.0 };
    let pulse = 0.5 + 0.5 * (t * 3.0).sin();
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
            let mut col = if handle { Rgb(150, 105, 60) } else { w.color };
            let mut ch2 = ch;
            if ch == '§' {
                ch2 = '*';
                col = w.color.lerp(Rgb(150, 200, 255), 0.5 + 0.5 * pulse);
            } else {
                col = col.lit(shade);
                if w.glow && !handle {
                    col = col.lerp(w.color.lerp(Rgb::WHITE, 0.3), 0.4 + 0.3 * pulse);
                }
            }
            c.set(x, y, ch2, col.scale(fade), col.scale(0.22 * fade));
        }
    }
    if swinging && sw > 0.3 {
        for i in 0..6 {
            let x = bx - 2 - i * 2 + (ph * 10.0) as i32;
            let y = by + i;
            if y >= vy && y < vy + vh {
                c.put(x, y, '\\', Rgb(255, 255, 255).scale(sw * (1.0 - i as f32 * 0.12)));
            }
        }
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
    let flames = ['(', ')', '*', '^', '\'', ',', '"', '&'];
    let shape: [i32; 4] = [1, 2, 2, 1];
    for (r, &half) in shape.iter().enumerate() {
        for dx in -half..=half {
            let h = hash2(frame * 7 + dx, r as i32 * 13 + frame);
            if r == 0 && h % 3 == 0 {
                continue;
            }
            let ch = flames[(h % flames.len() as u32) as usize];
            let heat = r as f32 / 3.0;
            let col = Rgb(255, 80, 20).lerp(Rgb(255, 240, 150), heat + 0.2 * ((h % 5) as f32 / 5.0));
            c.set(x0 + dx, y0 + r as i32, ch, col, Rgb(90, 30, 5).scale(0.6 + 0.4 * heat));
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
            };
            c.set(x0 + i, y0 + j, ch, col.scale(k), bg);
        }
    }
    for m in &lv.monsters {
        if m.alive && lv.is_visible(m.x, m.y) {
            let (i, j) = (m.x - px + w / 2, m.y - py + h / 2);
            if i >= 0 && i < w && j >= 0 && j < h {
                let col = if m.faction == Faction::Ally { Rgb(120, 255, 120) } else { Rgb(255, 70, 50) };
                c.set(x0 + i, y0 + j, m.def().glyph, col, bg);
            }
        }
    }
    c.set(x0 + w / 2, y0 + h / 2, ARROWS[g.p.facing as usize], Rgb(255, 255, 120), Rgb(60, 50, 0));
}

// ================================================================ overhead

pub fn render_overhead(c: &mut Canvas, g: &Game, vx: i32, vy: i32, vw: i32, vh: i32, t: f32) {
    let lv = &g.level;
    let pal = palette(lv.style);
    let (ox, oy) = (g.p.x, g.p.y);
    let hw = vw / 2 - 1;
    let hh = vh / 2;
    let pulse = 0.5 + 0.5 * (t * 3.0).sin();
    let to_screen = |wx: f32, wy: f32| -> (i32, i32) {
        (((wx - ox as f32) * 2.0).round() as i32 + hw, (wy - oy as f32).round() as i32 + hh)
    };
    let traps: Vec<(i32, i32)> = lv.traps.iter().filter(|tr| tr.revealed).map(|tr| (tr.x, tr.y)).collect();

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
            let l = if vis { light_at(g, tx as f32 + 0.5, ty as f32 + 0.5, t) } else { [0.16, 0.18, 0.28] };
            let tile = lv.tile(tx, ty);
            let (c1, c2, base, solid) = match tile {
                Tile::Wall => {
                    let h = hash2(tx, ty);
                    let ch = if h % 7 == 0 { '%' } else { '#' };
                    (ch, '#', pal.wall, true)
                }
                Tile::Floor => {
                    let h = hashf(tx, ty);
                    (if h < 0.15 { ',' } else { '.' }, ' ', pal.floor, false)
                }
                Tile::DoorClosed => ('+', '+', Rgb(170, 115, 60), true),
                Tile::DoorOpen => ('\'', ' ', Rgb(170, 115, 60), false),
                Tile::StairsUp => ('<', '<', Rgb(130, 190, 255).lerp(Rgb::WHITE, 0.3 * pulse), false),
                Tile::StairsDown => ('>', '>', Rgb(210, 120, 255).lerp(Rgb::WHITE, 0.3 * pulse), false),
            };
            let stairs = matches!(tile, Tile::StairsUp | Tile::StairsDown);
            let mut fg = if stairs { base.scale(if vis { 1.0 } else { 0.6 }) } else { base.lit(l).scale(1.1) };
            let mut ch1 = c1;
            let mut ch2 = c2;
            if !vis && !stairs {
                fg = base.lerp(Rgb(70, 80, 120), 0.6).scale(0.45);
            }
            let mut bg = if solid { fg.scale(0.35) } else { fg.scale(0.12) };
            if tile == Tile::Floor || tile == Tile::DoorOpen {
                let bl = lv.blood[i];
                if bl > 0.0 && vis {
                    fg = fg.lerp(Rgb(160, 15, 15), (bl * 0.4).min(0.8));
                    bg = bg.lerp(Rgb(60, 0, 0), (bl * 0.3).min(0.6));
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
        if !m.alive || !lv.is_visible(m.x, m.y) {
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
            let bg = if m.faction == Faction::Ally { Rgb(0, 60, 0) } else if m.flash > 0.3 { Rgb(120, 0, 0) } else { c.get(vx + sx, vy + sy).bg };
            c.set(vx + sx, vy + sy, def.glyph, col, bg);
            if !m.awake && ((t * 1.5 + m.id as f32) as i32) % 3 == 0 {
                put(c, sx + 1, sy, 'z', Rgb(120, 120, 200));
            } else if m.stun > 0 {
                put(c, sx + 1, sy, '*', Rgb(255, 255, 120));
            }
        }
    }
    // the player
    let (psx, psy) = (hw, hh);
    let pcol = if g.p.invis > 0 { Rgb(120, 120, 200) } else { Rgb(255, 255, 200) };
    if psx < vw && psy < vh {
        c.set(vx + psx, vy + psy, '@', pcol, Rgb(60, 50, 20));
        c.put(vx + psx + 1, vy + psy, ARROWS[g.p.facing as usize], Rgb(255, 220, 90));
    }
    for p in &g.fx.parts {
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
            let (sx, sy) = to_screen(bx - 0.5, by - 0.5);
            let ch = if k == 0 { b.ch } else { '.' };
            put(c, sx, sy, ch, b.color.scale(1.0 - k as f32 * 0.22));
        }
    }
    for ft in &g.fx.texts {
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
    if frac < 0.3 && g.over.is_none() {
        let pulse = (0.5 + 0.5 * (t * 4.0).sin()) * (0.3 - frac) / 0.3;
        let edge = 4;
        for y in vy..vy + vh {
            for x in vx..vx + vw {
                let d = (x - vx).min(vx + vw - 1 - x).min((y - vy) * 2).min((vy + vh - 1 - y) * 2);
                if d < edge {
                    let a = pulse * (1.0 - d as f32 / edge as f32) * 0.7;
                    c.tint_region(x, y, 1, 1, Rgb(160, 0, 0), a);
                }
            }
        }
    }
    if g.p.poison > 0 {
        c.tint_region(vx, vy, vw, vh, Rgb(60, 200, 40), 0.06 + 0.04 * (t * 3.0).sin());
    }
    if g.p.invis > 0 {
        c.tint_region(vx, vy, vw, vh, Rgb(40, 40, 110), 0.18);
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
    c.text(1, 0, "GRAZZARK", Rgb(200, 60, 50).lerp(Rgb(255, 150, 80), 0.5 + 0.5 * (t * 1.3).sin()));
    let cd = g.cdef();
    c.text(11, 0, &format!("{} lv{}", cd.name, g.p.level), cd.color);
    let labels = ["E", "SE", "S", "SW", "W", "NW", "N", "NE"];
    let strip = 41.min(w - 50).max(0);
    let sx0 = w / 2 - strip / 2;
    let ang = if g.overhead { facing_angle(g.p.facing) } else { g.cam.ang };
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
            let col = if labels[li] == "N" { Rgb(255, 80, 60) } else { Rgb(220, 200, 150) };
            c.text(sx0 + i, 0, labels[li], col);
        } else if c.get(sx0 + i, 0).ch == ' ' {
            c.put(sx0 + i, 0, if i == strip / 2 { '|' } else { '.' }, Rgb(80, 70, 90));
        }
    }
    let help = "?:help Tab:map R:mode";
    c.text(w - help.len() as i32 - 1, 0, help, Rgb(110, 100, 120));
}

fn draw_hud(c: &mut Canvas, g: &Game, y0: i32, w: i32, t: f32) {
    let bg = Rgb(12, 10, 16);
    c.fill(0, y0, w, HUD_H, ' ', Rgb(180, 180, 180), bg);
    let pal = palette(g.level.style);
    for x in 0..w {
        c.put(x, y0, '=', pal.accent.scale(0.35));
    }
    let name = DEPTH_NAMES[((g.depth.max(1) - 1) % 10) as usize];
    c.text(2, y0, &format!(" Depth {}: {} ", g.depth, name), pal.accent);
    let (mode, mcol) = if g.realtime {
        (" REAL-TIME ", Rgb(255, 80, 60).lerp(Rgb(255, 200, 120), 0.5 + 0.5 * (t * 5.0).sin()))
    } else {
        (" TURN-BASED ", Rgb(120, 190, 255))
    };
    let view = if g.overhead { " MAP " } else { " 3D " };
    let tag_x = w - mode.len() as i32 - view.len() as i32 - 3;
    c.text_bg(tag_x, y0, mode, Rgb::BLACK, mcol);
    c.text_bg(tag_x + mode.len() as i32 + 1, y0, view, Rgb::BLACK, Rgb(180, 170, 140));

    // row 1: vitals
    let y = y0 + 1;
    let hpf = g.p.hp as f32 / g.p.max_hp.max(1) as f32;
    let hpc = Rgb(220, 40, 40).lerp(Rgb(60, 220, 60), hpf);
    c.text(1, y, "HP", Rgb(255, 120, 120));
    bar(c, 4, y, 20, hpf, hpc, Rgb(60, 30, 30));
    c.text(25, y, &format!("{}/{}", g.p.hp, g.p.max_hp), hpc);
    c.text(34, y, "MP", Rgb(120, 150, 255));
    bar(c, 37, y, 14, g.p.mp as f32 / g.p.max_mp.max(1) as f32, Rgb(70, 110, 255), Rgb(30, 30, 60));
    c.text(52, y, &format!("{}/{}", g.p.mp, g.p.max_mp), Rgb(120, 150, 255));
    let need = Game::xp_needed(g.p.level);
    let prev = if g.p.level > 1 { Game::xp_needed(g.p.level - 1) } else { 0 };
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
    }

    // row 2: gear
    let y = y0 + 2;
    let wd = g.wdef();
    let wname = gear_name(wd.name, g.p.wpn.bonus);
    let b = g.dmg_bonus();
    c.text(1, y, &format!("Wpn: {} ({}-{}{})", wname, wd.dmg.0 + b, wd.dmg.1 + b, if wd.range > 0 { " ranged" } else { "" }), Rgb(200, 200, 210));
    let aname = gear_name(ARMORS[g.p.arm.def].name, g.p.arm.bonus);
    c.text(42, y, &format!("Arm: {} [AC {}]", aname, g.player_ac()), Rgb(180, 170, 150));
    let pots = g.p.inv.iter().filter(|i| **i == Item::Potion(Potion::Healing)).count();
    c.text(76, y, &format!("Heal:{}", pots), Rgb(255, 100, 100));
    c.text(85, y, &format!("Kills:{} Turn:{}", g.kills, g.turns), Rgb(150, 150, 150));
    if g.p.has_amulet {
        let col = Rgb(255, 200, 40).lerp(Rgb::WHITE, 0.5 + 0.5 * (t * 4.0).sin());
        c.text_bg(w - 11, y, " AMULET ", Rgb::BLACK, col);
    }

    // row 3: skills
    let y = y0 + 3;
    let mut x = 1;
    for (i, s) in g.skills().iter().enumerate() {
        let wait = g.skill_ready_in(*s);
        let can = wait <= 0.0 && g.p.mp >= s.mp();
        let label = if wait > 0.0 {
            format!("[{}] {} ({})", i + 1, s.name(), wait)
        } else if s.mp() > 0 {
            format!("[{}] {} {}mp", i + 1, s.name(), s.mp())
        } else {
            format!("[{}] {}", i + 1, s.name())
        };
        let col = if can { Rgb(160, 255, 160) } else { Rgb(100, 100, 100) };
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
        let fade = if k == 0 { 1.0 } else if k == 1 { 0.7 } else { 0.5 };
        let fresh = if m.age < 0.4 { 0.3 * (1.0 - m.age / 0.4) } else { 0.0 };
        let text: String = m.text.chars().take((w - 2).max(0) as usize).collect();
        c.text(1, y, &text, m.color.scale(fade).lerp(Rgb::WHITE, fresh));
    }
}

// ================================================================ overlays

pub fn draw_inventory(c: &mut Canvas, g: &Game) {
    let w = 60.min(c.w as i32 - 4);
    let h = (g.p.inv.len() as i32 + 7).max(10).min(c.h as i32 - 2);
    let x0 = (c.w as i32 - w) / 2;
    let y0 = (c.h as i32 - h) / 2;
    c.frame(x0, y0, w, h, Rgb(180, 150, 100), Rgb(16, 12, 10));
    c.text(x0 + 2, y0, " INVENTORY ", Rgb(255, 220, 150));
    if g.p.inv.is_empty() {
        c.text(x0 + 3, y0 + 2, "Your pack is empty.", Rgb(150, 150, 150));
    }
    for (i, it) in g.p.inv.iter().enumerate() {
        let y = y0 + 2 + i as i32;
        if y >= y0 + h - 3 {
            break;
        }
        let letter = (b'a' + i as u8) as char;
        let extra = match it {
            Item::Weapon(gr) => {
                let d = &WEAPONS[gr.def];
                format!("  {}-{}{}", d.dmg.0 + gr.bonus, d.dmg.1 + gr.bonus, if d.range > 0 { " ranged" } else { "" })
            }
            Item::Armor(gr) => format!("  AC {}", ARMORS[gr.def].ac + gr.bonus),
            _ => String::new(),
        };
        c.text(x0 + 3, y, &format!("{}) ", letter), Rgb(255, 230, 150));
        c.text(x0 + 6, y, &it.glyph().to_string(), it.color());
        c.text(x0 + 8, y, &format!("{}{}", it.name(), extra), Rgb(220, 220, 220));
    }
    c.text(x0 + 2, y0 + h - 2, "a-r: use/equip   SHIFT+letter: drop   Esc: close", Rgb(150, 140, 120));
}

pub const HELP_LINES: &[&str] = &[
    "THE QUEST: Descend to depth 10, claim the Amulet of GRAZZARK from its",
    "Warden, then climb all the way back to the surface alive.",
    "",
    "FIRST-PERSON VIEW            OVERHEAD MAP (Tab)",
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
    "  R           toggle TURN-BASED / REAL-TIME combat",
    "  Tab or M    toggle first-person / overhead map",
    "  - / +       volume                     Esc      menu",
    "",
    "Monsters fight each other. The dead rise again over time. Listen:",
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
        let col = if l.chars().all(|ch| ch.is_uppercase() || ch == ' ' || ch == '(' || ch == ')' || ch == '-')
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

pub const PAUSE_ITEMS: [&str; 4] = ["Resume", "How to play", "Abandon run (earn soul points)", "Quit to desktop (run is lost)"];

pub fn draw_pause(c: &mut Canvas, sel: usize) {
    let w = 40;
    let h = 9;
    let x0 = (c.w as i32 - w) / 2;
    let y0 = (c.h as i32 - h) / 2;
    c.frame(x0, y0, w, h, Rgb(200, 120, 90), Rgb(16, 8, 8));
    c.text(x0 + 2, y0, " PAUSED ", Rgb(255, 180, 140));
    for (i, it) in PAUSE_ITEMS.iter().enumerate() {
        let y = y0 + 2 + i as i32;
        if i == sel {
            c.text_bg(x0 + 2, y, &format!("> {:<34}", it), Rgb::BLACK, Rgb(255, 180, 120));
        } else {
            c.text(x0 + 4, y, it, Rgb(200, 190, 180));
        }
    }
}

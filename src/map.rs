//! Dungeon levels: tiles, generation, lighting, field of view and pathing.

use crate::data::Item;
use crate::entity::Monster;
use rand::rngs::StdRng;
use rand::Rng;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub const MAP_W: i32 = 64;
pub const MAP_H: i32 = 40;
pub const UNREACH: i32 = 9999;

/// Eight compass directions, clockwise from north. Index = facing.
pub const DIRS: [(i32, i32); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

pub fn cheb(x0: i32, y0: i32, x1: i32, y1: i32) -> i32 {
    (x1 - x0).abs().max((y1 - y0).abs())
}

pub fn dist(x0: i32, y0: i32, x1: i32, y1: i32) -> f32 {
    (((x1 - x0) * (x1 - x0) + (y1 - y0) * (y1 - y0)) as f32).sqrt()
}

/// Bresenham line including both endpoints.
pub fn line(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
    let mut pts = Vec::new();
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        pts.push((x, y));
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
    pts
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    Wall,
    Floor,
    DoorClosed,
    DoorOpen,
    StairsUp,
    StairsDown,
}

impl Tile {
    pub fn opaque(self) -> bool {
        matches!(self, Tile::Wall | Tile::DoorClosed)
    }
    pub fn walkable(self) -> bool {
        matches!(self, Tile::Floor | Tile::DoorOpen | Tile::StairsUp | Tile::StairsDown)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
    pub fn overlaps(&self, o: &Rect, m: i32) -> bool {
        self.x - m < o.x + o.w && self.x + self.w + m > o.x && self.y - m < o.y + o.h && self.y + self.h + m > o.y
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrapKind {
    Spikes,
    Gas,
    Fire,
    Alarm,
}

#[derive(Clone, Debug)]
pub struct Trap {
    pub x: i32,
    pub y: i32,
    pub kind: TrapKind,
    pub revealed: bool,
}

#[derive(Clone, Debug)]
pub struct Corpse {
    pub kind: usize,
    pub x: i32,
    pub y: i32,
    /// World time at which it rises again; negative = never.
    pub revive_at: f64,
}

#[derive(Clone, Debug)]
pub struct FloorItem {
    pub x: i32,
    pub y: i32,
    pub item: Item,
}

/// A wall-mounted torch at (x,y), lighting the floor tile (fx,fy).
#[derive(Clone, Debug)]
pub struct Torch {
    pub x: i32,
    pub y: i32,
    pub fx: i32,
    pub fy: i32,
    pub color: [f32; 3],
}

pub struct Level {
    pub w: i32,
    pub h: i32,
    pub depth: u32,
    pub style: u8,
    pub tiles: Vec<Tile>,
    pub explored: Vec<bool>,
    pub visible: Vec<bool>,
    pub blood: Vec<f32>,
    pub light: Vec<[f32; 3]>,
    pub torches: Vec<Torch>,
    pub rooms: Vec<Rect>,
    pub up: (i32, i32),
    pub down: (i32, i32),
    pub monsters: Vec<Monster>,
    pub items: Vec<FloorItem>,
    pub corpses: Vec<Corpse>,
    pub traps: Vec<Trap>,
    /// Set once the amulet-bearer has been hunted on this level.
    pub hunted: bool,
}

impl Level {
    pub fn blank(depth: u32) -> Level {
        let n = (MAP_W * MAP_H) as usize;
        Level {
            w: MAP_W,
            h: MAP_H,
            depth,
            style: match depth {
                1..=3 => 0,
                4..=6 => 1,
                7..=9 => 2,
                _ => 3,
            },
            tiles: vec![Tile::Wall; n],
            explored: vec![false; n],
            visible: vec![false; n],
            blood: vec![0.0; n],
            light: vec![[0.0; 3]; n],
            torches: Vec::new(),
            rooms: Vec::new(),
            up: (0, 0),
            down: (0, 0),
            monsters: Vec::new(),
            items: Vec::new(),
            corpses: Vec::new(),
            traps: Vec::new(),
            hunted: false,
        }
    }

    #[inline]
    pub fn inb(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }

    #[inline]
    pub fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.w + x) as usize
    }

    #[inline]
    pub fn tile(&self, x: i32, y: i32) -> Tile {
        if self.inb(x, y) {
            self.tiles[self.idx(x, y)]
        } else {
            Tile::Wall
        }
    }

    pub fn set(&mut self, x: i32, y: i32, t: Tile) {
        if self.inb(x, y) {
            let i = self.idx(x, y);
            self.tiles[i] = t;
        }
    }

    pub fn is_visible(&self, x: i32, y: i32) -> bool {
        self.inb(x, y) && self.visible[self.idx(x, y)]
    }

    /// Diagonal moves may not cut past opaque corners.
    pub fn can_step(&self, x: i32, y: i32, dx: i32, dy: i32) -> bool {
        if dx != 0 && dy != 0 {
            !self.tile(x + dx, y).opaque() && !self.tile(x, y + dy).opaque()
        } else {
            true
        }
    }

    pub fn monster_at(&self, x: i32, y: i32) -> Option<usize> {
        self.monsters.iter().position(|m| m.alive && m.x == x && m.y == y)
    }

    pub fn los(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
        let pts = line(x0, y0, x1, y1);
        let n = pts.len();
        for &(x, y) in pts.iter().take(n.saturating_sub(1)).skip(1) {
            if self.tile(x, y).opaque() {
                return false;
            }
        }
        true
    }

    fn mark_seen(&mut self, x: i32, y: i32) {
        if self.inb(x, y) {
            let i = self.idx(x, y);
            self.visible[i] = true;
            self.explored[i] = true;
        }
    }

    pub fn compute_fov(&mut self, px: i32, py: i32, r: i32) {
        for v in self.visible.iter_mut() {
            *v = false;
        }
        let r2 = r * r + r;
        self.mark_seen(px, py);
        for i in -r..=r {
            let targets = [(px + i, py - r), (px + i, py + r), (px - r, py + i), (px + r, py + i)];
            for &(tx, ty) in targets.iter() {
                let pts = line(px, py, tx, ty);
                for &(x, y) in pts.iter().skip(1) {
                    if !self.inb(x, y) {
                        break;
                    }
                    let dx = x - px;
                    let dy = y - py;
                    if dx * dx + dy * dy > r2 {
                        break;
                    }
                    self.mark_seen(x, y);
                    if self.tile(x, y).opaque() {
                        break;
                    }
                }
            }
        }
        // Walls bordering visible floor are visible too (removes speckle artefacts).
        let mut extra = Vec::new();
        for y in (py - r).max(0)..=(py + r).min(self.h - 1) {
            for x in (px - r).max(0)..=(px + r).min(self.w - 1) {
                let i = self.idx(x, y);
                if self.visible[i] && !self.tiles[i].opaque() {
                    for &(dx, dy) in DIRS.iter() {
                        let nx = x + dx;
                        let ny = y + dy;
                        if self.inb(nx, ny) && self.tile(nx, ny).opaque() && !self.visible[self.idx(nx, ny)] {
                            extra.push((nx, ny));
                        }
                    }
                }
            }
        }
        for (x, y) in extra {
            self.mark_seen(x, y);
        }
    }

    /// Path distance from (sx,sy) to every tile. Closed doors cost extra.
    /// Used both for monster pathing and for sound propagation.
    pub fn dijkstra(&self, sx: i32, sy: i32) -> Vec<i32> {
        let mut d = vec![UNREACH; (self.w * self.h) as usize];
        if !self.inb(sx, sy) {
            return d;
        }
        let mut heap = BinaryHeap::new();
        let si = self.idx(sx, sy);
        d[si] = 0;
        heap.push(Reverse((0i32, si)));
        while let Some(Reverse((cd, i))) = heap.pop() {
            if cd > d[i] {
                continue;
            }
            let x = (i as i32) % self.w;
            let y = (i as i32) / self.w;
            for &(dx, dy) in DIRS.iter() {
                let nx = x + dx;
                let ny = y + dy;
                if !self.inb(nx, ny) {
                    continue;
                }
                let cost = match self.tile(nx, ny) {
                    Tile::Wall => continue,
                    Tile::DoorClosed => 3,
                    _ => 1,
                };
                if !self.can_step(x, y, dx, dy) {
                    continue;
                }
                let ni = self.idx(nx, ny);
                let nd = cd + cost;
                if nd < d[ni] {
                    d[ni] = nd;
                    heap.push(Reverse((nd, ni)));
                }
            }
        }
        d
    }

    pub fn random_floor_in(&self, r: &Rect, rng: &mut StdRng) -> Option<(i32, i32)> {
        for _ in 0..40 {
            let x = rng.gen_range(r.x..r.x + r.w);
            let y = rng.gen_range(r.y..r.y + r.h);
            if self.tile(x, y) == Tile::Floor && self.monster_at(x, y).is_none() {
                return Some((x, y));
            }
        }
        None
    }

    pub fn random_floor(&self, rng: &mut StdRng) -> (i32, i32) {
        loop {
            let x = rng.gen_range(1..self.w - 1);
            let y = rng.gen_range(1..self.h - 1);
            if self.tile(x, y) == Tile::Floor && self.monster_at(x, y).is_none() {
                return (x, y);
            }
        }
    }

    /// Precompute static torch lighting per tile.
    pub fn bake_light(&mut self) {
        let amb = match self.depth {
            1..=3 => 0.035,
            4..=6 => 0.03,
            7..=9 => 0.025,
            _ => 0.03,
        };
        let ambc = [amb, amb, amb * 1.3];
        self.light = vec![ambc; self.tiles.len()];
        let torches = self.torches.clone();
        for t in torches.iter() {
            let lx = t.fx as f32 + 0.5 + (t.x - t.fx) as f32 * 0.45;
            let ly = t.fy as f32 + 0.5 + (t.y - t.fy) as f32 * 0.45;
            for y in (t.fy - 8)..=(t.fy + 8) {
                for x in (t.fx - 8)..=(t.fx + 8) {
                    if !self.inb(x, y) {
                        continue;
                    }
                    let dx = x as f32 + 0.5 - lx;
                    let dy = y as f32 + 0.5 - ly;
                    let d2 = dx * dx + dy * dy;
                    if d2 > 64.0 {
                        continue;
                    }
                    if !((x == t.fx && y == t.fy) || self.los(t.fx, t.fy, x, y)) {
                        continue;
                    }
                    let a = 1.4 / (1.0 + d2 * 0.18);
                    let i = self.idx(x, y);
                    for k in 0..3 {
                        self.light[i][k] += t.color[k] * a;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------- generation

pub fn generate(depth: u32, rng: &mut StdRng) -> Level {
    loop {
        if let Some(l) = try_generate(depth, rng) {
            return l;
        }
    }
}

fn hline(lv: &mut Level, x0: i32, x1: i32, y: i32) {
    for x in x0.min(x1)..=x0.max(x1) {
        if lv.tile(x, y) == Tile::Wall {
            lv.set(x, y, Tile::Floor);
        }
    }
}

fn vline(lv: &mut Level, y0: i32, y1: i32, x: i32) {
    for y in y0.min(y1)..=y0.max(y1) {
        if lv.tile(x, y) == Tile::Wall {
            lv.set(x, y, Tile::Floor);
        }
    }
}

fn carve(lv: &mut Level, a: (i32, i32), b: (i32, i32), rng: &mut StdRng) {
    if rng.gen_bool(0.5) {
        hline(lv, a.0, b.0, a.1);
        vline(lv, a.1, b.1, b.0);
    } else {
        vline(lv, a.1, b.1, a.0);
        hline(lv, a.0, b.0, b.1);
    }
}

fn try_generate(depth: u32, rng: &mut StdRng) -> Option<Level> {
    let mut lv = Level::blank(depth);
    let mut rooms: Vec<Rect> = Vec::new();
    let target = 9 + rng.gen_range(0..5);
    for _ in 0..500 {
        if rooms.len() >= target {
            break;
        }
        let w = rng.gen_range(4..12);
        let h = rng.gen_range(4..9);
        let x = rng.gen_range(1..MAP_W - w - 1);
        let y = rng.gen_range(1..MAP_H - h - 1);
        let r = Rect { x, y, w, h };
        if rooms.iter().any(|o| o.overlaps(&r, 2)) {
            continue;
        }
        rooms.push(r);
    }
    if rooms.len() < 6 {
        return None;
    }
    rooms.sort_by_key(|r| r.x);

    for i in 0..rooms.len() {
        let r = rooms[i];
        let round = i > 0 && r.w >= 6 && r.h >= 6 && rng.gen_bool(0.3);
        let cx = r.x as f32 + r.w as f32 / 2.0;
        let cy = r.y as f32 + r.h as f32 / 2.0;
        for yy in r.y..r.y + r.h {
            for xx in r.x..r.x + r.w {
                if round {
                    let nx = (xx as f32 + 0.5 - cx) / (r.w as f32 / 2.0);
                    let ny = (yy as f32 + 0.5 - cy) / (r.h as f32 / 2.0);
                    if nx * nx + ny * ny > 1.0 {
                        continue;
                    }
                }
                lv.set(xx, yy, Tile::Floor);
            }
        }
        if !round && r.w >= 8 && r.h >= 6 && rng.gen_bool(0.45) {
            let pillars = [
                (r.x + 2, r.y + 1),
                (r.x + r.w - 3, r.y + 1),
                (r.x + 2, r.y + r.h - 2),
                (r.x + r.w - 3, r.y + r.h - 2),
            ];
            for &(px, py) in pillars.iter() {
                lv.set(px, py, Tile::Wall);
            }
        }
    }

    let n = rooms.len();
    for i in 1..n {
        let a = rooms[i - 1].center();
        let b = rooms[i].center();
        carve(&mut lv, a, b, rng);
    }
    let loops = 2 + rng.gen_range(0..3);
    for _ in 0..loops {
        let i = rng.gen_range(0..n);
        let j = rng.gen_range(0..n);
        if i != j {
            let a = rooms[i].center();
            let b = rooms[j].center();
            carve(&mut lv, a, b, rng);
        }
    }

    // Doors at one-tile chokepoints on room borders.
    for r in rooms.iter() {
        let mut cands = Vec::new();
        for x in r.x - 1..=r.x + r.w {
            cands.push((x, r.y - 1));
            cands.push((x, r.y + r.h));
        }
        for y in r.y..r.y + r.h {
            cands.push((r.x - 1, y));
            cands.push((r.x + r.w, y));
        }
        for (x, y) in cands {
            if lv.tile(x, y) != Tile::Floor {
                continue;
            }
            let horiz = lv.tile(x - 1, y).opaque()
                && lv.tile(x + 1, y).opaque()
                && !lv.tile(x, y - 1).opaque()
                && !lv.tile(x, y + 1).opaque();
            let vert = lv.tile(x, y - 1).opaque()
                && lv.tile(x, y + 1).opaque()
                && !lv.tile(x - 1, y).opaque()
                && !lv.tile(x + 1, y).opaque();
            if horiz || vert {
                let roll: f32 = rng.gen();
                if roll < 0.35 {
                    lv.set(x, y, Tile::DoorClosed);
                } else if roll < 0.45 {
                    lv.set(x, y, Tile::DoorOpen);
                }
            }
        }
    }

    let up = rooms[0].center();
    lv.set(up.0, up.1, Tile::StairsUp);
    let dm = lv.dijkstra(up.0, up.1);
    let mut best = (0usize, -1i32);
    for (i, r) in rooms.iter().enumerate() {
        let c = r.center();
        let d = dm[lv.idx(c.0, c.1)];
        if d < UNREACH && d > best.1 {
            best = (i, d);
        }
    }
    if best.1 < 12 {
        return None;
    }
    for i in 0..lv.tiles.len() {
        if lv.tiles[i].walkable() && dm[i] >= UNREACH {
            return None;
        }
    }
    let down = rooms[best.0].center();
    lv.set(down.0, down.1, Tile::StairsDown);
    lv.up = up;
    lv.down = down;
    lv.rooms = rooms;

    // Wall torches.
    let tcol: [f32; 3] = match depth {
        1..=3 => [1.0, 0.62, 0.28],
        4..=6 => [0.95, 0.7, 0.3],
        7..=9 => [1.0, 0.38, 0.15],
        _ => [0.55, 0.3, 1.0],
    };
    let rooms_c = lv.rooms.clone();
    for r in rooms_c.iter() {
        let count = rng.gen_range(0..3) + if depth <= 3 { 1 } else { 0 };
        for _ in 0..count {
            for _try in 0..20 {
                let side = rng.gen_range(0..4);
                let (wx, wy, fx, fy) = match side {
                    0 => {
                        let x = rng.gen_range(r.x..r.x + r.w);
                        (x, r.y - 1, x, r.y)
                    }
                    1 => {
                        let x = rng.gen_range(r.x..r.x + r.w);
                        (x, r.y + r.h, x, r.y + r.h - 1)
                    }
                    2 => {
                        let y = rng.gen_range(r.y..r.y + r.h);
                        (r.x - 1, y, r.x, y)
                    }
                    _ => {
                        let y = rng.gen_range(r.y..r.y + r.h);
                        (r.x + r.w, y, r.x + r.w - 1, y)
                    }
                };
                if lv.tile(wx, wy) == Tile::Wall
                    && lv.tile(fx, fy) == Tile::Floor
                    && !lv.torches.iter().any(|t| t.x == wx && t.y == wy)
                {
                    let var = 0.85 + rng.gen::<f32>() * 0.3;
                    let mut c = tcol;
                    if (4..=6).contains(&depth) && rng.gen_bool(0.3) {
                        c = [0.35, 1.0, 0.5];
                    }
                    lv.torches.push(Torch { x: wx, y: wy, fx, fy, color: [c[0] * var, c[1] * var, c[2] * var] });
                    break;
                }
            }
        }
    }
    lv.bake_light();
    Some(lv)
}

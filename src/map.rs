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

/// A tile sound does not reach (see `Level::sound_field`).
pub const SOUND_FAR: u16 = u16::MAX;

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

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    Wall,
    Floor,
    DoorClosed,
    DoorOpen,
    StairsUp,
    StairsDown,
    /// A wall of living sinew grown by the dungeon: cut through it.
    Sinew,
    /// A closed gate of bars (sewers, gaols, crypts, foundries): you can see
    /// through it, but not walk through until it is opened.
    GateClosed,
    /// A ward prism, floating over the floor and feeding the Warden's power.
    Ward,
    /// A wall of crystal raised by the Prism Guardian to seal its room.
    Crystal,
    /// A treasure chest (see `Level::chests`). Bump it to open it.
    Chest,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum WardState {
    /// Untouched, souls streaming from it.
    Intact,
    /// Its Guardian is loose; the prism is shielded until the Guardian falls.
    Shielded,
    /// Crumbled: only shards remain.
    Broken,
}

/// A treasure chest standing against a wall. Some of them are Mimics, and
/// nothing about one tells you so, except the creak you hear if you listen.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub struct Chest {
    pub x: i32,
    pub y: i32,
    pub open: bool,
    pub mimic: bool,
}

/// One of the four ward prisms that strengthen the Warden.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub struct Ward {
    pub x: i32,
    pub y: i32,
    /// The room it stands in, sealed when its Guardian wakes.
    pub room: Rect,
    pub state: WardState,
    /// The Guardian's monster id while it lives.
    pub guardian: Option<u32>,
    pub seen: bool,
}

/// Themes whose doors are gates of bars you can see through.
pub const GATE_STYLES: [u8; 4] = [6, 7, 8, 15];

impl Tile {
    pub fn opaque(self) -> bool {
        matches!(
            self,
            Tile::Wall | Tile::DoorClosed | Tile::Sinew | Tile::Crystal
        )
    }
    /// A door or gate that opens when bumped.
    pub fn is_closed_door(self) -> bool {
        matches!(self, Tile::DoorClosed | Tile::GateClosed)
    }
    /// Blocks a diagonal step around its corner.
    pub fn blocks_corner(self) -> bool {
        self.opaque() || self == Tile::GateClosed
    }
    pub fn walkable(self) -> bool {
        matches!(
            self,
            Tile::Floor | Tile::DoorOpen | Tile::StairsUp | Tile::StairsDown
        )
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
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
        self.x - m < o.x + o.w
            && self.x + self.w + m > o.x
            && self.y - m < o.y + o.h
            && self.y + self.h + m > o.y
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrapKind {
    Spikes,
    Gas,
    Fire,
    Alarm,
    /// A fanged mouth in the living floor.
    Maw,
}

/// Special rooms that give a floor some character.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoomKind {
    /// A hall of pillars.
    Pillars,
    /// Half fallen in, strewn with rubble.
    Rubble,
    /// A walled inner chamber with treasure behind its door.
    Chamber,
    /// Heaped bones, some of which twitch.
    Ossuary,
    /// Racks of old arms and armour.
    Armory,
    /// Shelves of mouldering scrolls.
    Library,
    /// A candlelit shrine with healing draughts.
    Shrine,
    /// A trapped gallery with a prize in it.
    TrapHall,
    /// A beasts' lair littered with gnawed bones.
    Den,
    /// Pale fungus whose spores hang in the air.
    Garden,
}

impl RoomKind {
    /// What you notice on first walking in, if anything.
    pub fn enter_text(self) -> Option<&'static str> {
        match self {
            RoomKind::Pillars | RoomKind::Rubble => None,
            RoomKind::Chamber => Some(
                "A walled chamber stands in the middle of this room. Something is kept inside.",
            ),
            RoomKind::Ossuary => {
                Some("Bones lie heaped in rows here: an ossuary. Some of them twitch.")
            }
            RoomKind::Armory => {
                Some("Racks of old weapons and rusted mail line the walls: an armory!")
            }
            RoomKind::Library => {
                Some("Rotting shelves sag under mouldering scrolls: a forgotten library.")
            }
            RoomKind::Shrine => {
                Some("A quiet shrine. Pale candles still burn, though no one tends them.")
            }
            RoomKind::TrapHall => {
                Some("The floor here is scored and stained, and something glints in the middle...")
            }
            RoomKind::Den => {
                Some("Gnawed bones and a reek of musk: you have walked into a beast's den.")
            }
            RoomKind::Garden => {
                Some("A garden of pale fungus. Spores hang thick in the air: tread carefully.")
            }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub struct Feature {
    pub kind: RoomKind,
    pub room: Rect,
    pub seen: bool,
}

/// A wall of living sinew and how much cutting it still takes.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub struct Sinew {
    pub x: i32,
    pub y: i32,
    pub hp: i32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Trap {
    pub x: i32,
    pub y: i32,
    pub kind: TrapKind,
    pub revealed: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Corpse {
    pub kind: usize,
    pub x: i32,
    pub y: i32,
    /// World time at which it rises again; negative = never.
    pub revive_at: f64,
    /// Times this body has served you as a thrall and fallen.
    #[serde(default)]
    pub raises: u8,
}

/// A drifting cloud of paralysing spores on one tile.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug)]
pub struct Cloud {
    pub x: i32,
    pub y: i32,
    /// World time at which it disperses.
    pub until: f64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct FloorItem {
    pub x: i32,
    pub y: i32,
    pub item: Item,
}

/// A wall-mounted torch at (x,y), lighting the floor tile (fx,fy).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Torch {
    pub x: i32,
    pub y: i32,
    pub fx: i32,
    pub fy: i32,
    pub color: [f32; 3],
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Level {
    pub w: i32,
    pub h: i32,
    pub depth: u32,
    pub style: u8,
    pub tiles: Vec<Tile>,
    pub explored: Vec<bool>,
    pub visible: Vec<bool>,
    pub blood: Vec<f32>,
    /// What each stain is made of (see `data::stain_colors`): blood, slime, rot...
    /// Older saves have none, and read as plain blood.
    #[serde(default)]
    pub stain: Vec<u8>,
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
    /// This floor's unique monster was placed here.
    pub has_unique: bool,
    /// A room packed with sleeping monsters.
    #[serde(default)]
    pub closet: Option<Rect>,
    /// The player has laid eyes on the monster closet.
    #[serde(default)]
    pub closet_found: bool,
    /// Spore clouds.
    #[serde(default)]
    pub clouds: Vec<Cloud>,
    /// Special rooms.
    #[serde(default)]
    pub features: Vec<Feature>,
    /// Walls of sinew the living dungeon has grown.
    #[serde(default)]
    pub sinews: Vec<Sinew>,
    /// A ward prism, if one stands on this floor.
    #[serde(default)]
    pub ward: Option<Ward>,
    /// Treasure chests on this floor (and Mimics pretending to be them).
    #[serde(default)]
    pub chests: Vec<Chest>,
    /// Crystal walls sealing the ward's room, with what they replaced.
    #[serde(default)]
    pub crystal: Vec<(i32, i32, Tile)>,
}

impl Level {
    pub fn chest_at(&self, x: i32, y: i32) -> Option<usize> {
        self.chests.iter().position(|c| c.x == x && c.y == y)
    }

    /// Which way a chest at (x,y) faces: away from the wall it stands
    /// against (0 = east, 1 = south, 2 = west, 3 = north).
    pub fn chest_facing(&self, x: i32, y: i32) -> u8 {
        let dirs = [(1, 0), (0, 1), (-1, 0), (0, -1)];
        for (k, &(dx, dy)) in dirs.iter().enumerate() {
            // The wall is behind it, so it faces the other way.
            if self.tile(x - dx, y - dy).opaque() {
                return k as u8;
            }
        }
        (((x * 7 + y * 13) % 4 + 4) % 4) as u8
    }

    /// What the stain at this cell is made of.
    pub fn stain_at(&self, i: usize) -> u8 {
        self.stain.get(i).copied().unwrap_or(0)
    }

    pub fn set_stain(&mut self, i: usize, kind: u8) {
        if self.stain.len() != self.blood.len() {
            self.stain.resize(self.blood.len(), 0);
        }
        if let Some(s) = self.stain.get_mut(i) {
            *s = kind;
        }
    }

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
            stain: vec![0; n],
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
            has_unique: false,
            closet: None,
            closet_found: false,
            clouds: Vec::new(),
            features: Vec::new(),
            sinews: Vec::new(),
            ward: None,
            chests: Vec::new(),
            crystal: Vec::new(),
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
            !self.tile(x + dx, y).blocks_corner() && !self.tile(x, y + dy).blocks_corner()
        } else {
            true
        }
    }

    /// Doors take the look of the floor's theme: in sewers, gaols, crypts and
    /// foundries they become gates of bars.
    pub fn theme_doors(&mut self) {
        if GATE_STYLES.contains(&self.style) {
            for t in self.tiles.iter_mut() {
                if *t == Tile::DoorClosed {
                    *t = Tile::GateClosed;
                }
            }
        }
    }

    pub fn cloud_at(&self, x: i32, y: i32) -> bool {
        self.clouds.iter().any(|c| c.x == x && c.y == y)
    }

    pub fn monster_at(&self, x: i32, y: i32) -> Option<usize> {
        self.monsters
            .iter()
            .position(|m| m.alive && m.x == x && m.y == y)
    }

    /// Could something walk the straight line from (x0,y0) to (x1,y1) in one
    /// rush? Every tile after the start must be open floor (a closed door or
    /// gate stops it, even though you can see through a gate's bars), and no
    /// step may cut a blocked corner.
    pub fn clear_walk(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
        let pts = line(x0, y0, x1, y1);
        pts.windows(2).all(|w| {
            let ((ax, ay), (bx, by)) = (w[0], w[1]);
            self.tile(bx, by).walkable() && self.can_step(ax, ay, bx - ax, by - ay)
        })
    }

    /// How sound spreads from (sx,sy): for each tile, how far the sound has
    /// effectively travelled to get there, in half-tiles. Every step costs
    /// two; bending around a corner costs more the sharper the turn; a closed
    /// door muffles it like three more tiles of corridor. Walls are not passed
    /// through, but each wall the sound strikes gets the cost of the floor
    /// beside it plus a step (that is where it bounces). Tiles the sound never
    /// reaches within `limit` hold `SOUND_FAR`.
    pub fn sound_field(&self, sx: i32, sy: i32, limit: u16) -> Vec<u16> {
        let n = (self.w * self.h) as usize;
        let mut best = vec![SOUND_FAR; n];
        if !self.inb(sx, sy) {
            return best;
        }
        // States are (tile, the direction the sound was moving in); 8 = none yet.
        let mut d = vec![SOUND_FAR; n * 9];
        let mut heap = BinaryHeap::new();
        let si = self.idx(sx, sy);
        d[si * 9 + 8] = 0;
        best[si] = 0;
        heap.push(Reverse((0u16, si, 8usize)));
        while let Some(Reverse((cd, i, dir))) = heap.pop() {
            if cd > d[i * 9 + dir] {
                continue;
            }
            let x = (i as i32) % self.w;
            let y = (i as i32) / self.w;
            for (k, &(dx, dy)) in DIRS.iter().enumerate() {
                let (nx, ny) = (x + dx, y + dy);
                if !self.inb(nx, ny) || !self.can_step(x, y, dx, dy) {
                    continue;
                }
                let ni = self.idx(nx, ny);
                let t = self.tile(nx, ny);
                let turn = if dir == 8 {
                    0
                } else {
                    let a = (k as i32 - dir as i32).rem_euclid(8);
                    a.min(8 - a)
                };
                let bend = [0u16, 1, 3, 5, 5][turn as usize];
                let nd = cd.saturating_add(2 + bend);
                if !(t.walkable() || t.is_closed_door()) {
                    // The sound strikes this wall and rings back.
                    if t.opaque() && nd < best[ni] {
                        best[ni] = nd;
                    }
                    continue;
                }
                let nd = if t.is_closed_door() { nd.saturating_add(6) } else { nd };
                if nd > limit || nd >= d[ni * 9 + k] {
                    continue;
                }
                d[ni * 9 + k] = nd;
                if nd < best[ni] {
                    best[ni] = nd;
                }
                heap.push(Reverse((nd, ni, k)));
            }
        }
        best
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
            let targets = [
                (px + i, py - r),
                (px + i, py + r),
                (px - r, py + i),
                (px + r, py + i),
            ];
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
                        if self.inb(nx, ny)
                            && self.tile(nx, ny).opaque()
                            && !self.visible[self.idx(nx, ny)]
                        {
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
                    Tile::Wall | Tile::Sinew | Tile::Crystal | Tile::Ward | Tile::Chest => continue,
                    Tile::DoorClosed | Tile::GateClosed => 3,
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

/// Is every walkable tile reachable from (sx,sy)?
fn connected(lv: &Level, sx: i32, sy: i32) -> bool {
    let dm = lv.dijkstra(sx, sy);
    lv.tiles
        .iter()
        .enumerate()
        .all(|(i, t)| !t.walkable() || dm[i] < UNREACH)
}

/// Give some rooms a purpose: pillared halls, rubble, walled chambers, and
/// rooms whose contents are filled in later (armories, ossuaries, dens...).
/// A room is put back as it was if its new walls would cut anything off.
fn decorate_rooms(lv: &mut Level, rooms: &[Rect], round: &[bool], depth: u32, rng: &mut StdRng) {
    let start = rooms[0].center();
    for (i, r) in rooms.iter().enumerate().skip(1) {
        if !rng.gen_bool(0.55) {
            continue;
        }
        let mut kinds = vec![
            RoomKind::Ossuary,
            RoomKind::Armory,
            RoomKind::Library,
            RoomKind::Shrine,
            RoomKind::Den,
        ];
        if !round[i] {
            kinds.push(RoomKind::Rubble);
            if r.w >= 6 && r.h >= 5 {
                kinds.push(RoomKind::Pillars);
                kinds.push(RoomKind::TrapHall);
            }
            if r.w >= 7 && r.h >= 7 {
                kinds.push(RoomKind::Chamber);
                kinds.push(RoomKind::Chamber);
            }
        }
        if depth >= 3 {
            kinds.push(RoomKind::Garden);
        }
        let kind = kinds[rng.gen_range(0..kinds.len())];
        let saved: Vec<(i32, i32, Tile)> = (r.y - 1..=r.y + r.h)
            .flat_map(|y| (r.x - 1..=r.x + r.w).map(move |x| (x, y)))
            .map(|(x, y)| (x, y, lv.tile(x, y)))
            .collect();
        let c = r.center();
        let interior = |x: i32, y: i32| {
            x > r.x && x < r.x + r.w - 1 && y > r.y && y < r.y + r.h - 1 && (x, y) != c
        };
        match kind {
            RoomKind::Pillars => {
                for y in (r.y + 1..r.y + r.h - 1).step_by(2) {
                    for x in (r.x + 1..r.x + r.w - 1).step_by(2) {
                        if interior(x, y) && lv.tile(x, y) == Tile::Floor {
                            lv.set(x, y, Tile::Wall);
                        }
                    }
                }
            }
            RoomKind::Rubble => {
                for y in r.y + 1..r.y + r.h - 1 {
                    for x in r.x + 1..r.x + r.w - 1 {
                        if interior(x, y) && lv.tile(x, y) == Tile::Floor && rng.gen_bool(0.18) {
                            lv.set(x, y, Tile::Wall);
                        }
                    }
                }
            }
            RoomKind::Chamber => {
                let (x0, y0, x1, y1) = (r.x + 1, r.y + 1, r.x + r.w - 2, r.y + r.h - 2);
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        if x == x0 || x == x1 || y == y0 || y == y1 {
                            lv.set(x, y, Tile::Wall);
                        } else {
                            lv.set(x, y, Tile::Floor);
                        }
                    }
                }
                let door = match rng.gen_range(0..4) {
                    0 => ((x0 + x1) / 2, y0),
                    1 => ((x0 + x1) / 2, y1),
                    2 => (x0, (y0 + y1) / 2),
                    _ => (x1, (y0 + y1) / 2),
                };
                lv.set(door.0, door.1, Tile::DoorClosed);
            }
            _ => {}
        }
        if !connected(lv, start.0, start.1) {
            for (x, y, t) in saved {
                lv.set(x, y, t);
            }
            continue;
        }
        lv.features.push(Feature {
            kind,
            room: *r,
            seen: false,
        });
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

    let mut round = vec![false; rooms.len()];
    for i in 0..rooms.len() {
        let r = rooms[i];
        round[i] = i > 0 && r.w >= 6 && r.h >= 6 && rng.gen_bool(0.3);
        let round = round[i];
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
        if !round && r.w >= 8 && r.h >= 6 && rng.gen_bool(0.25) {
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

    decorate_rooms(&mut lv, &rooms, &round, depth, rng);

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
                    lv.torches.push(Torch {
                        x: wx,
                        y: wy,
                        fx,
                        fy,
                        color: [c[0] * var, c[1] * var, c[2] * var],
                    });
                    break;
                }
            }
        }
    }
    // Shrines burn with pale candles.
    let shrines: Vec<Rect> = lv
        .features
        .iter()
        .filter(|f| f.kind == RoomKind::Shrine)
        .map(|f| f.room)
        .collect();
    for r in shrines {
        let spots = [
            (r.x + r.w / 2, r.y - 1, r.x + r.w / 2, r.y),
            (r.x + r.w / 2, r.y + r.h, r.x + r.w / 2, r.y + r.h - 1),
            (r.x - 1, r.y + r.h / 2, r.x, r.y + r.h / 2),
            (r.x + r.w, r.y + r.h / 2, r.x + r.w - 1, r.y + r.h / 2),
        ];
        for (wx, wy, fx, fy) in spots {
            if lv.tile(wx, wy) == Tile::Wall
                && lv.tile(fx, fy) == Tile::Floor
                && !lv.torches.iter().any(|t| t.x == wx && t.y == wy)
            {
                lv.torches.push(Torch {
                    x: wx,
                    y: wy,
                    fx,
                    fy,
                    color: [0.6, 0.7, 1.1],
                });
            }
        }
    }
    lv.bake_light();
    Some(lv)
}

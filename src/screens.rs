//! Full-screen scenes: title, class selection, Soul Altar, Hall of the Dead,
//! the animated death screens (one per manner of death) and the victory.

use crate::canvas::Canvas;
use crate::color::{hash2, hashf, Rgb};
use crate::data::*;
use crate::game::{a_name, DeathCause};
use crate::map::TrapKind;
use crate::profile::Profile;
use rand::Rng;
use std::f32::consts::{PI, TAU};

pub const LOGO: &[&str] = &[
    r"  ____ ____      _     _________    _    ____  _  __",
    r" / ___|  _ \    / \   |__  /__  /  / \  |  _ \| |/ /",
    r"| |  _| |_) |  / _ \    / /  / /  / _ \ | |_) | ' / ",
    r"| |_| |  _ <  / ___ \  / /_ / /_ / ___ \|  _ <| . \ ",
    r" \____|_| \_\/_/   \_\/____/____/_/   \_\_| \_\_|\_\",
];

pub const TITLE_ITEMS: [&str; 7] = ["Continue Descent", "New Descent", "Soul Altar", "Bestiary", "Hall of the Dead", "How to Play", "Quit"];

// ================================================================ fire

/// Classic cellular fire simulation.
pub struct Fire {
    w: usize,
    h: usize,
    heat: Vec<u8>,
}

impl Fire {
    pub fn new() -> Fire {
        Fire { w: 0, h: 0, heat: Vec::new() }
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        if self.w != w || self.h != h {
            self.w = w;
            self.h = h;
            self.heat = vec![0; w * h];
        }
    }

    /// `decay_max` controls flame height: about 72/decay_max rows.
    pub fn step(&mut self, on: bool, decay_max: u8) {
        let (w, h) = (self.w, self.h);
        if w == 0 || h < 2 {
            return;
        }
        let mut rng = rand::thread_rng();
        for x in 0..w {
            let i = (h - 1) * w + x;
            self.heat[i] = if on { 36 - rng.gen_range(0..5) } else { self.heat[i].saturating_sub(3) };
        }
        for y in 1..h {
            for x in 0..w {
                let src = y * w + x;
                let p = self.heat[src];
                if p == 0 {
                    self.heat[src - w] = 0;
                    continue;
                }
                let r = rng.gen_range(0..3i32);
                let dx = (x as i32 - r + 1).clamp(0, w as i32 - 1) as usize;
                let decay = rng.gen_range(0..=decay_max);
                self.heat[(y - 1) * w + dx] = p.saturating_sub(decay);
            }
        }
    }

    /// tint: 0 fire, 1 ghost-green, 2 ash-grey, 3 sickly purple
    pub fn draw(&self, c: &mut Canvas, x0: i32, y0: i32, tint: u8) {
        const RAMP: [char; 8] = [' ', '.', ':', '*', 's', '%', '#', '@'];
        for y in 0..self.h {
            for x in 0..self.w {
                let v = self.heat[y * self.w + x];
                if v < 2 {
                    continue;
                }
                let f = v as f32 / 36.0;
                let col = fire_color(f, tint);
                let ch = RAMP[((f * 7.99) as usize).min(7)];
                c.set(x0 + x as i32, y0 + y as i32, ch, col, col.scale(0.35));
            }
        }
    }
}

pub fn fire_color(f: f32, tint: u8) -> Rgb {
    let base = if f < 0.33 {
        Rgb(0, 0, 0).lerp(Rgb(180, 20, 0), f / 0.33)
    } else if f < 0.66 {
        Rgb(180, 20, 0).lerp(Rgb(255, 140, 0), (f - 0.33) / 0.33)
    } else {
        Rgb(255, 140, 0).lerp(Rgb(255, 255, 200), (f - 0.66) / 0.34)
    };
    match tint {
        1 => Rgb(base.1 / 2, base.0, base.1 / 2 + base.2 / 2),
        2 => {
            let l = (base.luma() * 210.0) as u8;
            Rgb(l, l, l.saturating_add(10))
        }
        3 => Rgb(base.0 / 2 + base.1 / 4, base.2 / 2, base.0),
        _ => base,
    }
}

// ================================================================ title

pub fn draw_title(c: &mut Canvas, fire: &Fire, t: f32, sel: usize, prof: &Profile, has_save: bool) {
    let w = c.w as i32;
    let h = c.h as i32;
    for y in 0..h {
        let k = y as f32 / h as f32;
        let bg = Rgb(6, 3, 10).lerp(Rgb(30, 6, 4), k * k);
        for x in 0..w {
            c.set(x, y, ' ', Rgb::WHITE, bg);
        }
    }
    let fh = (h / 3).max(6);
    fire.draw(c, 0, h - fh, 0);

    let lw = LOGO[0].chars().count() as i32;
    let lx = (w - lw) / 2;
    let ly = (h / 6).max(1);
    for (r, line) in LOGO.iter().enumerate() {
        for (i, ch) in line.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let wave = ((i as f32 * 0.18) - t * 2.2 + r as f32 * 0.3).sin() * 0.5 + 0.5;
            let col = Rgb(150, 15, 10).lerp(Rgb(255, 190, 70), wave);
            c.set(lx + i as i32, ly + r as i32, ch, col, Rgb(25, 4, 2));
        }
    }
    c.center(ly + 6, "~ the Descent for the Amulet ~", Rgb(200, 170, 130));

    let my = ly + 9;
    let first = if has_save { 0 } else { 1 };
    for (i, item) in TITLE_ITEMS.iter().enumerate().skip(first) {
        let y = my + (i - first) as i32 * 2;
        if i == sel {
            let s = format!(">>  {}  <<", item);
            let col = Rgb(255, 230, 150).lerp(Rgb::WHITE, 0.5 + 0.5 * (t * 4.0).sin());
            c.center(y, &s, col);
        } else {
            c.center(y, item, Rgb(170, 150, 140));
        }
    }
    let stats = format!(
        "Soul Points: {}    Descents: {}    Victories: {}    Deepest: {}",
        prof.points, prof.runs, prof.wins, prof.best_depth
    );
    let rows = (TITLE_ITEMS.len() - first) as i32;
    c.center(my + rows * 2 + 1, &stats, Rgb(190, 140, 255));
    if has_save && sel == 1 {
        c.center(my + rows * 2 + 2, "(a new descent abandons your saved run)", Rgb(200, 120, 100));
    }
    c.center(h - 1, "Up/Down + Enter to choose", Rgb(120, 100, 90));
}

// ================================================================ class select

pub fn draw_class_select(c: &mut Canvas, sel: usize, prof: &Profile, t: f32) {
    let w = c.w as i32;
    let h = c.h as i32;
    c.fill(0, 0, w, h, ' ', Rgb::WHITE, Rgb(8, 6, 12));
    c.center(1, "CHOOSE YOUR CHAMPION", Rgb(255, 200, 120));
    let lx = 3;
    for (i, cd) in CLASSES.iter().enumerate() {
        let y = 4 + i as i32 * 2;
        let unlocked = prof.class_unlocked(cd);
        let label = if unlocked { cd.name.to_string() } else { format!("{} (locked)", cd.name) };
        if i == sel {
            let col = cd.color.lerp(Rgb::WHITE, 0.3 + 0.2 * (t * 4.0).sin());
            c.text(lx, y, &format!("> {}", label), col);
        } else {
            c.text(lx + 2, y, &label, if unlocked { cd.color.scale(0.8) } else { Rgb(90, 90, 90) });
        }
    }
    let cd = &CLASSES[sel];
    let px = 30.min(w / 3);
    let pw = w - px - 3;
    c.frame(px, 3, pw, h - 6, cd.color.scale(0.6), Rgb(14, 10, 18));
    let mut y = 5;
    c.text(px + 2, y, cd.name, cd.color);
    y += 2;
    for line in wrap(cd.desc, (pw - 4) as usize) {
        c.text(px + 2, y, &line, Rgb(210, 210, 210));
        y += 1;
    }
    y += 1;
    let b = prof.bonuses();
    c.text(px + 2, y, &format!("Health {}   Mana {}   Accuracy +{}", cd.hp + b.hp, cd.mp + b.mp, cd.hit), Rgb(200, 180, 160));
    y += 1;
    c.text(px + 2, y, &format!("Weapon: {}   Armor: {}", WEAPONS[cd.weapon].name, ARMORS[cd.armor].name), Rgb(180, 180, 200));
    y += 2;
    c.text(px + 2, y, "Skills:", Rgb(255, 220, 140));
    y += 1;
    for (i, s) in cd.skills.iter().enumerate() {
        c.text(px + 4, y, &format!("[{}] {} ({} mp) - {}", i + 1, s.name(), s.mp(), s.desc()), Rgb(170, 230, 170));
        y += 1;
    }
    let mut slot = 3;
    for u in UPGRADES.iter().filter(|u| u.cat == UCat::Skill(sel)) {
        let Some(s) = u.skill else { continue };
        if b.skills.contains(&s) {
            c.text(px + 4, y, &format!("[{}] {} ({} mp) - {}", slot, s.name(), s.mp(), s.desc()), Rgb(200, 160, 255));
            slot += 1;
        } else {
            c.text(px + 4, y, &format!("[-] {} - learn at the Soul Altar ({} pts)", s.name(), u.cost), Rgb(100, 90, 110));
        }
        y += 1;
    }
    y += 1;
    if !prof.class_unlocked(cd) {
        let cost = UPGRADES.iter().find(|u| Some(u.id) == cd.unlock).map(|u| u.cost).unwrap_or(0);
        c.text(px + 2, y, &format!("LOCKED - unlock at the Soul Altar for {} soul points.", cost), Rgb(255, 100, 100));
    } else {
        c.text(px + 2, y, "Press Enter to choose a difficulty.", Rgb(255, 230, 150));
    }
    c.center(h - 2, "Up/Down: choose   Enter: next   Esc: back", Rgb(120, 110, 100));
}

pub fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if !cur.is_empty() && cur.len() + 1 + word.len() > width {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ================================================================ altar

/// Heading for an altar section.
fn ucat_title(cat: UCat) -> String {
    match cat {
        UCat::Stat => "STRENGTHS".to_string(),
        UCat::Boon => "BOONS".to_string(),
        UCat::Class => "CLASSES".to_string(),
        UCat::Skill(k) => format!("{} SKILLS", CLASSES[k].name.to_uppercase()),
    }
}

pub fn draw_altar(c: &mut Canvas, sel: usize, prof: &Profile, t: f32, note: &str, fire: &Fire) {
    let w = c.w as i32;
    let h = c.h as i32;
    c.fill(0, 0, w, h, ' ', Rgb::WHITE, Rgb(8, 4, 14));
    let fh = (h / 5).max(4);
    fire.draw(c, 0, h - fh, 3);
    c.center(1, "~ THE SOUL ALTAR ~", Rgb(200, 150, 255).lerp(Rgb::WHITE, 0.3 + 0.3 * (t * 2.0).sin()));
    c.center(2, &format!("Soul Points: {}", prof.points), Rgb(230, 200, 255));

    // Rows: Some(i) is upgrade i, None(k) a section heading for upgrade k.
    let mut rows: Vec<(bool, usize)> = Vec::new();
    for (i, u) in UPGRADES.iter().enumerate() {
        if i == 0 || UPGRADES[i - 1].cat != u.cat {
            rows.push((false, i));
        }
        rows.push((true, i));
    }
    let top = 4;
    let avail = (h - top - fh - 4).max(4);
    let sel_row = rows.iter().position(|&(it, i)| it && i == sel).unwrap_or(0) as i32;
    let start = if sel_row - 1 >= avail { sel_row - avail + 1 } else { 0 }.max(0);
    let lx = 3;
    for (row, &(is_item, i)) in rows.iter().enumerate().skip(start as usize) {
        let y = top + row as i32 - start;
        if y >= top + avail {
            break;
        }
        let u = &UPGRADES[i];
        if !is_item {
            let col = match u.cat {
                UCat::Skill(k) => CLASSES[k].color,
                _ => Rgb(190, 150, 255),
            };
            c.text(lx, y, &format!("-- {} --", ucat_title(u.cat)), col);
            continue;
        }
        let r = prof.rank(u.id);
        let mut pips = String::from("[");
        for k in 0..u.max {
            pips.push(if k < r { '#' } else { '-' });
        }
        pips.push(']');
        let cost = match prof.next_cost(i) {
            Some(cst) => format!("{:>5} pts", cst),
            None => "   MAX   ".to_string(),
        };
        let line = format!("{:<22} {:<8} {}", u.title(), pips, cost);
        let affordable = prof.next_cost(i).map_or(false, |cst| cst <= prof.points);
        if i == sel {
            c.text_bg(lx, y, &format!("> {}", line), Rgb::BLACK, Rgb(200, 150, 255));
        } else {
            let col = if r >= u.max {
                Rgb(255, 215, 100)
            } else if affordable {
                Rgb(210, 210, 230)
            } else {
                Rgb(110, 100, 120)
            };
            c.text(lx + 2, y, &line, col);
        }
    }
    if start > 0 {
        c.text(lx + 48, top, "^ more", Rgb(140, 120, 160));
    }
    if (rows.len() as i32 - start) > avail {
        c.text(lx + 48, top + avail - 1, "v more", Rgb(140, 120, 160));
    }
    let u = &UPGRADES[sel];
    let dy = h - fh - 2;
    let mut desc = u.text();
    if let UCat::Skill(k) = u.cat {
        let cd = &CLASSES[k];
        desc = format!("{} only. {}", cd.name, desc);
        if !prof.class_unlocked(cd) {
            desc.push_str(&format!("  ({} is not unlocked yet.)", cd.name));
        }
    }
    let lines = wrap(&desc, (w - lx * 2).max(10) as usize);
    for (k, line) in lines.iter().take(2).enumerate() {
        c.text(lx, dy + k as i32, line, Rgb(220, 200, 255));
    }
    if !note.is_empty() {
        c.text(lx, dy - 1, note, Rgb(255, 230, 150));
    }
    c.center(h - 1, "Up/Down: choose   Enter: buy   Esc: back", Rgb(160, 140, 180));
}

// ================================================================ difficulty

pub fn draw_difficulty(c: &mut Canvas, sel: usize, class: usize, prof: &Profile, t: f32) {
    let w = c.w as i32;
    let h = c.h as i32;
    c.fill(0, 0, w, h, ' ', Rgb::WHITE, Rgb(10, 5, 8));
    let cd = &CLASSES[class];
    c.center(1, "CHOOSE YOUR FATE", Rgb(255, 170, 110));
    c.center(2, &format!("as the {}", cd.name), cd.color);
    let lx = 4;
    for (i, d) in DIFFICULTIES.iter().enumerate() {
        let y = 5 + i as i32 * 2;
        let open = prof.diff_unlocked(i);
        let label = if open { d.name.to_string() } else { format!("{} (locked)", d.name) };
        if i == sel {
            let col = if open { d.color.lerp(Rgb::WHITE, 0.3 + 0.2 * (t * 4.0).sin()) } else { Rgb(150, 110, 110) };
            c.text(lx, y, &format!("> {}", label), col);
        } else {
            c.text(lx + 2, y, &label, if open { d.color.scale(0.8) } else { Rgb(90, 80, 80) });
        }
    }
    let d = &DIFFICULTIES[sel];
    let px = 26.min(w / 3);
    let pw = w - px - 3;
    c.frame(px, 4, pw, 13, d.color.scale(0.6), Rgb(16, 8, 12));
    let mut y = 6;
    c.text(px + 2, y, d.name, d.color);
    y += 2;
    for line in wrap(d.desc, (pw - 4) as usize) {
        c.text(px + 2, y, &line, Rgb(210, 210, 210));
        y += 1;
    }
    y += 1;
    c.text(px + 2, y, &format!("Monster health  x{:.1}", d.hp), Rgb(230, 160, 160));
    y += 1;
    c.text(px + 2, y, &format!("Monster damage  x{:.1}", d.dmg), Rgb(230, 160, 160));
    y += 1;
    c.text(px + 2, y, &format!("Soul points     x{:.1}", d.souls), Rgb(200, 160, 255));
    y += 2;
    if prof.diff_unlocked(sel) {
        c.text(px + 2, y, "Press Enter to begin your descent.", Rgb(255, 230, 150));
    } else {
        let prev = DIFFICULTIES[sel - 1].name;
        c.text(px + 2, y, &format!("LOCKED - escape with the Amulet on {} to unlock.", prev), Rgb(255, 100, 100));
    }
    c.center(h - 2, "Up/Down: choose   Enter: begin   Esc: back", Rgb(120, 110, 100));
}

// ================================================================ bestiary

fn speed_word(ticks: f64) -> &'static str {
    match ticks as i32 {
        i32::MIN..=70 => "very fast",
        71..=89 => "fast",
        90..=105 => "average",
        106..=125 => "slow",
        _ => "very slow",
    }
}

fn faction_word(f: crate::entity::Faction) -> &'static str {
    use crate::entity::Faction::*;
    match f {
        Vermin => "vermin",
        Beast => "beast",
        Greenskin => "greenskin",
        Undead => "undead",
        Dark => "dark folk",
        Demon => "demon",
        _ => "other",
    }
}

pub fn draw_bestiary(c: &mut Canvas, sel: usize, prof: &Profile, t: f32) {
    let w = c.w as i32;
    let h = c.h as i32;
    c.fill(0, 0, w, h, ' ', Rgb::WHITE, Rgb(8, 8, 6));
    c.center(1, "THE BESTIARY", Rgb(230, 210, 150));
    let known = MONSTERS.iter().filter(|m| prof.kills_of(m.name) >= LORE_NAME).count();
    let stars = MONSTERS.iter().filter(|m| prof.kills_of(m.name) >= LORE_MASTER).count();
    c.center(2, &format!("{} of {} creatures known   {} mastered", known, MONSTERS.len(), stars), Rgb(160, 150, 120));

    let lw = 38.min(w / 2 - 2);
    let top = 4;
    let rows = (h - top - 2).max(4);
    let start = if sel as i32 >= rows { sel as i32 - rows + 1 } else { 0 };
    for (row, i) in (start as usize..MONSTERS.len()).enumerate() {
        if row as i32 >= rows {
            break;
        }
        let m = &MONSTERS[i];
        let k = prof.kills_of(m.name);
        let y = top + row as i32;
        let star = if k >= LORE_MASTER { '*' } else { ' ' };
        let name = if k >= LORE_NAME { m.name.to_string() } else { "???".to_string() };
        let name: String = name.chars().take((lw - 10).max(4) as usize).collect();
        let line = format!("{}{:<w$}{:>6}", star, name, k, w = (lw - 8).max(4) as usize);
        if i == sel {
            c.text_bg(1, y, &format!(" {}", line), Rgb::BLACK, Rgb(220, 200, 140));
        } else {
            let col = if k >= LORE_MASTER {
                Rgb(255, 215, 90)
            } else if k >= LORE_NAME {
                m.color.lerp(Rgb::WHITE, 0.3)
            } else {
                Rgb(80, 76, 70)
            };
            c.text(2, y, &line, col);
        }
    }

    // Detail panel.
    let m = &MONSTERS[sel];
    let k = prof.kills_of(m.name);
    let px = lw + 3;
    let pw = w - px - 2;
    if pw < 20 {
        return;
    }
    c.frame(px, top - 1, pw, h - top, Rgb(120, 110, 80), Rgb(14, 13, 10));
    let tx = px + 2;
    let tw = (pw - 4) as usize;
    let mut y = top;
    if k < LORE_NAME {
        c.text(tx, y, "An unknown horror.", Rgb(150, 140, 120));
        c.text(tx, y + 2, "Slay one to learn its name.", Rgb(120, 110, 100));
        return;
    }
    let title_col = if k >= LORE_MASTER { Rgb(255, 215, 90).lerp(Rgb::WHITE, 0.3 + 0.3 * (t * 3.0).sin()) } else { m.color };
    let star = if k >= LORE_MASTER { "* " } else { "" };
    c.text(tx, y, &format!("{}{}", star, cap_first(m.name)), title_col);
    c.text(tx + pw - 18, y, &format!("{:>5} slain", k), Rgb(180, 170, 140));
    y += 2;
    // Portrait.
    for line in m.art.iter() {
        if y >= top + 12 {
            break;
        }
        c.text(tx + 2, y, line, m.color);
        y += 1;
    }
    y += 1;
    let next = |n: u32| format!("({} more kills to learn more)", n.saturating_sub(k));
    if k < LORE_BASIC {
        c.text(tx, y, &next(LORE_BASIC), Rgb(120, 110, 100));
        return;
    }
    let hp_txt = if m.hp > 0 { m.hp.to_string() } else { "?".into() };
    c.text(tx, y, &format!("Health {}   Damage {}-{}   Armour {}   Speed: {}", hp_txt, m.dmg.0, m.dmg.1, m.ac, speed_word(m.speed)), Rgb(220, 200, 160));
    y += 1;
    c.text(tx, y, &format!("Kind: {}", faction_word(m.faction)), Rgb(190, 180, 150));
    y += 2;
    if k < LORE_FULL {
        c.text(tx, y, &next(LORE_FULL), Rgb(120, 110, 100));
        return;
    }
    let lore = MONSTER_LORE.get(sel).copied().unwrap_or("");
    for line in wrap(lore, tw) {
        if y >= h - 6 {
            break;
        }
        c.text(tx, y, &line, Rgb(200, 195, 180));
        y += 1;
    }
    y += 1;
    let depth = if m.boss {
        "depth 10, guarding the Amulet".to_string()
    } else if m.unique > 0 {
        format!("depth {} only (a unique)", m.unique)
    } else {
        format!("depths {}-{}", m.depth.0, m.depth.1)
    };
    let mut adv = vec![
        format!("Found on {}.  Accuracy +{}.  Sight {}.  {} XP.", depth, m.hit, m.sight, m.xp),
        if m.revive.1 > 0.0 {
            format!("Rises again {:.0}-{:.0} turns after death.", m.revive.0, m.revive.1)
        } else {
            "Stays dead once slain.".to_string()
        },
    ];
    let mut traits: Vec<String> = Vec::new();
    if let Some(r) = &m.ranged {
        traits.push(format!("{} ({}-{}, range {})", r.verb, r.dmg.0, r.dmg.1, r.range));
    }
    if m.poison {
        traits.push("poisonous".into());
    }
    if m.regen {
        traits.push("regenerates".into());
    }
    if m.crush {
        traits.push("crushing blows".into());
    }
    if m.ghost {
        traits.push("passes through walls".into());
    }
    if m.erratic {
        traits.push("erratic flight".into());
    }
    for a in m.abil.iter() {
        traits.push(a.describe());
    }
    if !traits.is_empty() {
        adv.push(format!("Traits: {}.", traits.join(", ")));
    }
    for para in adv {
        for line in wrap(&para, tw) {
            if y >= h - 3 {
                break;
            }
            c.text(tx, y, &line, Rgb(170, 200, 170));
            y += 1;
        }
    }
    y += 1;
    if k < LORE_MASTER {
        if y < h - 2 {
            c.text(tx, y, &format!("({} more kills to master it)", LORE_MASTER - k), Rgb(120, 110, 100));
        }
    } else if y < h - 2 {
        c.text(tx, y, "MASTERED: +1.5% crit chance and +2% crit damage against it.", Rgb(255, 215, 90));
    }
}

fn cap_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// ================================================================ hall of the dead

pub fn draw_hall(c: &mut Canvas, prof: &Profile) {
    let w = c.w as i32;
    let h = c.h as i32;
    c.fill(0, 0, w, h, ' ', Rgb::WHITE, Rgb(6, 6, 10));
    c.center(1, "HALL OF THE DEAD", Rgb(200, 200, 230));
    c.center(2, &format!("{} descents, {} victories, {} soul points earned in total", prof.runs, prof.wins, prof.total_points), Rgb(140, 140, 170));
    if prof.history.is_empty() {
        c.center(5, "No one has yet entered the dungeon of GRAZZARK.", Rgb(150, 150, 150));
    }
    for (i, r) in prof.history.iter().rev().enumerate() {
        let y = 4 + i as i32;
        if y >= h - 2 {
            break;
        }
        let line = format!(
            "{:<12} {:<9} lv{:<3} depth {:<3} {:>5} pts   {}",
            r.class,
            DIFFICULTIES[(r.difficulty as usize).min(DIFFICULTIES.len() - 1)].name,
            r.level,
            r.depth,
            r.points,
            if r.won { "ESCAPED WITH THE AMULET!".to_string() } else { r.cause.clone() }
        );
        let s: String = line.chars().take((w - 4).max(0) as usize).collect();
        c.text(2, y, &s, if r.won { Rgb(255, 220, 100) } else { Rgb(190, 170, 160) });
    }
    c.center(h - 1, "Esc: back", Rgb(120, 120, 140));
}

// ================================================================ death

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeathFx {
    Fire,
    Ash,
    BloodRain,
    GhostMist,
    Toxic,
    Cracks,
    Spikes,
    Arcane,
}

struct SP {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    ch: char,
    c: Rgb,
    life: f32,
}

pub struct DeathScreen {
    pub kind: DeathFx,
    pub title: String,
    pub epitaph: String,
    pub info: Vec<String>,
    pub t: f32,
    fire: Fire,
    parts: Vec<SP>,
    pool: Vec<f32>,
    cracks: Vec<(i32, i32, char)>,
    heads: Vec<(f32, f32, f32)>,
    w: usize,
    h: usize,
}

const TOMB: &[&str] = &[
    r"      ____________________      ",
    r"     /                    \     ",
    r"    /      R . I . P .     \    ",
    r"   /                        \   ",
    r"  |                          |  ",
    r"  |                          |  ",
    r"  |                          |  ",
    r"  |                          |  ",
    r"  |                          |  ",
    r"  |                          |  ",
    r"  |                          |  ",
    r" _|__________________________|_ ",
    r"|______________________________|",
];

impl DeathScreen {
    pub fn new(cause: &DeathCause, info: Vec<String>) -> DeathScreen {
        use DeathFx::*;
        let (kind, title, epitaph): (DeathFx, &str, String) = match cause {
            DeathCause::Monster(k) => {
                let n = a_name(*k);
                match MONSTERS[*k].death {
                    DeathKind::Beast => (BloodRain, "DEVOURED", format!("Torn apart by {}.", n)),
                    DeathKind::Undead => (GhostMist, "JOINED THE RESTLESS DEAD", format!("Slain by {}. Your bones will rise again...", n)),
                    DeathKind::Brute => (BloodRain, "SLAIN IN BATTLE", format!("Cut down by {}.", n)),
                    DeathKind::Crushed => (Cracks, "CRUSHED", format!("Pulverized by {}.", n)),
                    DeathKind::Demon => (Fire, "CLAIMED BY THE ABYSS", format!("Dragged into the abyss by {}.", n)),
                }
            }
            DeathCause::Spell(k) => (Arcane, "UNMADE BY SORCERY", format!("Blasted apart by the magic of {}.", a_name(*k))),
            DeathCause::OwnFire => (Ash, "HOIST BY YOUR OWN FIREBALL", "Burned to ash by your own spell. The dungeon laughs.".to_string()),
            DeathCause::Poison(s) => (Toxic, "POISONED", format!("Succumbed to the venom of {}.", s)),
            DeathCause::Trap(TrapKind::Spikes) => (Spikes, "IMPALED", "Skewered on a hidden spike trap.".to_string()),
            DeathCause::Trap(TrapKind::Gas) => (Toxic, "CHOKED", "Choked to death on poison gas.".to_string()),
            DeathCause::Trap(TrapKind::Fire) => (Fire, "IMMOLATED", "Consumed by a roaring fire trap.".to_string()),
            DeathCause::Trap(TrapKind::Alarm) => (Cracks, "OVERWHELMED", "The alarm summoned your doom.".to_string()),
            DeathCause::Abandoned => (GhostMist, "LOST TO MADNESS", "Fled screaming into the dark, never to be seen again.".to_string()),
        };
        DeathScreen {
            kind,
            title: title.to_string(),
            epitaph,
            info,
            t: 0.0,
            fire: self::Fire::new(),
            parts: Vec::new(),
            pool: Vec::new(),
            cracks: Vec::new(),
            heads: Vec::new(),
            w: 0,
            h: 0,
        }
    }

    pub fn update(&mut self, dt: f32, w: usize, h: usize) {
        self.t += dt;
        if self.w != w || self.h != h {
            self.w = w;
            self.h = h;
            self.pool = vec![0.0; w];
            self.cracks.clear();
            self.heads.clear();
            self.parts.clear();
        }
        let mut rng = rand::thread_rng();
        let (wf, hf) = (w as f32, h as f32);
        match self.kind {
            DeathFx::Fire | DeathFx::Ash => {
                self.fire.resize(w, h);
                self.fire.step(self.t < 6.0 || self.kind == DeathFx::Fire, 2);
            }
            DeathFx::BloodRain => {
                for _ in 0..(w / 8).max(2) {
                    self.parts.push(SP { x: rng.gen::<f32>() * wf, y: 0.0, vx: 0.0, vy: rng.gen_range(18.0..34.0), ch: if rng.gen_bool(0.5) { '|' } else { ':' }, c: Rgb(rng.gen_range(140..220), 0, 0), life: 5.0 });
                }
                for p in self.parts.iter_mut() {
                    p.y += p.vy * dt;
                    p.x += p.vx * dt;
                    p.life -= dt;
                    let xi = (p.x as usize).min(w.saturating_sub(1));
                    if p.vy > 0.0 && !self.pool.is_empty() && p.y >= hf - 1.0 - self.pool[xi] {
                        self.pool[xi] = (self.pool[xi] + 0.06).min(hf / 4.0);
                        p.life = 0.0;
                    }
                }
            }
            DeathFx::GhostMist | DeathFx::Toxic => {
                let n = (w / 12).max(2);
                for _ in 0..n {
                    let toxic = self.kind == DeathFx::Toxic;
                    let ch = if toxic { ['o', 'O', '.', 'o'][rng.gen_range(0..4)] } else { ['~', 'o', '.', '\''][rng.gen_range(0..4)] };
                    let c = if toxic { Rgb(rng.gen_range(60..140), rng.gen_range(180..255), 40) } else { Rgb(rng.gen_range(150..220), 255, rng.gen_range(180..230)) };
                    self.parts.push(SP { x: rng.gen::<f32>() * wf, y: hf, vx: 0.0, vy: -rng.gen_range(3.0..9.0), ch, c, life: rng.gen_range(3.0..7.0) });
                    if toxic && rng.gen_bool(0.3) {
                        self.parts.push(SP { x: rng.gen::<f32>() * wf, y: 0.0, vx: 0.0, vy: rng.gen_range(6.0..14.0), ch: '|', c: Rgb(90, 200, 40), life: 4.0 });
                    }
                }
                let t = self.t;
                for p in self.parts.iter_mut() {
                    p.x += ((p.y * 0.3 + t * 2.0).sin() * 3.0 + p.vx) * dt;
                    p.y += p.vy * dt;
                    p.life -= dt;
                }
            }
            DeathFx::Cracks => {
                if self.heads.is_empty() && self.cracks.is_empty() {
                    for _ in 0..7 {
                        self.heads.push((wf / 2.0, hf / 2.0, rng.gen::<f32>() * TAU));
                    }
                }
                let mut newheads = Vec::new();
                for hd in self.heads.iter_mut() {
                    hd.2 += rng.gen_range(-0.5..0.5);
                    let (dx, dy) = (hd.2.cos(), hd.2.sin() * 0.5);
                    hd.0 += dx * 30.0 * dt;
                    hd.1 += dy * 30.0 * dt;
                    let ch = if dx.abs() > dy.abs() * 2.5 {
                        '_'
                    } else if dy.abs() > dx.abs() * 2.5 {
                        '|'
                    } else if (dx > 0.0) == (dy > 0.0) {
                        '\\'
                    } else {
                        '/'
                    };
                    self.cracks.push((hd.0 as i32, hd.1 as i32, ch));
                    if rng.gen_bool(0.03) && self.cracks.len() < 3000 {
                        newheads.push((hd.0, hd.1, hd.2 + rng.gen_range(-1.0..1.0)));
                    }
                }
                self.heads.extend(newheads);
                self.heads.retain(|hd| hd.0 >= 0.0 && hd.1 >= 0.0 && hd.0 < wf && hd.1 < hf);
            }
            DeathFx::Spikes => {
                if rng.gen_bool(0.4) {
                    let x = rng.gen::<f32>() * wf;
                    let top = hf - (hf / 3.0) * (self.t / 1.2).min(1.0);
                    self.parts.push(SP { x, y: top, vx: 0.0, vy: rng.gen_range(4.0..10.0), ch: '\'', c: Rgb(200, 0, 0), life: 2.0 });
                }
                for p in self.parts.iter_mut() {
                    p.y += p.vy * dt;
                    p.life -= dt;
                }
            }
            DeathFx::Arcane => {
                if rng.gen_bool(0.15) {
                    let mut x = rng.gen::<f32>() * wf;
                    let mut y = 0.0;
                    while y < hf {
                        self.parts.push(SP { x, y, vx: 0.0, vy: 0.0, ch: ['/', '\\', '|'][rng.gen_range(0..3)], c: Rgb(200, 160, 255), life: 0.25 });
                        x += rng.gen_range(-1.5..1.5);
                        y += 1.0;
                    }
                }
                for p in self.parts.iter_mut() {
                    p.life -= dt;
                }
            }
        }
        self.parts.retain(|p| p.life > 0.0 && p.y >= -1.0 && p.y <= hf + 1.0);
        if self.parts.len() > 4000 {
            let ex = self.parts.len() - 4000;
            self.parts.drain(0..ex);
        }
    }

    pub fn draw(&self, c: &mut Canvas) {
        let w = c.w as i32;
        let h = c.h as i32;
        let t = self.t;
        let bg = match self.kind {
            DeathFx::Fire | DeathFx::Ash => Rgb(10, 2, 0),
            DeathFx::BloodRain => Rgb(18, 0, 0),
            DeathFx::GhostMist => Rgb(4, 10, 10),
            DeathFx::Toxic => Rgb(4, 14, 2),
            DeathFx::Cracks => Rgb(30, 4, 4).lerp(Rgb(60, 8, 8), 0.5 + 0.5 * (t * 2.0).sin()),
            DeathFx::Spikes => Rgb(12, 4, 4),
            DeathFx::Arcane => Rgb(10, 4, 20),
        };
        c.fill(0, 0, w, h, ' ', Rgb::WHITE, bg);
        match self.kind {
            DeathFx::Fire => self.fire.draw(c, 0, 0, 0),
            DeathFx::Ash => self.fire.draw(c, 0, 0, if t < 3.0 { 0 } else { 2 }),
            DeathFx::BloodRain => {
                for (x, &p) in self.pool.iter().enumerate() {
                    let ph = p as i32;
                    for k in 0..=ph {
                        let ch = if k == ph { '~' } else { '#' };
                        c.set(x as i32, h - 1 - k, ch, Rgb(150, 0, 0), Rgb(60, 0, 0));
                    }
                }
            }
            DeathFx::Cracks => {
                for &(x, y, ch) in &self.cracks {
                    c.set(x, y, ch, Rgb(255, 230, 200), Rgb(80, 10, 10));
                }
            }
            DeathFx::Spikes => {
                let rise = (t / 1.2).min(1.0);
                let sh = ((h as f32 / 3.0) * rise) as i32;
                for x in 0..w {
                    let hgt = (sh as f32 * (0.6 + 0.4 * hashf(x / 3, 7))) as i32;
                    for k in 0..hgt {
                        let y = h - 1 - k;
                        let phase = x % 3;
                        let ch = if k == hgt - 1 { '^' } else if phase == 0 { '/' } else if phase == 2 { '\\' } else { '|' };
                        let col = if k > hgt - 3 { Rgb(220, 30, 30) } else { Rgb(150, 150, 160) };
                        c.set(x, y, ch, col, Rgb(20, 20, 24));
                    }
                }
            }
            DeathFx::Arcane => {
                let n = 48;
                let r0 = (w.min(h * 2) as f32) * 0.42;
                for i in 0..n {
                    let a = i as f32 / n as f32 * TAU + t * 0.8;
                    let r = r0 * (1.0 + 0.08 * (t * 2.0 + i as f32).sin());
                    let x = w as f32 / 2.0 + a.cos() * r;
                    let y = h as f32 / 2.0 + a.sin() * r * 0.5;
                    let ch = ['*', '+', 'x', '%', '&', '$', '#', '@'][(hash2(i, (t * 3.0) as i32) % 8) as usize];
                    let col = Rgb(140, 80, 255).lerp(Rgb(120, 220, 255), 0.5 + 0.5 * (a * 3.0).sin());
                    c.put(x as i32, y as i32, ch, col);
                }
            }
            _ => {}
        }
        for p in &self.parts {
            c.put(p.x as i32, p.y as i32, p.ch, p.c);
        }

        // tombstone
        let tw = TOMB[0].chars().count() as i32;
        let tx = (w - tw) / 2;
        let ty = (h - TOMB.len() as i32) / 2 - 1;
        let stone = Rgb(70, 70, 78);
        for (r, line) in TOMB.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            let first = chars.iter().position(|&ch| ch != ' ').unwrap_or(0);
            let last = chars.iter().rposition(|&ch| ch != ' ').unwrap_or(0);
            for (i, &ch) in chars.iter().enumerate() {
                if i < first || i > last {
                    continue;
                }
                c.set(tx + i as i32, ty + r as i32, ch, Rgb(200, 200, 210), stone);
            }
        }
        let inner = 24usize;
        let mut lines: Vec<(String, Rgb)> = Vec::new();
        if let Some(first) = self.info.first() {
            lines.push((first.clone(), Rgb(255, 255, 255)));
        }
        lines.push((String::new(), stone));
        for l in wrap(&self.epitaph, inner) {
            lines.push((l, Rgb(230, 210, 190)));
        }
        for (i, (s, col)) in lines.iter().enumerate().take(7) {
            let s: String = s.chars().take(inner).collect();
            let x = tx + (tw - s.chars().count() as i32) / 2;
            c.text_bg(x, ty + 4 + i as i32, &s, *col, stone);
        }

        let tcol = Rgb(200, 0, 0).lerp(Rgb(255, 120, 80), 0.5 + 0.5 * (t * 3.0).sin());
        let title_y = (ty - 3).max(0);
        c.text_bg((w - self.title.len() as i32) / 2, title_y, &self.title, tcol, Rgb::BLACK);
        let by = ty + TOMB.len() as i32 + 1;
        for (i, l) in self.info.iter().skip(1).enumerate() {
            let col = if l.contains("Soul") { Rgb(210, 160, 255) } else { Rgb(220, 220, 220) };
            let x = (w - l.len() as i32) / 2;
            c.text_bg(x, by + i as i32, l, col, Rgb::BLACK);
        }
        if t > 1.5 {
            c.center(h - 1, "Press Enter to return to the surface world", Rgb(160, 160, 160));
        }
    }
}

// ================================================================ victory

pub fn draw_victory(c: &mut Canvas, t: f32, info: &[String]) {
    let w = c.w as i32;
    let h = c.h as i32;
    let horizon = (h as f32 * 0.68) as i32;
    let rise = (t / 6.0).min(1.0);
    for y in 0..h {
        let k = y as f32 / horizon.max(1) as f32;
        let sky = Rgb(8, 10, 40).lerp(Rgb(60, 40, 120), k).lerp(Rgb(255, 140, 60), (k * k * k) * rise);
        for x in 0..w {
            c.set(x, y, ' ', Rgb::WHITE, if y < horizon { sky } else { Rgb(10, 14, 8) });
        }
    }
    // stars fading out
    for i in 0..80 {
        let x = (hash2(i, 1) % w.max(1) as u32) as i32;
        let y = (hash2(i, 2) % (horizon.max(2) as u32 / 2)) as i32;
        let tw = 0.5 + 0.5 * (t * 3.0 + i as f32).sin();
        c.put(x, y, if tw > 0.7 { '*' } else { '.' }, Rgb(255, 255, 255).scale((1.0 - rise) * tw));
    }
    // sun
    let sx = w as f32 / 2.0;
    let sy = horizon as f32 + 4.0 - rise * (horizon as f32 * 0.45);
    let sr = 5.0;
    for i in 0..24 {
        let a = i as f32 / 24.0 * TAU + t * 0.2;
        for k in 0..((w / 2) as usize) {
            let r = sr + 2.0 + k as f32 * 1.6;
            let x = sx + a.cos() * r * 2.0;
            let y = sy + a.sin() * r;
            if y >= horizon as f32 || x < 0.0 || y < 0.0 || x >= w as f32 {
                break;
            }
            if k % 3 == 0 {
                let ch = if (a.sin() * a.cos()).abs() < 0.2 { if a.cos().abs() > 0.5 { '-' } else { '|' } } else if a.sin() * a.cos() > 0.0 { '\\' } else { '/' };
                c.put(x as i32, y as i32, ch, Rgb(255, 220, 120).scale(rise * (1.0 - k as f32 / (w as f32 / 2.0))));
            }
        }
    }
    for dy in -6..=6 {
        for dx in -12..=12 {
            let d = ((dx as f32 / 2.0).powi(2) + (dy as f32).powi(2)).sqrt();
            let y = sy as i32 + dy;
            if d <= sr && y < horizon && y >= 0 {
                let col = Rgb(255, 250, 200).lerp(Rgb(255, 170, 40), d / sr);
                c.set(sx as i32 + dx, y, if d < sr - 1.0 { '@' } else { 'O' }, col, col.scale(0.8));
            }
        }
    }
    // mountains
    for x in 0..w {
        let hgt = (6.0 + 5.0 * ((x as f32 * 0.07).sin() + 0.6 * (x as f32 * 0.19 + 1.3).sin()) + 3.0 * hashf(x / 4, 3)) as i32;
        for k in 0..hgt.max(1) {
            let y = horizon - k;
            let ch = if k == hgt - 1 { '^' } else { '#' };
            c.set(x, y, ch, Rgb(40, 30, 50).lerp(Rgb(120, 70, 60), rise * 0.5), Rgb(20, 14, 26));
        }
    }
    // birds
    for i in 0..5 {
        let bx = ((t * 6.0 + i as f32 * 17.0) % (w as f32 + 20.0)) as i32 - 10;
        let by = 3 + i * 2 + ((t * 2.0 + i as f32).sin() * 1.0) as i32;
        c.put(bx, by, if ((t * 6.0) as i32 + i) % 2 == 0 { 'v' } else { '-' }, Rgb(30, 20, 30));
    }
    let gold = Rgb(255, 200, 50).lerp(Rgb::WHITE, 0.5 + 0.5 * (t * 2.0).sin());
    c.center(2, "V  I  C  T  O  R  Y", gold);
    c.center(4, "You climb from the dark with the Amulet of GRAZZARK!", Rgb(255, 240, 200));
    for (i, l) in info.iter().enumerate() {
        let y = horizon + 2 + i as i32;
        let col = if l.contains("Soul") { Rgb(210, 160, 255) } else { Rgb(230, 230, 210) };
        c.center(y, l, col);
    }
    if t > 2.0 {
        c.center(h - 1, "Press Enter to return in glory", Rgb(200, 200, 160));
    }
    let _ = PI;
}

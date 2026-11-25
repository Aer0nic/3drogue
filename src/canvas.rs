//! An off-screen grid of coloured characters. Everything is drawn here first,
//! then `term::Term` diffs it against the previous frame and writes changes.

use crate::color::Rgb;
use std::sync::atomic::{AtomicBool, Ordering};

/// Solid visual mode: art is drawn as filled shapes and solid lines instead
/// of ASCII characters. Text (anything drawn through the `text` family or
/// the `*_raw` calls) is left alone so it stays readable.
static SOLID: AtomicBool = AtomicBool::new(false);

#[inline]
pub fn solid() -> bool {
    SOLID.load(Ordering::Relaxed)
}

pub fn set_solid(on: bool) {
    SOLID.store(on, Ordering::Relaxed);
}

/// The solid-mode stand-in for an ASCII art glyph: thin solid lines where
/// the ASCII draws a stroke, a few small solid shapes for marks and eyes.
/// Letters and digits are kept: they are labels (monster and item letters
/// on the maps) as often as they are art.
pub fn solid_glyph(ch: char) -> char {
    match ch {
        '#' => '╬',
        '@' | '§' | '°' | '•' => '●',
        '%' | '&' => '▓',
        '$' | '*' => '◆',
        '+' => '┼',
        '=' => '═',
        '-' => '─',
        '|' | '¦' => '│',
        '!' => '▮',
        '_' => '▁',
        '/' => '╱',
        '\\' => '╲',
        '(' | '[' | '{' => '▕',
        ')' | ']' | '}' => '▏',
        '<' => '◄',
        '>' => '►',
        '^' => '▲',
        '~' => '≈',
        '.' | ',' | '\'' | '`' | ':' | ';' | '"' => '·',
        _ => ch,
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: ' ', fg: Rgb(200, 200, 200), bg: Rgb::BLACK }
    }
}

#[derive(Clone)]
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<Cell>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas { w, h, cells: vec![Cell::default(); w * h] }
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        if w != self.w || h != self.h {
            self.w = w;
            self.h = h;
            self.cells = vec![Cell::default(); w * h];
        }
    }

    pub fn clear(&mut self) {
        for c in self.cells.iter_mut() {
            *c = Cell::default();
        }
    }

    #[inline]
    pub fn inb(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h
    }

    /// Draw an art cell: in solid mode the glyph becomes a solid shape.
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, ch: char, fg: Rgb, bg: Rgb) {
        let ch = if solid() { solid_glyph(ch) } else { ch };
        self.set_raw(x, y, ch, fg, bg);
    }

    /// Set a cell exactly as given: text, or a copy of a cell already drawn.
    #[inline]
    pub fn set_raw(&mut self, x: i32, y: i32, ch: char, fg: Rgb, bg: Rgb) {
        if self.inb(x, y) {
            let i = y as usize * self.w + x as usize;
            self.cells[i] = Cell { ch, fg, bg };
        }
    }

    /// Set character and foreground, keeping the existing background.
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, ch: char, fg: Rgb) {
        let ch = if solid() { solid_glyph(ch) } else { ch };
        self.put_raw(x, y, ch, fg);
    }

    /// `put` without solid-mode glyph swapping, for text.
    #[inline]
    pub fn put_raw(&mut self, x: i32, y: i32, ch: char, fg: Rgb) {
        if self.inb(x, y) {
            let i = y as usize * self.w + x as usize;
            self.cells[i].ch = ch;
            self.cells[i].fg = fg;
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Cell {
        if self.inb(x, y) {
            self.cells[y as usize * self.w + x as usize]
        } else {
            Cell::default()
        }
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, fg: Rgb) {
        for (i, ch) in s.chars().enumerate() {
            self.put_raw(x + i as i32, y, ch, fg);
        }
    }

    pub fn text_bg(&mut self, x: i32, y: i32, s: &str, fg: Rgb, bg: Rgb) {
        for (i, ch) in s.chars().enumerate() {
            self.set_raw(x + i as i32, y, ch, fg, bg);
        }
    }

    pub fn center(&mut self, y: i32, s: &str, fg: Rgb) {
        let x = (self.w as i32 - s.chars().count() as i32) / 2;
        self.text(x, y, s, fg);
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, ch: char, fg: Rgb, bg: Rgb) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, ch, fg, bg);
            }
        }
    }

    /// A bordered box with a solid background. In solid mode the border is
    /// drawn with box lines.
    pub fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, fg: Rgb, bg: Rgb) {
        self.fill(x, y, w, h, ' ', fg, bg);
        let [hz, vt, tl, tr, bl, br] = if solid() {
            ['━', '┃', '┏', '┓', '┗', '┛']
        } else {
            ['-', '|', '+', '+', '+', '+']
        };
        for xx in x..x + w {
            self.set_raw(xx, y, hz, fg, bg);
            self.set_raw(xx, y + h - 1, hz, fg, bg);
        }
        for yy in y..y + h {
            self.set_raw(x, yy, vt, fg, bg);
            self.set_raw(x + w - 1, yy, vt, fg, bg);
        }
        self.set_raw(x, y, tl, fg, bg);
        self.set_raw(x + w - 1, y, tr, fg, bg);
        self.set_raw(x, y + h - 1, bl, fg, bg);
        self.set_raw(x + w - 1, y + h - 1, br, fg, bg);
    }

    pub fn tint_region(&mut self, x0: i32, y0: i32, w: i32, h: i32, col: Rgb, amt: f32) {
        for y in y0.max(0)..(y0 + h).min(self.h as i32) {
            for x in x0.max(0)..(x0 + w).min(self.w as i32) {
                let i = y as usize * self.w + x as usize;
                let c = &mut self.cells[i];
                c.fg = c.fg.lerp(col, amt);
                c.bg = c.bg.lerp(col, amt * 0.7);
            }
        }
    }

    pub fn darken(&mut self, amt: f32) {
        for c in self.cells.iter_mut() {
            c.fg = c.fg.scale(1.0 - amt);
            c.bg = c.bg.scale(1.0 - amt);
        }
    }
}

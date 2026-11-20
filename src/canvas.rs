//! An off-screen grid of coloured characters. Everything is drawn here first,
//! then `term::Term` diffs it against the previous frame and writes changes.

use crate::color::Rgb;

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

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, ch: char, fg: Rgb, bg: Rgb) {
        if self.inb(x, y) {
            let i = y as usize * self.w + x as usize;
            self.cells[i] = Cell { ch, fg, bg };
        }
    }

    /// Set character and foreground, keeping the existing background.
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, ch: char, fg: Rgb) {
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
            self.put(x + i as i32, y, ch, fg);
        }
    }

    pub fn text_bg(&mut self, x: i32, y: i32, s: &str, fg: Rgb, bg: Rgb) {
        for (i, ch) in s.chars().enumerate() {
            self.set(x + i as i32, y, ch, fg, bg);
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

    /// A bordered box with a solid background.
    pub fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, fg: Rgb, bg: Rgb) {
        self.fill(x, y, w, h, ' ', fg, bg);
        for xx in x..x + w {
            self.set(xx, y, '-', fg, bg);
            self.set(xx, y + h - 1, '-', fg, bg);
        }
        for yy in y..y + h {
            self.set(x, yy, '|', fg, bg);
            self.set(x + w - 1, yy, '|', fg, bg);
        }
        self.set(x, y, '+', fg, bg);
        self.set(x + w - 1, y, '+', fg, bg);
        self.set(x, y + h - 1, '+', fg, bg);
        self.set(x + w - 1, y + h - 1, '+', fg, bg);
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

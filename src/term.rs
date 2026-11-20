//! Terminal output: raw mode, alternate screen, and diffed frame presentation.

use crate::canvas::{Canvas, Cell};
use crate::color::Rgb;
use crossterm::{
    cursor, execute, queue,
    style::{self, Color},
    terminal,
};
use std::io::{self, BufWriter, Stdout, Write};

pub struct Term {
    out: BufWriter<Stdout>,
    prev: Vec<Cell>,
    pw: usize,
    ph: usize,
    pub truecolor: bool,
}

impl Term {
    pub fn new(truecolor: bool) -> io::Result<Term> {
        terminal::enable_raw_mode()?;
        let mut out = BufWriter::with_capacity(1 << 17, io::stdout());
        execute!(
            out,
            terminal::EnterAlternateScreen,
            cursor::Hide,
            terminal::Clear(terminal::ClearType::All)
        )?;
        Ok(Term { out, prev: Vec::new(), pw: 0, ph: 0, truecolor })
    }

    pub fn restore() {
        let mut o = io::stdout();
        let _ = execute!(o, style::ResetColor, cursor::Show, terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }

    pub fn size() -> (usize, usize) {
        terminal::size()
            .map(|(w, h)| (w as usize, h as usize))
            .unwrap_or((80, 24))
    }

    /// Force a full redraw on the next present.
    pub fn invalidate(&mut self) {
        self.pw = 0;
        self.ph = 0;
    }

    fn color(&self, c: Rgb) -> Color {
        if self.truecolor {
            Color::Rgb { r: c.0, g: c.1, b: c.2 }
        } else {
            Color::AnsiValue(to256(c))
        }
    }

    pub fn present(&mut self, c: &Canvas) -> io::Result<()> {
        let full = c.w != self.pw || c.h != self.ph;
        if full {
            queue!(self.out, style::ResetColor, terminal::Clear(terminal::ClearType::All))?;
            self.prev = vec![Cell { ch: '\u{0}', fg: Rgb::BLACK, bg: Rgb::BLACK }; c.w * c.h];
            self.pw = c.w;
            self.ph = c.h;
        }
        let mut fg: Option<Rgb> = None;
        let mut bg: Option<Rgb> = None;
        let mut cur: Option<(usize, usize)> = None;
        let total = c.w * c.h;
        for y in 0..c.h {
            for x in 0..c.w {
                let i = y * c.w + x;
                // Never write the bottom-right cell: some terminals scroll.
                if i + 1 == total {
                    continue;
                }
                let cell = c.cells[i];
                if self.prev[i] == cell {
                    continue;
                }
                if cur != Some((x, y)) {
                    queue!(self.out, cursor::MoveTo(x as u16, y as u16))?;
                }
                if fg != Some(cell.fg) {
                    let col = self.color(cell.fg);
                    queue!(self.out, style::SetForegroundColor(col))?;
                    fg = Some(cell.fg);
                }
                if bg != Some(cell.bg) {
                    let col = self.color(cell.bg);
                    queue!(self.out, style::SetBackgroundColor(col))?;
                    bg = Some(cell.bg);
                }
                let ch = if cell.ch.is_control() { ' ' } else { cell.ch };
                queue!(self.out, style::Print(ch))?;
                cur = Some((x + 1, y));
                self.prev[i] = cell;
            }
        }
        self.out.flush()
    }
}

/// Map an RGB colour onto the xterm 256-colour palette.
pub fn to256(c: Rgb) -> u8 {
    let (r, g, b) = (c.0 as i32, c.1 as i32, c.2 as i32);
    if (r - g).abs() < 12 && (g - b).abs() < 12 && (r - b).abs() < 12 {
        let v = (r + g + b) / 3;
        if v < 8 {
            return 16;
        }
        if v > 238 {
            return 231;
        }
        return (232 + (v - 8) * 24 / 231) as u8;
    }
    let q = |v: i32| -> i32 {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            ((v - 35) / 40).min(5)
        }
    };
    (16 + 36 * q(r) + 6 * q(g) + q(b)) as u8
}

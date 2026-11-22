//! Terminal output: raw mode, alternate screen, and diffed frame presentation.

use crate::canvas::{Canvas, Cell};
use crate::color::Rgb;
use crossterm::{
    cursor, execute, queue,
    style::{self, Color},
    terminal,
};
use std::io::{self, BufWriter, Stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// The terminal has been asked to report every key on its own, left and
/// right modifiers included (kitty keyboard protocol). Only while needed.
static KBD_EXACT: AtomicBool = AtomicBool::new(false);

pub struct Term {
    out: BufWriter<Stdout>,
    prev: Vec<Cell>,
    pw: usize,
    ph: usize,
    pub truecolor: bool,
    /// Round colours to 16 shades a channel (bare-bones detail), so tiny
    /// changes such as torch flicker don't make the terminal redraw cells.
    pub coarse: bool,
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
        Ok(Term { out, prev: Vec::new(), pw: 0, ph: 0, truecolor, coarse: false })
    }

    /// Can this terminal tell us exactly which keys are held (left or right
    /// Shift and Ctrl, pressed and released)? Asked once at start-up.
    pub fn exact_keys_supported() -> bool {
        terminal::supports_keyboard_enhancement().unwrap_or(false)
    }

    /// Switch exact key reporting on or off. With it on, modifier keys arrive
    /// as key events of their own and every key reports its release.
    pub fn set_exact_keys(on: bool) {
        use crossterm::event::{
            KeyboardEnhancementFlags as F, PopKeyboardEnhancementFlags,
            PushKeyboardEnhancementFlags,
        };
        if on == KBD_EXACT.load(Ordering::Relaxed) {
            return;
        }
        let mut o = io::stdout();
        if on {
            let _ = execute!(
                o,
                PushKeyboardEnhancementFlags(
                    F::DISAMBIGUATE_ESCAPE_CODES
                        | F::REPORT_EVENT_TYPES
                        | F::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                        | F::REPORT_ALTERNATE_KEYS
                )
            );
        } else {
            let _ = execute!(o, PopKeyboardEnhancementFlags);
        }
        KBD_EXACT.store(on, Ordering::Relaxed);
    }

    pub fn restore() {
        Self::set_exact_keys(false);
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

    /// The colour as the terminal will show it. In 256-colour mode the
    /// palette index is stashed in the red channel, so cells whose colours
    /// differ only by shades the palette can't show compare equal and are
    /// not redrawn (torch flicker would otherwise repaint every cell).
    #[inline]
    fn key(&self, c: Rgb) -> Rgb {
        if self.truecolor {
            if self.coarse {
                let q = |v: u8| (v & 0xF0) | (v >> 4);
                Rgb(q(c.0), q(c.1), q(c.2))
            } else {
                c
            }
        } else {
            Rgb(to256(c), 0, 0)
        }
    }

    fn color(&self, k: Rgb) -> Color {
        if self.truecolor {
            Color::Rgb { r: k.0, g: k.1, b: k.2 }
        } else {
            Color::AnsiValue(k.0)
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
                let src = c.cells[i];
                let cell = Cell { ch: src.ch, fg: self.key(src.fg), bg: self.key(src.bg) };
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

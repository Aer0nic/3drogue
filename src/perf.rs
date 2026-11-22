//! How much work each frame does, and picking that to suit the machine.
//!
//! The game draws characters into a terminal, so the terminal emulator does
//! the actual pixel work (GPU-accelerated terminals such as kitty, Alacritty,
//! WezTerm or Ghostty already put that on the graphics card). What the game
//! itself spends time on is ray casting the first-person view on the CPU and
//! pushing escape codes down the pipe to the terminal. So "running fast" here
//! means: use every CPU core for the ray caster, and do less work per frame
//! when the machine (or the terminal) can't keep up.
//!
//! Three detail levels:
//! - **High**: everything.
//! - **Low**: the 3D view casts every other column and doubles them up.
//! - **Minimal**: bare bones for very slow CPUs. Half-resolution casting,
//!   plain lighting on floors and ceilings (no texture noise, no flicker,
//!   no blood or rot), no particles or low-health vignette, colours rounded so
//!   the terminal has fewer cells to redraw, and 20 frames a second.
//!
//! In **Auto** the game times every frame and steps down a level when frames
//! run long, and back up when there is plenty of headroom.

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Detail {
    High = 0,
    Low = 1,
    Minimal = 2,
}

impl Detail {
    pub fn name(self) -> &'static str {
        match self {
            Detail::High => "High",
            Detail::Low => "Low",
            Detail::Minimal => "Minimal",
        }
    }

    fn from_u8(v: u8) -> Detail {
        match v {
            0 => Detail::High,
            1 => Detail::Low,
            _ => Detail::Minimal,
        }
    }

    /// Time allowed for one frame at this level.
    pub fn frame_time(self) -> Duration {
        match self {
            Detail::High | Detail::Low => Duration::from_millis(33),
            Detail::Minimal => Duration::from_millis(50),
        }
    }

    /// How many screen columns share one cast ray.
    pub fn col_step(self) -> i32 {
        match self {
            Detail::High => 1,
            _ => 2,
        }
    }

    fn lower(self) -> Option<Detail> {
        match self {
            Detail::High => Some(Detail::Low),
            Detail::Low => Some(Detail::Minimal),
            Detail::Minimal => None,
        }
    }

    fn higher(self) -> Option<Detail> {
        match self {
            Detail::High => None,
            Detail::Low => Some(Detail::High),
            Detail::Minimal => Some(Detail::Low),
        }
    }
}

/// What the player picked: a fixed level, or let the game decide.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Setting {
    Auto,
    Fixed(Detail),
}

impl Setting {
    /// Stored in the profile as 0 = Auto, 1 = High, 2 = Low, 3 = Minimal.
    pub fn to_u8(self) -> u8 {
        match self {
            Setting::Auto => 0,
            Setting::Fixed(d) => d as u8 + 1,
        }
    }

    pub fn from_u8(v: u8) -> Setting {
        match v {
            0 => Setting::Auto,
            v => Setting::Fixed(Detail::from_u8(v - 1)),
        }
    }

    pub fn parse(s: &str) -> Option<Setting> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Some(Setting::Auto),
            "high" => Some(Setting::Fixed(Detail::High)),
            "low" => Some(Setting::Fixed(Detail::Low)),
            "minimal" | "min" | "bare" => Some(Setting::Fixed(Detail::Minimal)),
            _ => None,
        }
    }

    /// The next choice in the menu: Auto, High, Low, Minimal, Auto...
    pub fn cycle(self, dir: i32) -> Setting {
        let n = 4;
        Setting::from_u8(((self.to_u8() as i32 + dir).rem_euclid(n)) as u8)
    }
}

// The renderer reads these every frame; the main loop writes them.
static DETAIL: AtomicU8 = AtomicU8::new(0);
static THREADS: AtomicUsize = AtomicUsize::new(1);

/// The detail level in force right now.
pub fn detail() -> Detail {
    Detail::from_u8(DETAIL.load(Ordering::Relaxed))
}

fn set_detail(d: Detail) {
    DETAIL.store(d as u8, Ordering::Relaxed);
}

/// Worker threads the ray caster may use.
pub fn threads() -> usize {
    THREADS.load(Ordering::Relaxed).max(1)
}

/// What the game found out about the machine at start-up.
#[derive(Clone, Debug)]
pub struct System {
    pub cores: usize,
}

impl System {
    pub fn detect() -> System {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        System { cores }
    }

    /// Leave a core for the audio thread and the terminal, and don't bother
    /// with more than eight: the view is only a few thousand cells.
    pub fn render_threads(&self) -> usize {
        if self.cores <= 2 {
            1
        } else {
            (self.cores - 1).min(8)
        }
    }

    /// Where Auto starts before it has measured anything.
    pub fn starting_detail(&self) -> Detail {
        if self.cores <= 1 {
            Detail::Low
        } else {
            Detail::High
        }
    }
}

/// Picks the detail level in Auto mode from measured frame times.
pub struct Governor {
    pub setting: Setting,
    /// Smoothed time spent working each frame (update, draw and present).
    avg: f32,
    /// How long frames have been too slow, or comfortably fast.
    slow_for: f32,
    fast_for: f32,
}

impl Governor {
    pub fn new(sys: &System, setting: Setting) -> Governor {
        THREADS.store(sys.render_threads(), Ordering::Relaxed);
        let start = match setting {
            Setting::Auto => sys.starting_detail(),
            Setting::Fixed(d) => d,
        };
        set_detail(start);
        Governor {
            setting,
            avg: 0.0,
            slow_for: 0.0,
            fast_for: 0.0,
        }
    }

    pub fn set(&mut self, setting: Setting) {
        self.setting = setting;
        if let Setting::Fixed(d) = setting {
            set_detail(d);
        }
        self.reset();
    }

    fn reset(&mut self) {
        self.avg = 0.0;
        self.slow_for = 0.0;
        self.fast_for = 0.0;
    }

    /// Text for the menu, e.g. "Auto (High)".
    pub fn label(&self) -> String {
        match self.setting {
            Setting::Auto => format!("Auto ({})", detail().name()),
            Setting::Fixed(d) => d.name().to_string(),
        }
    }

    /// Record one frame. `work` is how long the frame took to update, draw
    /// and present (not counting the wait for the next frame); `dt` is the
    /// real time since the previous frame. `busy` is true while the 3D view
    /// is on screen; menus are cheap and say nothing about the machine.
    pub fn record(&mut self, work: Duration, dt: f32, busy: bool) {
        if self.setting != Setting::Auto || !busy {
            return;
        }
        let w = work.as_secs_f32();
        self.avg = if self.avg == 0.0 {
            w
        } else {
            self.avg * 0.9 + w * 0.1
        };
        let cur = detail();
        let budget = cur.frame_time().as_secs_f32();
        if self.avg > budget * 0.8 {
            self.slow_for += dt;
            self.fast_for = 0.0;
        } else {
            self.slow_for = 0.0;
            // Only climb back up if the richer level would still fit easily:
            // High costs roughly twice what Low does.
            match cur.higher() {
                Some(up) if self.avg * 2.2 < up.frame_time().as_secs_f32() * 0.5 => {
                    self.fast_for += dt
                }
                _ => self.fast_for = 0.0,
            }
        }
        if self.slow_for > 1.0 {
            if let Some(d) = cur.lower() {
                set_detail(d);
            }
            self.reset();
        } else if self.fast_for > 8.0 {
            if let Some(d) = cur.higher() {
                set_detail(d);
            }
            self.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_round_trips_and_cycles() {
        for v in 0..4 {
            assert_eq!(Setting::from_u8(v).to_u8(), v);
        }
        assert_eq!(Setting::Auto.cycle(1), Setting::Fixed(Detail::High));
        assert_eq!(Setting::Auto.cycle(-1), Setting::Fixed(Detail::Minimal));
        assert_eq!(Setting::Fixed(Detail::Minimal).cycle(1), Setting::Auto);
        assert_eq!(
            Setting::parse("MINIMAL"),
            Some(Setting::Fixed(Detail::Minimal))
        );
    }

    #[test]
    fn auto_steps_down_when_slow_and_up_when_fast() {
        let sys = System { cores: 4 };
        let mut gv = Governor::new(&sys, Setting::Auto);
        assert_eq!(detail(), Detail::High);
        for _ in 0..30 {
            gv.record(Duration::from_millis(40), 0.04, true);
        }
        assert_eq!(detail(), Detail::Low);
        for _ in 0..400 {
            gv.record(Duration::from_millis(2), 0.033, true);
        }
        assert_eq!(detail(), Detail::High);
        gv.set(Setting::Fixed(Detail::Minimal));
        for _ in 0..400 {
            gv.record(Duration::from_millis(1), 0.033, true);
        }
        assert_eq!(detail(), Detail::Minimal);
    }
}

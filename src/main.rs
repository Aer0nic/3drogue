//! GRAZZARK: a first-person ASCII roguelike.
//!
//! Descend ten randomly generated levels, take the Amulet of GRAZZARK from
//! its Warden, and climb back to the surface alive. Death earns soul points
//! that buy permanent upgrades on the Soul Altar.

mod anim;
mod audio;
mod canvas;
mod color;
mod data;
mod entity;
mod fx;
mod game;
mod launch;
mod perf;
mod map;
mod overlay;
mod profile;
mod render;
mod screens;
mod term;

use audio::{Audio, MusicMode, Sfx, Spatial};
use canvas::Canvas;
use color::Rgb;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode,
};
use data::{CLASSES, DIFFICULTIES, MONSTERS, UPGRADES};
use game::{cap, Action, DeathCause, Game, Outcome, GREY};
use profile::{Profile, RunRecord};
use rand::Rng;
use screens::{DeathScreen, Fire};
use std::sync::atomic::AtomicI32;
#[cfg(unix)]
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use term::Term;

static SAVED_STDERR: AtomicI32 = AtomicI32::new(-1);

/// The ALSA backend can print diagnostics to stderr, which would scribble
/// over the game screen. Send stderr to /dev/null while the game runs.
#[cfg(not(unix))]
fn silence_stderr() {
    let _ = &SAVED_STDERR;
}
#[cfg(not(unix))]
fn restore_stderr() {}

#[cfg(unix)]
fn silence_stderr() {
    unsafe {
        let saved = libc::dup(2);
        SAVED_STDERR.store(saved, Ordering::SeqCst);
        let devnull = libc::open(
            b"/dev/null\0".as_ptr() as *const libc::c_char,
            libc::O_WRONLY,
        );
        if devnull >= 0 {
            libc::dup2(devnull, 2);
            libc::close(devnull);
        }
    }
}

#[cfg(unix)]
fn restore_stderr() {
    let s = SAVED_STDERR.load(Ordering::SeqCst);
    if s >= 0 {
        unsafe {
            libc::dup2(s, 2);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    Title,
    ClassSel,
    /// Choose difficulty after the class.
    DiffSel,
    Altar,
    Bestiary,
    Hall,
    Help,
    /// The legend of GRAZZARK.
    Story,
    /// Choose which profile to play.
    Profiles,
    /// Type a name for a new profile.
    NewProfile,
    Game,
    Dead,
    Victory,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Overlay {
    None,
    Inventory,
    Pause,
    Help,
    /// Full-screen plain map (M).
    Map,
    /// Choose what to pick up from a pile.
    Pickup,
    /// Message history (L).
    Log,
    /// Choose which corpse to raise.
    Raise,
    /// The legend, told before your very first descent.
    Story,
}

/// Screen-melt transition between dungeon levels.
struct Melt {
    snap: Canvas,
    offs: Vec<f32>,
    t: f32,
}

struct App {
    screen: Screen,
    sel: usize,
    class_sel: usize,
    diff_sel: usize,
    best_sel: usize,
    altar_sel: usize,
    pause_sel: usize,
    /// Lines scrolled back from the newest in the message log.
    log_scroll: usize,
    raise_sel: usize,
    profile: Profile,
    game: Option<Game>,
    audio: Audio,
    fire: Fire,
    t: f32,
    overlay: Overlay,
    ambient_timer: f32,
    last_depth: u32,
    melt: Option<Melt>,
    prev: Canvas,
    death: Option<DeathScreen>,
    end_info: Vec<String>,
    end_t: f32,
    over_timer: f32,
    quit: bool,
    note: String,
    /// A saved run exists on disk.
    has_save: bool,
    /// Profiles found on disk, for the picker.
    profiles: Vec<profile::ProfileEntry>,
    prof_sel: usize,
    /// The name being typed for a new profile, and any complaint about it.
    name_buf: String,
    name_err: String,
    /// A profile has been chosen this session (so Esc can go back to the title).
    loaded: bool,
    /// Title screen testing code: letters typed while holding left Shift and
    /// right Ctrl, and which of those keys are held right now (only known on
    /// terminals that report each key exactly).
    cheat_buf: String,
    held_lshift: bool,
    held_rctrl: bool,
    exact_keys: bool,
    /// A line shown on the title screen for a few seconds.
    title_note: String,
    title_note_t: f32,
    /// Picks the graphics detail level.
    gov: perf::Governor,
    /// Detail was given on the command line, so the profile's choice is ignored.
    detail_from_cli: bool,
}

impl App {
    fn new(profile: Profile, audio: Audio, w: usize, h: usize, gov: perf::Governor, detail_from_cli: bool) -> App {
        App {
            screen: Screen::Title,
            sel: if game::has_save() { 0 } else { 1 },
            class_sel: 0,
            diff_sel: 0,
            best_sel: 0,
            altar_sel: 0,
            pause_sel: 0,
            log_scroll: 0,
            raise_sel: 0,
            profile,
            game: None,
            audio,
            fire: Fire::new(),
            t: 0.0,
            overlay: Overlay::None,
            ambient_timer: 3.0,
            last_depth: 1,
            melt: None,
            prev: Canvas::new(w, h),
            death: None,
            end_info: Vec::new(),
            end_t: 0.0,
            over_timer: 0.0,
            quit: false,
            note: String::new(),
            has_save: false,
            profiles: Vec::new(),
            prof_sel: 0,
            name_buf: String::new(),
            name_err: String::new(),
            loaded: false,
            gov,
            detail_from_cli,
            cheat_buf: String::new(),
            held_lshift: false,
            held_rctrl: false,
            exact_keys: false,
            title_note: String::new(),
            title_note_t: 0.0,
        }
    }

    /// At startup: no profiles means naming a new one; a single profile loads
    /// at once; with several, the player picks.
    fn choose_profile_at_start(&mut self) {
        self.profiles = Profile::list();
        match self.profiles.len() {
            0 => self.begin_new_profile(),
            1 => {
                let dir = self.profiles[0].dir.clone();
                self.use_profile(&dir);
            }
            _ => self.open_profile_list(),
        }
    }

    fn open_profile_list(&mut self) {
        self.profiles = Profile::list();
        let current = if self.loaded {
            Some(self.profile.name.clone())
        } else {
            profile::last_used()
        };
        self.prof_sel = current
            .and_then(|n| self.profiles.iter().position(|e| e.name == n))
            .unwrap_or(0);
        self.screen = Screen::Profiles;
    }

    fn begin_new_profile(&mut self) {
        self.name_buf.clear();
        self.name_err.clear();
        self.screen = Screen::NewProfile;
    }

    /// Switch to the profile in `dir` and go to the title screen.
    fn use_profile(&mut self, dir: &std::path::Path) {
        self.profile = Profile::open(dir);
        self.after_profile_loaded();
    }

    fn after_profile_loaded(&mut self) {
        if let Some(v) = self.profile.volume {
            self.audio.set_volume(v);
        }
        if let (Some(d), false) = (self.profile.detail, self.detail_from_cli) {
            self.gov.set(perf::Setting::from_u8(d));
        }
        self.has_save = game::has_save();
        self.sel = if self.has_save { 0 } else { 1 };
        self.loaded = true;
        self.note.clear();
        self.screen = Screen::Title;
    }

    fn click(&self) {
        self.audio.play(Sfx::Menu, &Spatial::center(0.5));
    }

    /// Testing code on the title screen: hold left Shift and right Ctrl and
    /// type "stewardess" for 99999 soul points. Returns true if the key was
    /// taken as part of the code. Terminals that report keys exactly must show
    /// the very keys held; others can only say that Ctrl is down.
    fn title_cheat(&mut self, k: &KeyEvent) -> bool {
        const WORD: &str = "stewardess";
        let KeyCode::Char(ch) = k.code else {
            self.cheat_buf.clear();
            return false;
        };
        let held = if self.exact_keys {
            self.held_lshift && self.held_rctrl
        } else {
            k.modifiers.contains(KeyModifiers::CONTROL)
        };
        // Only letters of the word are taken, so Ctrl+C and the rest still work.
        if !held || !WORD.contains(ch.to_ascii_lowercase()) {
            self.cheat_buf.clear();
            return false;
        }
        if type_code(&mut self.cheat_buf, ch, WORD) {
            self.profile.points = self.profile.points.saturating_add(99_999);
            self.profile.save();
            self.title_note = format!(
                "The Soul Altar trembles. +99999 soul points ({} in all).",
                self.profile.points
            );
            self.title_note_t = 4.0;
            self.audio.play(Sfx::LevelUp, &Spatial::center(0.7));
        }
        true
    }

    fn on_key(&mut self, k: KeyEvent) {
        let code = k.code;
        if self.screen == Screen::Title && self.loaded && self.title_cheat(&k) {
            return;
        }
        if k.modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            self.save_and_quit();
            return;
        }
        if self.screen == Screen::NewProfile {
            match code {
                KeyCode::Enter => match Profile::create(&self.name_buf) {
                    Ok(p) => {
                        self.click();
                        self.profile = p;
                        let _ = std::fs::write(
                            profile::base_dir().join("last_profile.txt"),
                            &self.profile.name,
                        );
                        self.after_profile_loaded();
                    }
                    Err(e) => self.name_err = e,
                },
                KeyCode::Esc => {
                    // With nothing to go back to, Esc leaves the game.
                    if self.loaded {
                        self.screen = Screen::Title;
                    } else if !Profile::list().is_empty() {
                        self.open_profile_list();
                    } else {
                        self.quit = true;
                    }
                }
                KeyCode::Backspace => {
                    self.name_buf.pop();
                    self.name_err.clear();
                }
                KeyCode::Char(c) if profile::name_char_ok(c) => {
                    if self.name_buf.chars().count() < profile::NAME_MAX
                        && !(c == ' ' && self.name_buf.is_empty())
                    {
                        self.name_buf.push(c);
                    }
                    self.name_err.clear();
                }
                _ => {}
            }
            return;
        }
        let up = matches!(code, KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k'));
        let down = matches!(
            code,
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j')
        );
        let enter = matches!(code, KeyCode::Enter | KeyCode::Char(' '));
        let esc = matches!(code, KeyCode::Esc | KeyCode::Char('q'));
        match self.screen {
            Screen::Title => {
                let n = screens::TITLE_ITEMS.len();
                // "Continue" (item 0) only exists when there is a saved run.
                let first = if self.has_save { 0 } else { 1 };
                if self.sel < first {
                    self.sel = first;
                }
                if up {
                    self.sel = if self.sel <= first {
                        n - 1
                    } else {
                        self.sel - 1
                    };
                    self.click();
                } else if down {
                    self.sel = if self.sel + 1 >= n {
                        first
                    } else {
                        self.sel + 1
                    };
                    self.click();
                } else if enter {
                    self.click();
                    match self.sel {
                        0 => self.continue_game(),
                        1 => self.screen = Screen::ClassSel,
                        2 => {
                            self.note.clear();
                            self.screen = Screen::Altar;
                        }
                        3 => self.screen = Screen::Bestiary,
                        4 => self.screen = Screen::Hall,
                        5 => self.screen = Screen::Story,
                        6 => self.open_profile_list(),
                        7 => self.begin_new_profile(),
                        8 => self.screen = Screen::Help,
                        _ => self.quit = true,
                    }
                } else if esc {
                    self.quit = true;
                }
            }
            // Handled above, before keys are read as menu moves.
            Screen::NewProfile => {}
            Screen::Profiles => {
                let n = self.profiles.len().max(1);
                if up {
                    self.prof_sel = (self.prof_sel + n - 1) % n;
                    self.click();
                } else if down {
                    self.prof_sel = (self.prof_sel + 1) % n;
                    self.click();
                } else if enter {
                    if let Some(e) = self.profiles.get(self.prof_sel) {
                        self.click();
                        let dir = e.dir.clone();
                        self.use_profile(&dir);
                    }
                } else if matches!(code, KeyCode::Char('n') | KeyCode::Char('N')) {
                    self.click();
                    self.begin_new_profile();
                } else if esc {
                    if self.loaded {
                        self.screen = Screen::Title;
                    } else {
                        self.quit = true;
                    }
                }
            }
            Screen::ClassSel => {
                let n = CLASSES.len();
                if up {
                    self.class_sel = (self.class_sel + n - 1) % n;
                    self.click();
                } else if down {
                    self.class_sel = (self.class_sel + 1) % n;
                    self.click();
                } else if enter {
                    if self.profile.class_unlocked(&CLASSES[self.class_sel]) {
                        self.click();
                        self.diff_sel = self.diff_sel.min(self.profile.unlocked_diff as usize);
                        self.screen = Screen::DiffSel;
                    }
                } else if esc {
                    self.screen = Screen::Title;
                }
            }
            Screen::DiffSel => {
                let n = DIFFICULTIES.len();
                if up {
                    self.diff_sel = (self.diff_sel + n - 1) % n;
                    self.click();
                } else if down {
                    self.diff_sel = (self.diff_sel + 1) % n;
                    self.click();
                } else if enter {
                    if self.profile.diff_unlocked(self.diff_sel) {
                        self.start_game(self.class_sel, self.diff_sel as u8);
                    }
                } else if esc {
                    self.screen = Screen::ClassSel;
                }
            }
            Screen::Bestiary => {
                let n = MONSTERS.len();
                if up {
                    self.best_sel = (self.best_sel + n - 1) % n;
                    self.click();
                } else if down {
                    self.best_sel = (self.best_sel + 1) % n;
                    self.click();
                } else if esc || enter {
                    self.screen = Screen::Title;
                }
            }
            Screen::Altar => {
                let n = UPGRADES.len();
                if up {
                    self.altar_sel = (self.altar_sel + n - 1) % n;
                    self.click();
                } else if down {
                    self.altar_sel = (self.altar_sel + 1) % n;
                    self.click();
                } else if enter {
                    match self.profile.buy(self.altar_sel) {
                        Ok(m) => {
                            self.note = m;
                            self.audio.play(Sfx::LevelUp, &Spatial::center(0.7));
                        }
                        Err(m) => {
                            self.note = m;
                            self.click();
                        }
                    }
                } else if esc {
                    self.screen = Screen::Title;
                }
            }
            Screen::Hall | Screen::Help | Screen::Story => {
                if esc || enter {
                    self.screen = Screen::Title;
                }
            }
            Screen::Game => self.game_key(k),
            Screen::Dead | Screen::Victory => {
                if (enter || matches!(code, KeyCode::Esc)) && self.end_t > 1.5 {
                    self.screen = Screen::Title;
                    self.has_save = game::has_save();
                    self.sel = if self.has_save { 0 } else { 1 };
                    self.death = None;
                    self.audio.set_music(MusicMode::Title, 1);
                    self.audio.set_intensity(0.0);
                }
            }
        }
    }

    fn game_key(&mut self, k: KeyEvent) {
        let Some(g) = self.game.as_mut() else { return };
        if g.over.is_some() {
            return;
        }
        use KeyCode::*;
        match self.overlay {
            Overlay::Inventory => {
                match k.code {
                    Esc | Tab => self.overlay = Overlay::None,
                    // Backtick: sort the pack by type.
                    Char('`') => g.sort_inv(),
                    // Letters pick a slot; a stack of potions uses or drops one.
                    Char(c) if c.is_ascii_lowercase() => {
                        if let Some(i) = g.slot_item((c as u8 - b'a') as usize) {
                            self.overlay = Overlay::None;
                            g.do_action(Action::Use(i));
                        }
                    }
                    Char(c) if c.is_ascii_uppercase() => {
                        if let Some(i) = g.slot_item((c.to_ascii_lowercase() as u8 - b'a') as usize) {
                            self.overlay = Overlay::None;
                            g.do_action(Action::Drop(i));
                        }
                    }
                    _ => {}
                }
                return;
            }
            Overlay::Pause => {
                let n = render::PAUSE_ITEMS.len();
                match k.code {
                    Up | Char('w') | Char('k') => self.pause_sel = (self.pause_sel + n - 1) % n,
                    Down | Char('s') | Char('j') => self.pause_sel = (self.pause_sel + 1) % n,
                    Esc => self.overlay = Overlay::None,
                    Left | Right | Char('a') | Char('d') | Enter | Char(' ')
                        if self.pause_sel == 2 =>
                    {
                        let dir = if matches!(k.code, Left | Char('a')) { -1 } else { 1 };
                        let s = self.gov.setting.cycle(dir);
                        self.gov.set(s);
                        self.profile.detail = Some(s.to_u8());
                    }
                    Enter | Char(' ') => match self.pause_sel {
                        0 => self.overlay = Overlay::None,
                        1 => self.overlay = Overlay::Help,
                        3 => {
                            self.overlay = Overlay::None;
                            g.abandon();
                        }
                        _ => self.save_and_quit(),
                    },
                    _ => {}
                }
                return;
            }
            Overlay::Help => {
                self.overlay = Overlay::None;
                return;
            }
            Overlay::Pickup => {
                let n = g.items_here().len();
                match k.code {
                    Esc => self.overlay = Overlay::None,
                    Enter | Char(' ') | Char(',') | Char('g') | Char('G') => {
                        self.overlay = Overlay::None;
                        g.do_action(Action::PickupAll);
                    }
                    Char(c) if c.is_ascii_lowercase() => {
                        let i = (c as u8 - b'a') as usize;
                        if i < n {
                            self.overlay = Overlay::None;
                            g.do_action(Action::PickupOne(i));
                        }
                    }
                    _ => {}
                }
                return;
            }
            Overlay::Map => {
                if matches!(k.code, Esc | Char('m') | Tab | Enter | Char(' ')) {
                    self.overlay = Overlay::None;
                }
                return;
            }
            Overlay::Story => {
                if matches!(k.code, Enter | Esc | Char(' ')) {
                    self.overlay = Overlay::None;
                }
                return;
            }
            Overlay::Raise => {
                let n = g.raise_menu.len();
                let pick = |g: &mut Game, i: usize| {
                    if let Some(o) = g.raise_menu.get(i).copied() {
                        g.raise_menu.clear();
                        g.do_action(Action::RaiseAt(o.x, o.y));
                    }
                };
                match k.code {
                    Up | Char('w') | Char('k') if n > 0 => {
                        self.raise_sel = (self.raise_sel + n - 1) % n
                    }
                    Down | Char('s') | Char('j') if n > 0 => {
                        self.raise_sel = (self.raise_sel + 1) % n
                    }
                    Enter | Char(' ') => {
                        self.overlay = Overlay::None;
                        pick(g, self.raise_sel);
                    }
                    Esc => {
                        self.overlay = Overlay::None;
                        g.raise_menu.clear();
                    }
                    Char(c) if c.is_ascii_lowercase() && ((c as u8 - b'a') as usize) < n => {
                        self.overlay = Overlay::None;
                        pick(g, (c as u8 - b'a') as usize);
                    }
                    _ => {}
                }
                return;
            }
            Overlay::Log => {
                match k.code {
                    Up | Char('w') | Char('k') => self.log_scroll += 1,
                    Down | Char('s') | Char('j') => {
                        self.log_scroll = self.log_scroll.saturating_sub(1)
                    }
                    PageUp => self.log_scroll += 10,
                    PageDown => self.log_scroll = self.log_scroll.saturating_sub(10),
                    Home => self.log_scroll = usize::MAX / 2,
                    End => self.log_scroll = 0,
                    Esc | Char('l') | Char('L') | Enter | Char(' ') | Char('q') => {
                        self.overlay = Overlay::None
                    }
                    _ => {}
                }
                return;
            }
            Overlay::None => {}
        }

        match k.code {
            // Shift+M: swoop between first-person and overhead play.
            Char('M') => {
                g.toggle_view();
                return;
            }
            Tab | Char('m') => {
                self.overlay = Overlay::Map;
                return;
            }
            Char('l') | Char('L') => {
                self.overlay = Overlay::Log;
                self.log_scroll = 0;
                return;
            }
            Char('g') | Char(',') if g.items_here().len() > 1 => {
                self.overlay = Overlay::Pickup;
                return;
            }
            // Shift+Q: listen intently (Keen Hearing).
            Char('Q') => {
                g.do_action(Action::Listen);
                return;
            }
            // Shift+C: Last Rites on or off.
            Char('C') => {
                g.toggle_rites();
                return;
            }
            // Shift+R, so it is hard to switch by accident.
            Char('R') => {
                g.toggle_realtime();
                return;
            }
            Esc => {
                self.overlay = Overlay::Pause;
                self.pause_sel = 0;
                return;
            }
            Char('i') | Char('I') => {
                self.overlay = Overlay::Inventory;
                return;
            }
            Char('?') => {
                self.overlay = Overlay::Help;
                return;
            }
            Char('-') | Char('_') | Char('=') | Char('+') => {
                let d = if matches!(k.code, Char('-') | Char('_')) {
                    -0.1
                } else {
                    0.1
                };
                let v = self.audio.adjust_volume(d);
                self.profile.volume = Some(v);
                if self.audio.enabled() {
                    g.msg(format!("Volume {:.0}%", v * 100.0), GREY);
                } else {
                    g.msg("Audio is unavailable (no sound device found).", GREY);
                }
                return;
            }
            _ => {}
        }

        let f = g.p.facing;
        let action = if g.overhead {
            match k.code {
                Up | Char('w') | Char('k') => Some(Action::Move(0, true)),
                Right | Char('d') => Some(Action::Move(2, true)),
                Down | Char('s') | Char('j') => Some(Action::Move(4, true)),
                Left | Char('a') | Char('h') => Some(Action::Move(6, true)),
                Home | Char('y') => Some(Action::Move(7, true)),
                PageUp | Char('u') => Some(Action::Move(1, true)),
                End | Char('b') => Some(Action::Move(5, true)),
                PageDown | Char('n') => Some(Action::Move(3, true)),
                Char('q') => Some(Action::Turn(-1)),
                Char('e') => Some(Action::Turn(1)),
                _ => None,
            }
        } else {
            match k.code {
                Up | Char('w') => Some(Action::Move(f, false)),
                Down | Char('s') => Some(Action::Move(f + 4, false)),
                Left | Char('a') => Some(Action::Turn(-1)),
                Right | Char('d') => Some(Action::Turn(1)),
                Char('q') => Some(Action::Move(f + 6, false)),
                Char('e') => Some(Action::Move(f + 2, false)),
                Char('z') => Some(Action::Move(f + 7, false)),
                Char('c') => Some(Action::Move(f + 1, false)),
                _ => None,
            }
        };
        let action = action.or(match k.code {
            Char(' ') | Char('f') | Char('F') => Some(Action::Attack),
            Char('.') | Char('5') | Char('x') => Some(Action::Wait),
            Char('g') | Char(',') => Some(Action::Pickup),
            Char('>') | Char('<') | Enter => Some(Action::Stairs),
            Char(c @ '1'..='6') => Some(Action::Skill(c as usize - '1' as usize)),
            Char('p') | Char('P') => Some(Action::QuickHeal),
            Char('t') | Char('T') => Some(Action::Relight),
            _ => None,
        });
        if let Some(a) = action {
            g.do_action(a);
        }
    }

    /// Quitting mid-run saves it so it can be continued next time.
    fn save_and_quit(&mut self) {
        if self.screen == Screen::Game {
            if let Some(g) = &self.game {
                g.save();
            }
        }
        self.quit = true;
    }

    fn continue_game(&mut self) {
        match Game::load() {
            Some(g) => {
                let depth = g.depth;
                self.game = Some(g);
                self.has_save = false;
                self.last_depth = depth;
                self.screen = Screen::Game;
                self.overlay = Overlay::None;
                self.melt = None;
                self.over_timer = 0.0;
                self.ambient_timer = 3.0;
                self.audio.set_music(MusicMode::Dungeon, depth);
                self.audio.set_intensity(0.0);
            }
            None => {
                self.has_save = false;
                self.sel = 1;
                self.note = "The saved run could not be read.".to_string();
            }
        }
    }

    fn start_game(&mut self, class: usize, difficulty: u8) {
        // A new descent replaces any saved run.
        game::delete_save();
        self.has_save = false;
        let g = Game::new(class, self.profile.bonuses(), difficulty);
        self.game = Some(g);
        self.last_depth = 1;
        self.screen = Screen::Game;
        // The first descent of all begins with the legend.
        self.overlay = if self.profile.history.is_empty() && self.profile.runs == 0 {
            Overlay::Story
        } else {
            Overlay::None
        };
        self.melt = None;
        self.over_timer = 0.0;
        self.ambient_timer = 3.0;
        self.audio.set_music(MusicMode::Dungeon, 1);
        self.audio.set_intensity(0.0);
    }

    fn update(&mut self, dt: f32, w: usize, h: usize) {
        self.t += dt;
        self.title_note_t = (self.title_note_t - dt).max(0.0);
        match self.screen {
            Screen::Title => {
                let fh = (h / 3).max(6);
                self.fire.resize(w, fh);
                self.fire.step(true, ((72 / fh).clamp(1, 12)) as u8);
            }
            Screen::Altar => {
                let fh = (h / 5).max(4);
                self.fire.resize(w, fh);
                self.fire.step(true, ((72 / fh).clamp(1, 16)) as u8);
            }
            Screen::Game => self.update_game(dt, w),
            Screen::Dead => {
                if let Some(d) = &mut self.death {
                    d.update(dt, w, h);
                }
                self.end_t += dt;
            }
            Screen::Victory => self.end_t += dt,
            _ => {}
        }
    }

    fn update_game(&mut self, dt: f32, w: usize) {
        let paused = self.overlay != Overlay::None;
        let finished;
        {
            let Some(g) = self.game.as_mut() else { return };
            g.update(dt, paused);
            // Raise Dead found more than one body: ask which.
            if !g.raise_menu.is_empty() && self.overlay == Overlay::None {
                self.overlay = Overlay::Raise;
                self.raise_sel = 0;
            }

            let evs: Vec<game::SoundEv> = g.sounds.drain(..).collect();
            for ev in evs {
                if let Some(sp) = g.spatial_for(ev.sfx, ev.x, ev.y, ev.vol) {
                    self.audio.play_after(ev.sfx, &sp, ev.delay);
                }
            }

            if !paused {
                self.ambient_timer -= dt;
                if self.ambient_timer <= 0.0 {
                    self.ambient_timer = rand::thread_rng().gen_range(2.5..7.0);
                    g.ambient_event();
                }
            }

            if g.depth != self.last_depth {
                self.last_depth = g.depth;
                self.audio.set_music(MusicMode::Dungeon, g.depth);
                let mut rng = rand::thread_rng();
                let offs = (0..w).map(|_| -rng.gen_range(0.0..14.0)).collect();
                self.melt = Some(Melt {
                    snap: self.prev.clone(),
                    offs,
                    t: 0.0,
                });
            }

            let mut inten = g.combat_heat * 0.85;
            if g.p.has_amulet {
                inten += 0.3;
            }
            if g.depth == 10 {
                inten += 0.15;
            }
            self.audio.set_intensity(inten);
            self.audio.set_amulet(g.p.has_amulet);

            if g.over.is_some() {
                self.over_timer += dt;
            }
            finished = self.over_timer > 1.8;
        }

        let mut done_melt = false;
        if let Some(m) = &mut self.melt {
            m.t += dt;
            let speed = dt * 40.0 * (1.0 + m.t * 3.0);
            for o in m.offs.iter_mut() {
                *o += speed;
            }
            let hh = m.snap.h as f32;
            done_melt = m.offs.iter().all(|&o| o > hh);
        }
        if done_melt {
            self.melt = None;
        }
        if finished {
            self.end_run();
        }
    }

    fn end_run(&mut self) {
        let Some(g) = self.game.take() else { return };
        let won = matches!(g.over, Some(Outcome::Won));
        let base = g.max_depth * 10
            + g.p.level as u32 * 5
            + g.kills
            + if won { 250 } else { 0 }
            + if g.p.has_amulet { 50 } else { 0 };
        let dd = g.diff();
        let mult = dd.souls * (1.0 + 0.1 * g.bonus.harvest as f32);
        let pts = (base as f32 * mult).round() as u32;
        let mastered = self.profile.record_kills(&g.slain);
        let unlocked = if won {
            self.profile.record_win(g.difficulty as usize)
        } else {
            None
        };
        let cd = g.cdef();
        let cause_txt = match &g.over {
            Some(Outcome::Died(c)) => c.describe(),
            _ => "escaped with the Amulet of GRAZZARK".to_string(),
        };
        self.profile.points += pts;
        self.profile.total_points += pts;
        self.profile.runs += 1;
        if won {
            self.profile.wins += 1;
        }
        self.profile.best_depth = self.profile.best_depth.max(g.max_depth);
        self.profile.history.push(RunRecord {
            class: cd.name.to_string(),
            level: g.p.level,
            depth: g.max_depth,
            cause: cause_txt.clone(),
            points: pts,
            won,
            turns: g.turns,
            kills: g.kills,
            difficulty: g.difficulty,
        });
        if self.profile.history.len() > 40 {
            self.profile.history.remove(0);
        }
        self.profile.save();

        let mut info = vec![
            format!("{}, level {}  ({})", cd.name, g.p.level, dd.name),
            format!(
                "Reached depth {} of 10. Slew {} foes in {} turns.",
                g.max_depth, g.kills, g.turns
            ),
            cap(&cause_txt),
            if mult != 1.0 {
                format!(
                    "+{} Soul Points (x{:.1})  (you now have {})",
                    pts, mult, self.profile.points
                )
            } else {
                format!(
                    "+{} Soul Points  (you now have {})",
                    pts, self.profile.points
                )
            },
        ];
        if let Some(n) = unlocked {
            info.push(format!("{} difficulty is now unlocked!", n.to_uppercase()));
        }
        for m in mastered.iter().take(3) {
            info.push(format!(
                "Bestiary: you have mastered the {}! (+crit against it)",
                m
            ));
        }
        self.end_t = 0.0;
        self.overlay = Overlay::None;
        self.melt = None;
        self.over_timer = 0.0;
        if won {
            self.end_info = info;
            self.screen = Screen::Victory;
            self.audio.set_music(MusicMode::Victory, 1);
            self.audio.set_intensity(0.0);
            self.audio.play(Sfx::LevelUp, &Spatial::center(0.9));
        } else {
            let cause = match g.over {
                Some(Outcome::Died(c)) => c,
                _ => DeathCause::Abandoned,
            };
            self.death = Some(DeathScreen::new(&cause, info));
            self.screen = Screen::Dead;
            self.audio.set_music(MusicMode::Death, 1);
            self.audio.set_intensity(0.0);
        }
    }

    fn draw(&self, c: &mut Canvas) {
        c.clear();
        match self.screen {
            Screen::Title => {
                screens::draw_title(c, &self.fire, self.t, self.sel, &self.profile, self.has_save);
                if self.title_note_t > 0.0 {
                    let k = self.title_note_t.min(1.0);
                    c.center(c.h as i32 - 2, &self.title_note, Rgb(255, 230, 150).scale(k));
                }
            }
            Screen::ClassSel => {
                screens::draw_class_select(c, self.class_sel, &self.profile, self.t)
            }
            Screen::DiffSel => {
                screens::draw_difficulty(c, self.diff_sel, self.class_sel, &self.profile, self.t)
            }
            Screen::Bestiary => screens::draw_bestiary(c, self.best_sel, &self.profile, self.t),
            Screen::Altar => screens::draw_altar(
                c,
                self.altar_sel,
                &self.profile,
                self.t,
                &self.note,
                &self.fire,
            ),
            Screen::Hall => screens::draw_hall(c, &self.profile),
            Screen::Story => screens::draw_story(c, self.t),
            Screen::Profiles => screens::draw_profiles(
                c,
                &self.profiles,
                self.prof_sel,
                self.loaded.then_some(self.profile.name.as_str()),
                self.t,
            ),
            Screen::NewProfile => screens::draw_new_profile(
                c,
                &self.name_buf,
                &self.name_err,
                self.profiles.is_empty() && !self.loaded,
                self.t,
            ),
            Screen::Help => {
                c.fill(0, 0, c.w as i32, c.h as i32, ' ', Rgb::WHITE, Rgb(6, 6, 10));
                render::draw_help(c);
            }
            Screen::Game => {
                if let Some(g) = &self.game {
                    render::render_game(c, g, self.t);
                    if let Some(m) = &self.melt {
                        let w = c.w.min(m.snap.w);
                        let h = c.h as i32;
                        for x in 0..w {
                            let shift = m.offs[x.min(m.offs.len() - 1)].max(0.0) as i32;
                            for y in shift..h {
                                let sy = y - shift;
                                if sy < m.snap.h as i32 {
                                    let cell = m.snap.get(x as i32, sy);
                                    c.set(x as i32, y, cell.ch, cell.fg, cell.bg);
                                }
                            }
                        }
                    }
                    match self.overlay {
                        Overlay::Inventory => render::draw_inventory(c, g),
                        Overlay::Pause => {
                            c.darken(0.5);
                            render::draw_pause(c, self.pause_sel, &self.gov.label());
                        }
                        Overlay::Help => render::draw_help(c),
                        Overlay::Map => render::draw_map(c, g),
                        Overlay::Log => render::draw_log(c, g, self.log_scroll),
                        Overlay::Raise => render::draw_raise(c, g, self.raise_sel, &self.profile),
                        Overlay::Story => screens::draw_story(c, self.t),
                        Overlay::Pickup => render::draw_pickup(c, g),
                        Overlay::None => {}
                    }
                    if let Some(o) = &g.over {
                        let k = (self.over_timer / 1.8).min(1.0);
                        let col = match o {
                            Outcome::Won => Rgb(255, 240, 200),
                            Outcome::Died(_) => Rgb(110, 0, 0),
                        };
                        let (w, h) = (c.w as i32, c.h as i32);
                        c.tint_region(0, 0, w, h, col, k * 0.75);
                    }
                }
            }
            Screen::Dead => {
                if let Some(d) = &self.death {
                    d.draw(c);
                }
            }
            Screen::Victory => screens::draw_victory(c, self.end_t, &self.end_info),
        }
    }
}

/// Feed one typed letter into a secret word being entered. Returns true
/// when the word is complete (and starts over).
fn type_code(buf: &mut String, ch: char, word: &str) -> bool {
    let ch = ch.to_ascii_lowercase();
    buf.push(ch);
    if !word.starts_with(buf.as_str()) {
        // Start again, keeping this letter if it begins the word.
        buf.clear();
        if word.starts_with(ch) {
            buf.push(ch);
        }
    }
    if buf == word {
        buf.clear();
        return true;
    }
    false
}

fn print_usage() {
    println!("GRAZZARK - a first-person ASCII roguelike");
    println!();
    println!("USAGE: grazzark [--no-audio] [--256] [--truecolor] [--detail=LEVEL]");
    println!("  --no-audio    run silently");
    println!("  --256         use 256-colour mode (for terminals without true colour)");
    println!("  --truecolor   force 24-bit colour");
    println!("  --detail=auto|high|low|minimal");
    println!("                graphics detail (default auto: measures frame times and");
    println!("                lowers detail if the machine can't keep up; minimal is");
    println!("                bare bones for very slow CPUs). Also in the pause menu.");
    let sys = perf::System::detect();
    println!();
    println!("This machine: {} CPU threads, the 3D view uses {}.", sys.cores, sys.render_threads());
    println!();
    println!(
        "Profiles (soul points, upgrades, history): {}",
        profile::base_dir().join("profiles").display()
    );
}

/// Only use 24-bit colour when the terminal says it supports it. `TERM=xterm`
/// on its own proves nothing: old terminals (e.g. roxterm-gtk2 on VTE 0.28)
/// read `ESC[38;2;R;G;Bm` as separate attributes (bold, blink, reverse...)
/// and garble the screen, so the 256-colour palette is the safe default.
fn detect_truecolor(term_env: &str) -> bool {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    let colorterm = var("COLORTERM").to_ascii_lowercase();
    if colorterm == "truecolor" || colorterm == "24bit" {
        return true;
    }
    if term_env.contains("direct") || term_env.contains("truecolor") {
        return true;
    }
    // VTE has supported 24-bit colour since 0.36 (VTE_VERSION=3600).
    if var("VTE_VERSION").parse::<u32>().map_or(false, |v| v >= 3600) {
        return true;
    }
    // Terminals known to support it that don't always set COLORTERM.
    let known = ["KITTY_WINDOW_ID", "ALACRITTY_SOCKET", "ALACRITTY_LOG", "WEZTERM_EXECUTABLE",
        "KONSOLE_VERSION", "WT_SESSION"];
    if known.iter().any(|k| std::env::var_os(k).is_some()) {
        return true;
    }
    matches!(var("TERM_PROGRAM").as_str(), "iTerm.app" | "WezTerm" | "vscode" | "ghostty")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return;
    }
    // Double-clicked on a Linux desktop: reopen inside a terminal window.
    if launch::ensure_terminal(&args) {
        return;
    }
    launch::grow_console();
    let no_audio = args.iter().any(|a| a == "--no-audio" || a == "--mute");
    let term_env = std::env::var("TERM").unwrap_or_default();
    let truecolor = if args.iter().any(|a| a == "--truecolor") {
        true
    } else if args.iter().any(|a| a == "--256") {
        false
    } else {
        // Windows 10+ consoles and Windows Terminal speak 24-bit colour.
        cfg!(windows) || detect_truecolor(&term_env)
    };

    profile::migrate_legacy_in(&profile::base_dir());
    let profile = Profile::default();
    silence_stderr();
    let audio = Audio::new(!no_audio);
    if let Some(v) = profile.volume {
        audio.set_volume(v);
    }
    audio.set_music(MusicMode::Title, 1);

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        Term::restore();
        restore_stderr();
        default_hook(info);
        launch::pause_before_exit();
    }));

    let mut term = match Term::new(truecolor) {
        Ok(t) => t,
        Err(e) => {
            restore_stderr();
            eprintln!("Could not initialise the terminal: {}", e);
            launch::pause_before_exit();
            return;
        }
    };
    let (w, h) = Term::size();
    let cli_detail = args
        .iter()
        .find_map(|a| a.strip_prefix("--detail="))
        .and_then(perf::Setting::parse);
    let gov = perf::Governor::new(&perf::System::detect(), cli_detail.unwrap_or(perf::Setting::Auto));
    let mut app = App::new(profile, audio, w, h, gov, cli_detail.is_some());
    app.choose_profile_at_start();
    let mut canvas = Canvas::new(w, h);
    let mut last = Instant::now();
    // Exact key reporting (left vs right Shift/Ctrl) is only switched on while
    // the title screen is up, so play keeps the terminal's usual keys.
    let exact_ok = Term::exact_keys_supported();

    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1);
        last = now;

        while event::poll(Duration::ZERO).unwrap_or(false) {
            match event::read() {
                // Modifier keys on their own only matter to the title code.
                Ok(Event::Key(KeyEvent { code: KeyCode::Modifier(m), kind, .. })) => {
                    let down = kind != KeyEventKind::Release;
                    match m {
                        ModifierKeyCode::LeftShift => app.held_lshift = down,
                        ModifierKeyCode::RightControl => app.held_rctrl = down,
                        _ => {}
                    }
                }
                Ok(Event::Key(k)) if k.kind != KeyEventKind::Release => app.on_key(k),
                Ok(Event::Resize(..)) => term.invalidate(),
                _ => {}
            }
            if app.quit {
                break;
            }
        }
        if app.quit {
            break;
        }

        let want_exact = exact_ok && app.screen == Screen::Title;
        if want_exact != app.exact_keys {
            Term::set_exact_keys(want_exact);
            app.exact_keys = want_exact;
            app.held_lshift = false;
            app.held_rctrl = false;
            app.cheat_buf.clear();
        }
        let (w, h) = Term::size();
        canvas.resize(w, h);
        if w < 80 || h < 24 {
            canvas.clear();
            canvas.center(
                h as i32 / 2,
                "Please enlarge your terminal to at least 80x24.",
                Rgb(255, 200, 120),
            );
            canvas.center(
                h as i32 / 2 + 1,
                &format!("(currently {}x{})", w, h),
                Rgb(160, 160, 160),
            );
        } else {
            app.update(dt, w, h);
            app.draw(&mut canvas);
        }
        term.coarse = perf::detail() == perf::Detail::Minimal;
        let _ = term.present(&canvas);
        app.prev = canvas.clone();

        let el = now.elapsed();
        let busy = app.screen == Screen::Game && app.game.is_some();
        app.gov.record(el, dt, busy);
        let frame = perf::detail().frame_time();
        if el < frame {
            let _ = event::poll(frame - el);
        }
    }

    app.profile.save();
    drop(app);
    Term::restore();
    restore_stderr();
    println!(
        "Farewell, adventurer. Your soul is bound to {}",
        Profile::path().display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::CLASSES;
    use crate::map::Tile;
    use crate::profile::Bonuses;

    /// Plays random actions for every class, forcing descents through all ten
    /// levels and back, rendering both views each step. Catches panics in game
    /// logic and rendering.
    #[test]
    fn soak_all_classes() {
        let mut rng = rand::thread_rng();
        for class in 0..CLASSES.len() {
            for _run in 0..3 {
                let mut bonus = Bonuses {
                    xp_mult: 1.0,
                    ..Default::default()
                };
                bonus.skills = UPGRADES.iter().filter_map(|u| u.skill).collect();
                // Exercise every boon as well.
                bonus.bloodlust = 3;
                bonus.pockets = 2;
                bonus.carto = 2;
                bonus.anchor = true;
                bonus.coup = 3;
                bonus.alchemy = 3;
                bonus.thorns = 3;
                bonus.bloodmagic = true;
                bonus.hunter = 3;
                bonus.heirloom = 2;
                bonus.ironwill = 1 + (_run as i32 % 2);
                bonus.leech = 2;
                bonus.laststand = 3;
                bonus.fleet = 2;
                bonus.scavenger = 2;
                bonus.glint = true;
                bonus.belt = 2;
                bonus.appraise = true;
                bonus.gulp = true;
                bonus.pyre = 5;
                bonus.mastered = vec!["giant rat".into(), "orc".into()];
                let mut g = Game::new(class, bonus, ((_run + class) % 4) as u8);
                // Every floor gets its unique, and the hero wears a rotating
                // set of unique gear so every power is exercised.
                g.unique_chance = [1.0; 10];
                let n_u = data::UNIQUES.len();
                for k in 0..4 {
                    let u = (class * 3 + _run * 5 + k * 4) % n_u;
                    match data::unique_item(u) {
                        data::Item::Weapon(gr) => g.p.wpn = gr,
                        data::Item::Armor(gr) => g.p.arm = gr,
                        data::Item::Trinket(t) => {
                            if data::UNIQUES[u].slot == data::USlot::Ring {
                                g.p.ring = Some(t)
                            } else {
                                g.p.neck = Some(t)
                            }
                        }
                        _ => {}
                    }
                }
                g.p.inv.push(data::unique_item((class + _run) % n_u));
                let mut canvas = Canvas::new(140, 45);
                let mut t = 0.0f32;
                let mut went_down = true;
                for step in 0..4200 {
                    if g.over.is_some() {
                        break;
                    }
                    // keep the hero alive long enough to see deep levels
                    g.p.max_hp = g.p.max_hp.max(400);
                    g.p.hp = g.p.hp.max(g.p.max_hp / 2);
                    g.p.mp = g.p.max_mp;
                    if step % 200 == 199 {
                        g.p.poison = 0;
                        let target = if went_down { g.level.down } else { g.level.up };
                        g.p.x = target.0;
                        g.p.y = target.1;
                        g.refresh_view();
                        g.p.next_act = g.time;
                        g.do_action(Action::Stairs);
                        if g.depth == 10 {
                            went_down = false;
                            // grab the amulet for the trip back
                            if let Some(i) = g
                                .level
                                .items
                                .iter()
                                .position(|i| i.item == data::Item::Amulet)
                            {
                                let (ax, ay) = (g.level.items[i].x, g.level.items[i].y);
                                g.p.x = ax;
                                g.p.y = ay;
                                g.refresh_view();
                                g.p.next_act = g.time;
                                g.do_action(Action::Pickup);
                            }
                        }
                    }
                    if step == 1000 {
                        g.toggle_realtime();
                    }
                    if step % 37 == 0 && !g.level.monsters.is_empty() {
                        let k = rng.gen_range(0..g.level.monsters.len());
                        if g.level.monsters[k].alive && !g.level.monsters[k].champion {
                            g.make_champion(k);
                        } else if g.level.monsters[k].alive && !g.level.monsters[k].overlord {
                            g.make_overlord(k);
                        }
                    }
                    if step % 90 == 0 {
                        g.p.vision = 10;
                    }
                    if g.p.has_amulet && step % 60 == 0 {
                        let c = game::Curse::ALL[(step / 60) % game::Curse::ALL.len()];
                        g.start_curse(c, 30);
                    }
                    let a = match rng.gen_range(0..14) {
                        0..=4 => Action::Move(rng.gen_range(0..8), rng.gen_bool(0.5)),
                        5 => Action::Turn(if rng.gen_bool(0.5) { 1 } else { -1 }),
                        6 | 7 => Action::Attack,
                        8 => Action::Wait,
                        9 => match rng.gen_range(0..3) {
                            0 => Action::Pickup,
                            1 => Action::PickupAll,
                            _ => Action::PickupOne(rng.gen_range(0..3)),
                        },
                        10 => Action::Skill(rng.gen_range(0..6)),
                        11 => Action::Use(rng.gen_range(0..4)),
                        12 => Action::Drop(rng.gen_range(0..20)),
                        _ => Action::QuickHeal,
                    };
                    g.do_action(a);
                    for _ in 0..2 {
                        g.update(0.033, false);
                        t += 0.033;
                    }
                    for ev in g.sounds.drain(..).collect::<Vec<_>>() {
                        let _ = g.spatial(ev.x, ev.y, ev.vol);
                    }
                    if step % 7 == 0 {
                        g.ambient_event();
                    }
                    g.overhead = step % 2 == 0;
                    canvas.clear();
                    render::render_game(&mut canvas, &g, t);
                    if step % 50 == 0 {
                        render::draw_map(&mut canvas, &g);
                        render::draw_inventory(&mut canvas, &g);
                        render::draw_pickup(&mut canvas, &g);
                        render::draw_log(&mut canvas, &g, step / 50);
                    }
                    if !g.raise_menu.is_empty() {
                        render::draw_raise(&mut canvas, &g, 0, &Profile::default());
                        let o = g.raise_menu[g.raise_menu.len() - 1];
                        g.do_action(Action::RaiseAt(o.x, o.y));
                    }
                    if g.p.torch_out && step % 7 == 0 {
                        g.do_action(Action::Relight);
                    }
                    for m in g.level.monsters.iter() {
                        if m.alive && !m.def().ghost {
                            assert!(
                                g.level.tile(m.x, m.y) != Tile::Wall,
                                "{} inside a wall",
                                m.def().name
                            );
                        }
                    }
                    assert!(
                        g.level.tile(g.p.x, g.p.y) != Tile::Wall,
                        "player inside a wall"
                    );
                }
                eprintln!("class {} max_depth {} depth {} lvl {} kills {} amulet {} uniques {:?} outcome {}", class, g.max_depth, g.depth, g.p.level, g.kills, g.p.has_amulet, g.found_uniques,
                    match &g.over { Some(Outcome::Won) => "WON".to_string(), Some(Outcome::Died(c)) => c.describe(), None => "alive".into() });
            }
        }
    }

    #[test]
    fn the_title_code_is_typed_in_full() {
        let mut buf = String::new();
        let typed = |buf: &mut String, s: &str| s.chars().map(|c| type_code(buf, c, "stewardess")).last().unwrap();
        assert!(typed(&mut buf, "STEWARDESS"), "case does not matter (Shift is held)");
        assert!(!typed(&mut buf, "stewardes"));
        assert!(typed(&mut buf, "s"));
        assert!(typed(&mut buf, "stewsstewardess"), "a slip starts it over");
        assert!(!typed(&mut buf, "stewardesx"));
    }

    #[test]
    fn menu_screens_render() {
        let mut prof = Profile::default();
        prof.points = 1234;
        prof.unlocked_diff = 1;
        for (i, m) in data::MONSTERS.iter().enumerate() {
            prof.bestiary
                .insert(m.name.to_string(), [0, 1, 10, 100, 250][i % 5]);
        }
        let fire = screens::Fire::new();
        for &(w, h) in &[(80usize, 24usize), (120, 40), (200, 60)] {
            let mut c = Canvas::new(w, h);
            screens::draw_story(&mut c, 1.0);
            let entries: Vec<profile::ProfileEntry> = (0..30)
                .map(|i| profile::ProfileEntry {
                    name: format!("Hero {}", i),
                    dir: std::path::PathBuf::new(),
                    runs: i,
                    wins: i / 3,
                    points: i * 100,
                    has_save: i % 4 == 0,
                })
                .collect();
            for sel in [0, 5, 29] {
                screens::draw_profiles(&mut c, &entries, sel, Some("Hero 5"), 1.0);
                screens::draw_profiles(&mut c, &entries, sel, None, 1.0);
            }
            screens::draw_new_profile(
                &mut c,
                "Grimwald",
                "There is already a profile called Grimwald.",
                true,
                1.0,
            );
            screens::draw_new_profile(&mut c, "", "", false, 1.0);
            let mut c = Canvas::new(w, h);
            for sel in 0..UPGRADES.len() {
                screens::draw_altar(&mut c, sel, &prof, 1.0, "note", &fire);
            }
            for sel in 0..data::MONSTERS.len() {
                screens::draw_bestiary(&mut c, sel, &prof, 1.0);
            }
            for sel in 0..DIFFICULTIES.len() {
                screens::draw_difficulty(&mut c, sel, sel % CLASSES.len(), &prof, 1.0);
            }
            for sel in 0..CLASSES.len() {
                screens::draw_class_select(&mut c, sel, &prof, 1.0);
            }
            for sel in 0..screens::TITLE_ITEMS.len() {
                screens::draw_title(&mut c, &fire, 1.0, sel, &prof, true);
            }
            screens::draw_hall(&mut c, &prof);
        }
    }

    #[test]
    fn death_screens_render() {
        let causes = vec![
            DeathCause::Monster(0),
            DeathCause::Monster(4),
            DeathCause::Monster(12),
            DeathCause::Monster(18),
            DeathCause::Spell(17),
            DeathCause::OwnFire,
            DeathCause::Poison("a giant spider".into()),
            DeathCause::Trap(map::TrapKind::Spikes),
            DeathCause::Trap(map::TrapKind::Gas),
            DeathCause::Abandoned,
        ];
        let mut c = Canvas::new(120, 40);
        let mut causes = causes;
        for k in data::M_UNIQUE0..data::MONSTERS.len() {
            causes.push(DeathCause::Monster(k));
            causes.push(DeathCause::Spell(k));
        }
        for cause in causes {
            let mut d = DeathScreen::new(&cause, vec!["Warrior, level 3".into(), "x".into()]);
            for _ in 0..120 {
                d.update(0.033, 120, 40);
                c.clear();
                d.draw(&mut c);
            }
        }
        for i in 0..100 {
            screens::draw_victory(&mut c, i as f32 * 0.1, &["a".to_string()]);
        }
    }

    #[test]
    fn all_sounds_synthesise() {
        use crate::audio::{spatialize, Mood};
        use crate::data::VoiceKind::*;
        let sp = Spatial {
            gain: 0.5,
            pan: 0.3,
            cutoff: 900.0,
            echoes: vec![(0.05, 0.3, -0.5)],
            reverb: 0.6,
            room: 0.8,
        };
        let voices = [
            Squeak, Screech, Yap, Rattle, Moan, Hiss, Growl, Snarl, Roar, Wail, Chant, Cackle,
            Demon, Human, Gurgle, Chitter, Grind, Crackle, Chime, Spore,
        ];
        let moods = [Mood::Alert, Mood::Attack, Mood::Hurt, Mood::Die, Mood::Idle];
        let mut all = vec![
            Sfx::Step,
            Sfx::Swing,
            Sfx::Hit,
            Sfx::Clang,
            Sfx::Miss,
            Sfx::PlayerHurt,
            Sfx::PlayerDie,
            Sfx::Drip,
            Sfx::Rumble,
            Sfx::Chains,
            Sfx::Whisper,
            Sfx::Creak,
            Sfx::Wind,
            Sfx::Bones,
            Sfx::Fireball,
            Sfx::Explosion,
            Sfx::Frost,
            Sfx::Arrow,
            Sfx::Bolt,
            Sfx::Heal,
            Sfx::LevelUp,
            Sfx::Pickup,
            Sfx::Stairs,
            Sfx::Revive,
            Sfx::Trap,
            Sfx::Amulet,
            Sfx::Blink,
            Sfx::Stomp,
            Sfx::Menu,
            Sfx::GritThrow,
            Sfx::GritHit,
        ];
        for v in voices {
            for m in moods {
                all.push(Sfx::Voice(v, m));
            }
        }
        use crate::data::Material;
        for m in [
            Material::Flesh,
            Material::Ooze,
            Material::Bone,
            Material::Shell,
            Material::Stone,
            Material::Crystal,
            Material::Armor,
            Material::Spirit,
            Material::Flame,
            Material::Wall,
        ] {
            all.push(Sfx::Impact(m));
        }
        let mut rng = rand::thread_rng();
        for s in all {
            let mono = audio::synth_for_test(s, 44100.0, &mut rng);
            assert!(!mono.is_empty());
            assert!(mono.iter().all(|x| x.is_finite()), "{:?} produced NaN", s);
            let st = spatialize(&mono, &sp, 44100.0);
            assert!(
                st.iter().all(|x| x.is_finite() && x.abs() < 50.0),
                "{:?} blew up",
                s
            );
        }
    }
}

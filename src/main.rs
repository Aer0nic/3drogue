//! GRAZZARK: a first-person ASCII roguelike.
//!
//! Descend ten randomly generated levels, take the Amulet of GRAZZARK from
//! its Warden, and climb back to the surface alive. Death earns soul points
//! that buy permanent upgrades on the Soul Altar.

mod audio;
mod canvas;
mod color;
mod data;
mod entity;
mod fx;
mod game;
mod map;
mod profile;
mod render;
mod screens;
mod term;

use audio::{Audio, MusicMode, Sfx, Spatial};
use canvas::Canvas;
use color::Rgb;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use data::{CLASSES, UPGRADES};
use game::{cap, Action, DeathCause, Game, Outcome, GREY};
use profile::{Profile, RunRecord};
use rand::Rng;
use screens::{DeathScreen, Fire};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};
use term::Term;

static SAVED_STDERR: AtomicI32 = AtomicI32::new(-1);

/// The ALSA backend can print diagnostics to stderr, which would scribble
/// over the game screen. Send stderr to /dev/null while the game runs.
fn silence_stderr() {
    unsafe {
        let saved = libc::dup(2);
        SAVED_STDERR.store(saved, Ordering::SeqCst);
        let devnull = libc::open(b"/dev/null\0".as_ptr() as *const libc::c_char, libc::O_WRONLY);
        if devnull >= 0 {
            libc::dup2(devnull, 2);
            libc::close(devnull);
        }
    }
}

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
    Altar,
    Hall,
    Help,
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
    altar_sel: usize,
    pause_sel: usize,
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
}

impl App {
    fn new(profile: Profile, audio: Audio, w: usize, h: usize) -> App {
        App {
            screen: Screen::Title,
            sel: 0,
            class_sel: 0,
            altar_sel: 0,
            pause_sel: 0,
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
        }
    }

    fn click(&self) {
        self.audio.play(Sfx::Menu, &Spatial::center(0.5));
    }

    fn on_key(&mut self, k: KeyEvent) {
        let code = k.code;
        if k.modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        let up = matches!(code, KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k'));
        let down = matches!(code, KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j'));
        let enter = matches!(code, KeyCode::Enter | KeyCode::Char(' '));
        let esc = matches!(code, KeyCode::Esc | KeyCode::Char('q'));
        match self.screen {
            Screen::Title => {
                let n = screens::TITLE_ITEMS.len();
                if up {
                    self.sel = (self.sel + n - 1) % n;
                    self.click();
                } else if down {
                    self.sel = (self.sel + 1) % n;
                    self.click();
                } else if enter {
                    self.click();
                    match self.sel {
                        0 => self.screen = Screen::ClassSel,
                        1 => {
                            self.note.clear();
                            self.screen = Screen::Altar;
                        }
                        2 => self.screen = Screen::Hall,
                        3 => self.screen = Screen::Help,
                        _ => self.quit = true,
                    }
                } else if esc {
                    self.quit = true;
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
                        self.start_game(self.class_sel);
                    }
                } else if esc {
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
            Screen::Hall | Screen::Help => {
                if esc || enter {
                    self.screen = Screen::Title;
                }
            }
            Screen::Game => self.game_key(k),
            Screen::Dead | Screen::Victory => {
                if (enter || matches!(code, KeyCode::Esc)) && self.end_t > 1.5 {
                    self.screen = Screen::Title;
                    self.sel = 0;
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
                    Char(c) if c.is_ascii_lowercase() => {
                        let i = (c as u8 - b'a') as usize;
                        if i < g.p.inv.len() {
                            self.overlay = Overlay::None;
                            g.do_action(Action::Use(i));
                        }
                    }
                    Char(c) if c.is_ascii_uppercase() => {
                        let i = (c.to_ascii_lowercase() as u8 - b'a') as usize;
                        if i < g.p.inv.len() {
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
                    Enter | Char(' ') => match self.pause_sel {
                        0 => self.overlay = Overlay::None,
                        1 => self.overlay = Overlay::Help,
                        2 => {
                            self.overlay = Overlay::None;
                            g.abandon();
                        }
                        _ => self.quit = true,
                    },
                    _ => {}
                }
                return;
            }
            Overlay::Help => {
                self.overlay = Overlay::None;
                return;
            }
            Overlay::None => {}
        }

        match k.code {
            Tab | Char('m') | Char('M') => {
                g.overhead = !g.overhead;
                return;
            }
            Char('r') | Char('R') => {
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
                let d = if matches!(k.code, Char('-') | Char('_')) { -0.1 } else { 0.1 };
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
                Right | Char('d') | Char('l') => Some(Action::Move(2, true)),
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
            _ => None,
        });
        if let Some(a) = action {
            g.do_action(a);
        }
    }

    fn start_game(&mut self, class: usize) {
        let g = Game::new(class, self.profile.bonuses());
        self.game = Some(g);
        self.last_depth = 1;
        self.screen = Screen::Game;
        self.overlay = Overlay::None;
        self.melt = None;
        self.over_timer = 0.0;
        self.ambient_timer = 3.0;
        self.audio.set_music(MusicMode::Dungeon, 1);
        self.audio.set_intensity(0.0);
    }

    fn update(&mut self, dt: f32, w: usize, h: usize) {
        self.t += dt;
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

            let evs: Vec<game::SoundEv> = g.sounds.drain(..).collect();
            for ev in evs {
                if let Some(sp) = g.spatial(ev.x, ev.y, ev.vol) {
                    self.audio.play(ev.sfx, &sp);
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
                self.melt = Some(Melt { snap: self.prev.clone(), offs, t: 0.0 });
            }

            let mut inten = g.combat_heat * 0.85;
            if g.p.has_amulet {
                inten += 0.3;
            }
            if g.depth == 10 {
                inten += 0.15;
            }
            self.audio.set_intensity(inten);

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
        let pts = g.max_depth * 10
            + g.p.level as u32 * 5
            + g.kills
            + if won { 250 } else { 0 }
            + if g.p.has_amulet { 50 } else { 0 };
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
        });
        if self.profile.history.len() > 40 {
            self.profile.history.remove(0);
        }
        self.profile.save();

        let info = vec![
            format!("{}, level {}", cd.name, g.p.level),
            format!("Reached depth {} of 10. Slew {} foes in {} turns.", g.max_depth, g.kills, g.turns),
            cap(&cause_txt),
            format!("+{} Soul Points  (you now have {})", pts, self.profile.points),
        ];
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
            Screen::Title => screens::draw_title(c, &self.fire, self.t, self.sel, &self.profile),
            Screen::ClassSel => screens::draw_class_select(c, self.class_sel, &self.profile, self.t),
            Screen::Altar => screens::draw_altar(c, self.altar_sel, &self.profile, self.t, &self.note, &self.fire),
            Screen::Hall => screens::draw_hall(c, &self.profile),
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
                            render::draw_pause(c, self.pause_sel);
                        }
                        Overlay::Help => render::draw_help(c),
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

fn print_usage() {
    println!("GRAZZARK - a first-person ASCII roguelike");
    println!();
    println!("USAGE: grazzark [--no-audio] [--256] [--truecolor]");
    println!("  --no-audio    run silently");
    println!("  --256         use 256-colour mode (for terminals without true colour)");
    println!("  --truecolor   force 24-bit colour");
    println!();
    println!("Profile (soul points, upgrades, history): {}", Profile::path().display());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return;
    }
    let no_audio = args.iter().any(|a| a == "--no-audio" || a == "--mute");
    let term_env = std::env::var("TERM").unwrap_or_default();
    let truecolor = if args.iter().any(|a| a == "--truecolor") {
        true
    } else if args.iter().any(|a| a == "--256") {
        false
    } else {
        term_env != "linux" && !term_env.is_empty()
    };

    let profile = Profile::load();
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
    }));

    let mut term = match Term::new(truecolor) {
        Ok(t) => t,
        Err(e) => {
            restore_stderr();
            eprintln!("Could not initialise the terminal: {}", e);
            return;
        }
    };
    let (w, h) = Term::size();
    let mut app = App::new(profile, audio, w, h);
    let mut canvas = Canvas::new(w, h);
    let frame = Duration::from_millis(33);
    let mut last = Instant::now();

    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1);
        last = now;

        while event::poll(Duration::ZERO).unwrap_or(false) {
            match event::read() {
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

        let (w, h) = Term::size();
        canvas.resize(w, h);
        if w < 80 || h < 24 {
            canvas.clear();
            canvas.center(h as i32 / 2, "Please enlarge your terminal to at least 80x24.", Rgb(255, 200, 120));
            canvas.center(h as i32 / 2 + 1, &format!("(currently {}x{})", w, h), Rgb(160, 160, 160));
        } else {
            app.update(dt, w, h);
            app.draw(&mut canvas);
        }
        let _ = term.present(&canvas);
        app.prev = canvas.clone();

        let el = now.elapsed();
        if el < frame {
            let _ = event::poll(frame - el);
        }
    }

    app.profile.save();
    drop(app);
    Term::restore();
    restore_stderr();
    println!("Farewell, adventurer. Your soul is bound to {}", Profile::path().display());
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
                let mut bonus = Bonuses { xp_mult: 1.0, ..Default::default() };
                bonus.skills = vec![data::Skill::Blink, data::Skill::SoulBurst, data::Skill::PhaseWard, data::Skill::SecondWind];
                let mut g = Game::new(class, bonus);
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
                            if let Some(i) = g.level.items.iter().position(|i| i.item == data::Item::Amulet) {
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
                    let a = match rng.gen_range(0..14) {
                        0..=4 => Action::Move(rng.gen_range(0..8), rng.gen_bool(0.5)),
                        5 => Action::Turn(if rng.gen_bool(0.5) { 1 } else { -1 }),
                        6 | 7 => Action::Attack,
                        8 => Action::Wait,
                        9 => Action::Pickup,
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
                    assert!(g.level.tile(g.p.x, g.p.y) != Tile::Wall, "player inside a wall");
                }
                eprintln!("class {} max_depth {} depth {} lvl {} kills {} amulet {} outcome {}", class, g.max_depth, g.depth, g.p.level, g.kills, g.p.has_amulet,
                    match &g.over { Some(Outcome::Won) => "WON".to_string(), Some(Outcome::Died(c)) => c.describe(), None => "alive".into() });
            }
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
        let sp = Spatial { gain: 0.5, pan: 0.3, cutoff: 900.0, echoes: vec![(0.05, 0.3, -0.5)], reverb: 0.6, room: 0.8 };
        let voices = [Squeak, Screech, Yap, Rattle, Moan, Hiss, Growl, Snarl, Roar, Wail, Chant, Cackle, Demon, Human];
        let moods = [Mood::Alert, Mood::Attack, Mood::Hurt, Mood::Die, Mood::Idle];
        let mut all = vec![
            Sfx::Step, Sfx::Swing, Sfx::Hit, Sfx::Clang, Sfx::Miss, Sfx::PlayerHurt, Sfx::PlayerDie, Sfx::Drip,
            Sfx::Rumble, Sfx::Chains, Sfx::Whisper, Sfx::Creak, Sfx::Wind, Sfx::Bones, Sfx::Fireball, Sfx::Explosion,
            Sfx::Frost, Sfx::Arrow, Sfx::Bolt, Sfx::Heal, Sfx::LevelUp, Sfx::Pickup, Sfx::Stairs, Sfx::Revive,
            Sfx::Trap, Sfx::Amulet, Sfx::Blink, Sfx::Stomp, Sfx::Menu,
        ];
        for v in voices {
            for m in moods {
                all.push(Sfx::Voice(v, m));
            }
        }
        let mut rng = rand::thread_rng();
        for s in all {
            let mono = audio::synth_for_test(s, 44100.0, &mut rng);
            assert!(!mono.is_empty());
            assert!(mono.iter().all(|x| x.is_finite()), "{:?} produced NaN", s);
            let st = spatialize(&mono, &sp, 44100.0);
            assert!(st.iter().all(|x| x.is_finite() && x.abs() < 50.0), "{:?} blew up", s);
        }
    }
}

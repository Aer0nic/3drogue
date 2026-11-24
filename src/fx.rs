//! Purely visual effects living in world space: particles, projectiles,
//! floating combat text, dynamic lights, screen flash and shake.

use crate::color::Rgb;
use crate::data::Bleed;
use rand::Rng;
use std::f32::consts::TAU;

pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub life: f32,
    pub max: f32,
    pub ch: char,
    pub color: Rgb,
    /// Glowing particles ignore scene lighting.
    pub glow: bool,
    pub grav: f32,
    pub drag: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BoltEnd {
    None,
    Sparks(Rgb),
    Explosion(Rgb),
    Ice,
    /// An arrow strikes a creature: it sprays whatever that creature bleeds.
    Wound(Bleed),
    Magic(Rgb),
}

pub struct Bolt {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub t: f32,
    pub dur: f32,
    pub ch: char,
    pub color: Rgb,
    pub light: Option<[f32; 3]>,
    pub end: BoltEnd,
    pub z: f32,
}

impl Bolt {
    pub fn pos_at(&self, t: f32) -> (f32, f32) {
        let k = (t / self.dur).clamp(0.0, 1.0);
        (
            self.x0 + (self.x1 - self.x0) * k,
            self.y0 + (self.y1 - self.y0) * k,
        )
    }
}

pub struct FloatText {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub text: String,
    pub color: Rgb,
    pub life: f32,
}

pub struct DynLight {
    pub x: f32,
    pub y: f32,
    pub color: [f32; 3],
    pub radius: f32,
    pub life: f32,
    pub max: f32,
}

/// How a door gets out of the way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DoorOpening {
    /// A plank door creaks open on its hinges.
    Swing,
    /// A gate of bars rises up into the lintel.
    Rise,
    /// Heavy stone, bone or root doors part down the middle.
    Split,
    /// Portals and crystal doors dissipate into the air.
    Dissolve,
}

/// A door partway through opening.
#[derive(Clone, Copy, Debug)]
pub struct DoorAnim {
    pub x: i32,
    pub y: i32,
    pub kind: DoorOpening,
    /// Seconds since it began to open.
    pub age: f32,
}

/// How long a door takes to open.
pub const DOOR_TIME: f32 = 0.6;

/// Something that plays across the whole view rather than in the world:
/// gas welling up from a trap, flames whooshing round you, and so on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScreenKind {
    /// Noxious green gas jetting up from the floor.
    Gas,
    /// A puff of pale spores breathed in.
    Spores,
    /// Fire roaring up and sweeping round the edges.
    Fire,
    /// Spikes stabbing up from below.
    Spikes,
    /// A fanged maw snapping shut from above and below.
    Maw,
    /// An alarm: pulses of light beating in from the edges.
    Alarm,
    /// Frost creeping in from the edges.
    Frost,
}

pub struct ScreenFx {
    pub kind: ScreenKind,
    pub age: f32,
    pub dur: f32,
    /// Varies the shapes from one effect to the next.
    pub seed: u32,
}

impl ScreenFx {
    /// How far along it is, 0..1.
    pub fn k(&self) -> f32 {
        (self.age / self.dur).clamp(0.0, 1.0)
    }
}

/// Strands of a web falling away from the view: every web strand whose
/// break point lies in `lo..hi` drops, `age` seconds after it snapped.
pub struct WebFall {
    pub age: f32,
    pub lo: f32,
    pub hi: f32,
}

/// How long torn strands take to fall out of the view.
pub const WEB_FALL: f32 = 1.6;

/// A monster playing out its death. Its corpse (if it leaves one) is hidden
/// until this finishes.
pub struct Dying {
    pub kind: usize,
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub rx: f32,
    pub ry: f32,
    pub age: f32,
    /// It leaves a body behind (bosses and uniques do not).
    pub corpse: bool,
    /// Last Rites will take the body when the death is done: Some(true) to
    /// shatter bones, Some(false) to burn flesh.
    pub rite: Option<bool>,
    pub ally: bool,
}

pub struct Fx {
    pub doors: Vec<DoorAnim>,
    pub dying: Vec<Dying>,
    pub screen: Vec<ScreenFx>,
    /// Seconds since a web splatted over the view (it spreads out at first).
    pub web_age: f32,
    /// A tug against the web: it shudders, then settles.
    pub web_jolt: f32,
    /// Strands torn loose and falling.
    pub web_falls: Vec<WebFall>,
    /// Varies the web's pattern from one webbing to the next.
    pub web_seed: u32,
    pub parts: Vec<Particle>,
    pub bolts: Vec<Bolt>,
    pub texts: Vec<FloatText>,
    pub lights: Vec<DynLight>,
    pub flash: Rgb,
    pub flash_amt: f32,
    pub shake: f32,
}

impl Fx {
    pub fn new() -> Fx {
        Fx {
            doors: Vec::new(),
            dying: Vec::new(),
            screen: Vec::new(),
            web_age: 0.0,
            web_jolt: 0.0,
            web_falls: Vec::new(),
            web_seed: 0,
            parts: Vec::new(),
            bolts: Vec::new(),
            texts: Vec::new(),
            lights: Vec::new(),
            flash: Rgb::BLACK,
            flash_amt: 0.0,
            shake: 0.0,
        }
    }

    pub fn clear(&mut self) {
        self.doors.clear();
        self.dying.clear();
        self.screen.clear();
        self.web_falls.clear();
        self.parts.clear();
        self.bolts.clear();
        self.texts.clear();
        self.lights.clear();
    }

    /// How far the door at (x,y) has opened (0..1), if it is still opening.
    pub fn door_at(&self, x: i32, y: i32) -> Option<(DoorOpening, f32)> {
        self.doors
            .iter()
            .find(|d| d.x == x && d.y == y)
            .map(|d| (d.kind, (d.age / DOOR_TIME).clamp(0.0, 1.0)))
    }

    pub fn update(&mut self, dt: f32) {
        for s in self.screen.iter_mut() {
            s.age += dt;
        }
        for d in self.dying.iter_mut() {
            d.age += dt;
        }
        self.screen.retain(|s| s.age < s.dur);
        self.web_age += dt;
        self.web_jolt = (self.web_jolt - dt * 2.5).max(0.0);
        for f in self.web_falls.iter_mut() {
            f.age += dt;
        }
        self.web_falls.retain(|f| f.age < WEB_FALL);
        for d in self.doors.iter_mut() {
            d.age += dt;
        }
        self.doors.retain(|d| d.age < DOOR_TIME);
        for p in self.parts.iter_mut() {
            p.vz -= p.grav * dt;
            let drag = (1.0 - p.drag * dt).max(0.0);
            p.vx *= drag;
            p.vy *= drag;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.z += p.vz * dt;
            if p.z < 0.0 {
                p.z = 0.0;
                p.vz = -p.vz * 0.3;
                p.vx *= 0.6;
                p.vy *= 0.6;
            }
            p.life -= dt;
        }
        self.parts.retain(|p| p.life > 0.0 && p.z < 1.2);

        let mut ends = Vec::new();
        let mut trail = Vec::new();
        for b in self.bolts.iter_mut() {
            b.t += dt;
            let (x, y) = b.pos_at(b.t);
            trail.push((x, y, b.z, b.color, b.light.is_some()));
            if b.t >= b.dur {
                ends.push((b.x1, b.y1, b.end));
            }
        }
        self.bolts.retain(|b| b.t < b.dur);
        let mut rng = rand::thread_rng();
        for (x, y, z, c, magic) in trail {
            if magic || rng.gen_bool(0.4) {
                self.parts.push(Particle {
                    x,
                    y,
                    z,
                    vx: rng.gen_range(-0.2..0.2),
                    vy: rng.gen_range(-0.2..0.2),
                    vz: rng.gen_range(-0.1..0.2),
                    life: 0.35,
                    max: 0.35,
                    ch: if magic { '*' } else { '.' },
                    color: c,
                    glow: true,
                    grav: 0.0,
                    drag: 1.0,
                });
            }
        }
        for (x, y, e) in ends {
            match e {
                BoltEnd::None => {}
                BoltEnd::Sparks(c) => self.sparks(x, y, 0.5, c),
                BoltEnd::Explosion(c) => self.explosion(x, y, c),
                BoltEnd::Ice => self.ice(x, y),
                BoltEnd::Wound(b) => self.wound(x, y, 0.5, 8, b),
                BoltEnd::Magic(c) => {
                    self.burst(x, y, 0.5, 16, c, &['*', '+', '.'], 2.0, 0.6, 0.0, true);
                    self.light(
                        x,
                        y,
                        [c.0 as f32 / 120.0, c.1 as f32 / 120.0, c.2 as f32 / 120.0],
                        2.0,
                        0.4,
                    );
                }
            }
        }

        for t in self.texts.iter_mut() {
            t.z += dt * 0.6;
            t.life -= dt;
        }
        self.texts.retain(|t| t.life > 0.0);
        for l in self.lights.iter_mut() {
            l.life -= dt;
        }
        self.lights.retain(|l| l.life > 0.0);
        self.flash_amt = (self.flash_amt - dt * 2.5).max(0.0);
        self.shake = (self.shake - dt * 2.5).max(0.0);
        if self.parts.len() > 1500 {
            let excess = self.parts.len() - 1500;
            self.parts.drain(0..excess);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn burst(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        n: usize,
        color: Rgb,
        chars: &[char],
        speed: f32,
        life: f32,
        grav: f32,
        glow: bool,
    ) {
        let mut rng = rand::thread_rng();
        for _ in 0..n {
            let a = rng.gen::<f32>() * TAU;
            let s = speed * (0.3 + rng.gen::<f32>() * 0.7);
            let l = life * (0.5 + rng.gen::<f32>() * 0.5);
            let ch = chars[rng.gen_range(0..chars.len())];
            self.parts.push(Particle {
                x,
                y,
                z,
                vx: a.cos() * s,
                vy: a.sin() * s,
                vz: (rng.gen::<f32>() - 0.3) * speed,
                life: l,
                max: l,
                ch,
                color,
                glow,
                grav,
                drag: 1.5,
            });
        }
    }

    pub fn blood(&mut self, x: f32, y: f32, z: f32, n: usize) {
        self.burst(
            x,
            y,
            z,
            n,
            Rgb(175, 12, 12),
            &[',', '.', '\'', '`', ';'],
            1.6,
            0.9,
            3.0,
            false,
        );
    }

    pub fn ichor(&mut self, x: f32, y: f32, z: f32, n: usize, c: Rgb) {
        self.burst(
            x,
            y,
            z,
            n,
            c,
            &[',', '.', '\'', ';', '~'],
            1.6,
            0.9,
            3.0,
            false,
        );
    }

/// A wound: what sprays out when a creature is struck.
    pub fn wound(&mut self, x: f32, y: f32, z: f32, n: usize, b: Bleed) {
        let drip = [',', '.', '\'', ';', '~'];
        match b {
            Bleed::Blood => self.blood(x, y, z, n),
            Bleed::Rot => self.burst(x, y, z, n, Rgb(85, 60, 30), &drip, 1.2, 0.9, 3.0, false),
            Bleed::Slime => {
                self.burst(x, y, z, n + 2, Rgb(90, 200, 50), &['o', ',', '.', '~', '\''], 1.4, 1.0, 3.5, false)
            }
            Bleed::Ichor => {
                self.burst(x, y, z, n, Rgb(180, 185, 70), &drip, 1.6, 0.9, 3.0, false);
                self.burst(x, y, z, n / 3, Rgb(70, 55, 40), &['\'', '`', '-'], 1.8, 0.6, 3.0, false);
            }
            Bleed::Spores => {
                self.burst(x, y, z, n, Rgb(200, 190, 140), &['.', '\'', '`', 'o'], 0.8, 1.4, -0.2, false);
                self.burst(x, y, z, n / 2, Rgb(150, 170, 90), &[',', '.'], 1.2, 0.8, 3.0, false);
            }
            Bleed::Bone => self.burst(
                x, y, z, n, Rgb(225, 220, 195), &['\'', ',', '.', '`', '-'], 1.8, 0.6, 3.5, false,
            ),
            Bleed::Stone => {
                self.burst(x, y, z, n, Rgb(135, 130, 125), &['.', ',', '\'', '`', ':'], 2.0, 0.6, 4.0, false);
                self.burst(x, y, z, n / 2, Rgb(90, 85, 80), &['.', '\''], 0.6, 0.9, 0.3, false);
            }
            Bleed::Crystal => {
                let mut rng = rand::thread_rng();
                for _ in 0..n {
                    let c = [Rgb(170, 240, 255), Rgb(220, 170, 255), Rgb(255, 255, 255), Rgb(150, 255, 200)]
                        [rng.gen_range(0..4)];
                    self.burst(x, y, z, 1, c, &['*', '\'', '/', '\\', '+'], 2.2, 0.55, 3.0, true);
                }
                self.light(x, y, [0.8, 1.1, 1.4], 1.4, 0.25);
            }
            Bleed::Spirit => {
                self.burst(x, y, z, n, Rgb(150, 185, 230), &['~', '.', '\'', 'o'], 0.9, 0.9, -0.4, true)
            }
            Bleed::Ember => {
                self.burst(x, y, z, n, Rgb(255, 150, 40), &['*', '.', '\'', '+'], 1.8, 0.6, -0.5, true);
                self.light(x, y, [1.4, 0.7, 0.2], 1.4, 0.25);
            }
            Bleed::Demon => self.ichor(x, y, z, n, Rgb(255, 120, 30)),
        }
    }

/// What is left in the air when a creature is slain.
    pub fn remains(&mut self, x: f32, y: f32, b: Bleed) {
        match b {
            Bleed::Blood => self.blood(x, y, 0.5, 22),
            Bleed::Rot => {
                self.wound(x, y, 0.5, 18, b);
                self.bones(x, y);
            }
            Bleed::Slime => {
                self.wound(x, y, 0.4, 36, b);
                self.rise(x, y, 10, Rgb(110, 210, 70), &['o', '.', '\''], 0.8);
            }
            Bleed::Ichor => self.wound(x, y, 0.4, 24, b),
            Bleed::Spores => {
                self.rise(x, y, 34, Rgb(200, 195, 150), &['.', '\'', '`', 'o', '*'], 1.6);
                self.wound(x, y, 0.5, 10, b);
            }
            Bleed::Bone => self.bones(x, y),
            Bleed::Stone => {
                self.wound(x, y, 0.6, 30, b);
                self.burst(x, y, 0.5, 8, Rgb(120, 115, 110), &['#', '%'], 1.4, 0.9, 4.0, false);
                self.rise(x, y, 14, Rgb(95, 90, 85), &['.', '\'', '~'], 1.4);
                self.shake = self.shake.max(0.35);
            }
            Bleed::Crystal => {
                self.wound(x, y, 0.6, 40, b);
                self.ring(x, y, 28, 3.0, Rgb(190, 240, 255), &['*', '+', '\''], 0.7);
                self.light(x, y, [1.4, 1.8, 2.2], 3.0, 0.6);
                self.flash(Rgb(190, 230, 255), 0.3);
            }
            Bleed::Spirit => {
                self.mist(x, y, Rgb(150, 185, 230));
                self.wound(x, y, 0.6, 12, b);
            }
            Bleed::Ember => {
                self.wound(x, y, 0.5, 30, b);
                self.rise(x, y, 20, Rgb(255, 120, 30), &['*', '.', '\''], 1.0);
                self.light(x, y, [1.8, 0.9, 0.3], 2.5, 0.6);
            }
            Bleed::Demon => {
                self.ichor(x, y, 0.5, 20, Rgb(255, 110, 20));
                self.burst(x, y, 0.4, 20, Rgb(80, 70, 70), &['o', '~', '.'], 0.6, 1.5, -0.3, false);
                self.light(x, y, [1.5, 0.6, 0.2], 2.0, 0.5);
            }
        }
    }

    pub fn sparks(&mut self, x: f32, y: f32, z: f32, c: Rgb) {
        self.burst(x, y, z, 10, c, &['*', '.', '\'', '+'], 2.5, 0.4, 2.0, true);
    }

    pub fn explosion(&mut self, x: f32, y: f32, c: Rgb) {
        self.burst(
            x,
            y,
            0.5,
            60,
            c,
            &['*', '#', '%', '@', '+'],
            3.0,
            0.7,
            0.5,
            true,
        );
        self.burst(
            x,
            y,
            0.5,
            30,
            Rgb(255, 235, 130),
            &['*', '.', '\''],
            4.0,
            0.4,
            0.0,
            true,
        );
        self.burst(
            x,
            y,
            0.4,
            25,
            Rgb(90, 80, 80),
            &['o', 'O', '.', '~'],
            0.8,
            1.6,
            -0.4,
            false,
        );
        self.light(x, y, [2.6, 1.3, 0.4], 3.0, 0.6);
        self.shake = self.shake.max(0.9);
    }

    pub fn ice(&mut self, x: f32, y: f32) {
        self.burst(
            x,
            y,
            0.5,
            30,
            Rgb(150, 220, 255),
            &['*', '+', 'x', '.'],
            2.5,
            0.8,
            0.5,
            true,
        );
        self.light(x, y, [0.4, 0.9, 1.8], 2.5, 0.5);
    }

    pub fn ring(
        &mut self,
        x: f32,
        y: f32,
        n: usize,
        speed: f32,
        c: Rgb,
        chars: &[char],
        life: f32,
    ) {
        let mut rng = rand::thread_rng();
        for i in 0..n {
            let a = i as f32 / n as f32 * TAU;
            let ch = chars[rng.gen_range(0..chars.len())];
            self.parts.push(Particle {
                x,
                y,
                z: 0.35 + rng.gen::<f32>() * 0.2,
                vx: a.cos() * speed,
                vy: a.sin() * speed,
                vz: 0.0,
                life,
                max: life,
                ch,
                color: c,
                glow: true,
                grav: 0.0,
                drag: 0.5,
            });
        }
    }

    pub fn rise(&mut self, x: f32, y: f32, n: usize, c: Rgb, chars: &[char], spread: f32) {
        let mut rng = rand::thread_rng();
        for _ in 0..n {
            let ch = chars[rng.gen_range(0..chars.len())];
            let l = 0.8 + rng.gen::<f32>() * 0.8;
            self.parts.push(Particle {
                x: x + (rng.gen::<f32>() - 0.5) * spread,
                y: y + (rng.gen::<f32>() - 0.5) * spread,
                z: rng.gen::<f32>() * 0.3,
                vx: (rng.gen::<f32>() - 0.5) * 0.2,
                vy: (rng.gen::<f32>() - 0.5) * 0.2,
                vz: 0.3 + rng.gen::<f32>() * 0.4,
                life: l,
                max: l,
                ch,
                color: c,
                glow: true,
                grav: 0.0,
                drag: 0.5,
            });
        }
    }

    pub fn heal(&mut self, x: f32, y: f32) {
        self.rise(x, y, 30, Rgb(120, 255, 140), &['+', '*', '.'], 1.2);
        self.flash(Rgb(60, 255, 120), 0.25);
    }

    pub fn levelup(&mut self, x: f32, y: f32) {
        self.rise(x, y, 50, Rgb(255, 220, 80), &['*', '+', '\'', '.'], 1.6);
        self.ring(x, y, 32, 3.0, Rgb(255, 230, 120), &['*', '+'], 0.8);
        self.flash(Rgb(255, 215, 60), 0.45);
    }

    pub fn mist(&mut self, x: f32, y: f32, c: Rgb) {
        self.rise(x, y, 26, c, &['~', 'o', '.', '\''], 1.4);
    }

    pub fn bones(&mut self, x: f32, y: f32) {
        self.burst(
            x,
            y,
            0.5,
            16,
            Rgb(230, 225, 200),
            &['%', '\'', '=', '-', 'o'],
            1.6,
            1.0,
            3.0,
            false,
        );
    }

    pub fn text(&mut self, x: f32, y: f32, z: f32, s: String, c: Rgb) {
        self.texts.push(FloatText {
            x,
            y,
            z,
            text: s,
            color: c,
            life: 1.0,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn bolt(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        speed: f32,
        ch: char,
        color: Rgb,
        light: Option<[f32; 3]>,
        end: BoltEnd,
    ) {
        let d = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        let dur = (d / speed).max(0.05);
        self.bolts.push(Bolt {
            x0,
            y0,
            x1,
            y1,
            t: 0.0,
            dur,
            ch,
            color,
            light,
            end,
            z: 0.5,
        });
    }

    pub fn light(&mut self, x: f32, y: f32, color: [f32; 3], radius: f32, life: f32) {
        self.lights.push(DynLight {
            x,
            y,
            color,
            radius,
            life,
            max: life,
        });
    }

    /// Start an effect across the whole view.
    pub fn screen(&mut self, kind: ScreenKind, dur: f32) {
        let seed = rand::thread_rng().gen();
        self.screen.retain(|s| s.kind != kind);
        self.screen.push(ScreenFx { kind, age: 0.0, dur, seed });
    }

    /// A fresh web lands over the view.
    pub fn web_splat(&mut self) {
        self.web_age = 0.0;
        self.web_jolt = 0.6;
        self.web_seed = rand::thread_rng().gen();
    }

    /// Strands with break points in `lo..hi` snap and fall away.
    pub fn web_tear(&mut self, lo: f32, hi: f32) {
        if hi > lo {
            self.web_falls.push(WebFall { age: 0.0, lo, hi });
            self.web_jolt = 1.0;
        }
    }

    pub fn flash(&mut self, c: Rgb, amt: f32) {
        self.flash = c;
        self.flash_amt = self.flash_amt.max(amt);
    }
}

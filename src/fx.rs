//! Purely visual effects living in world space: particles, projectiles,
//! floating combat text, dynamic lights, screen flash and shake.

use crate::color::Rgb;
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
    Blood,
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
        (self.x0 + (self.x1 - self.x0) * k, self.y0 + (self.y1 - self.y0) * k)
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

pub struct Fx {
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
        self.parts.clear();
        self.bolts.clear();
        self.texts.clear();
        self.lights.clear();
    }

    pub fn update(&mut self, dt: f32) {
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
                BoltEnd::Blood => self.blood(x, y, 0.5, 8),
                BoltEnd::Magic(c) => {
                    self.burst(x, y, 0.5, 16, c, &['*', '+', '.'], 2.0, 0.6, 0.0, true);
                    self.light(x, y, [c.0 as f32 / 120.0, c.1 as f32 / 120.0, c.2 as f32 / 120.0], 2.0, 0.4);
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
        self.burst(x, y, z, n, Rgb(175, 12, 12), &[',', '.', '\'', '`', ';'], 1.6, 0.9, 3.0, false);
    }

    pub fn ichor(&mut self, x: f32, y: f32, z: f32, n: usize, c: Rgb) {
        self.burst(x, y, z, n, c, &[',', '.', '\'', ';', '~'], 1.6, 0.9, 3.0, false);
    }

    pub fn sparks(&mut self, x: f32, y: f32, z: f32, c: Rgb) {
        self.burst(x, y, z, 10, c, &['*', '.', '\'', '+'], 2.5, 0.4, 2.0, true);
    }

    pub fn explosion(&mut self, x: f32, y: f32, c: Rgb) {
        self.burst(x, y, 0.5, 60, c, &['*', '#', '%', '@', '+'], 3.0, 0.7, 0.5, true);
        self.burst(x, y, 0.5, 30, Rgb(255, 235, 130), &['*', '.', '\''], 4.0, 0.4, 0.0, true);
        self.burst(x, y, 0.4, 25, Rgb(90, 80, 80), &['o', 'O', '.', '~'], 0.8, 1.6, -0.4, false);
        self.light(x, y, [2.6, 1.3, 0.4], 3.0, 0.6);
        self.shake = self.shake.max(0.9);
    }

    pub fn ice(&mut self, x: f32, y: f32) {
        self.burst(x, y, 0.5, 30, Rgb(150, 220, 255), &['*', '+', 'x', '.'], 2.5, 0.8, 0.5, true);
        self.light(x, y, [0.4, 0.9, 1.8], 2.5, 0.5);
    }

    pub fn ring(&mut self, x: f32, y: f32, n: usize, speed: f32, c: Rgb, chars: &[char], life: f32) {
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
        self.burst(x, y, 0.5, 16, Rgb(230, 225, 200), &['%', '\'', '=', '-', 'o'], 1.6, 1.0, 3.0, false);
    }

    pub fn text(&mut self, x: f32, y: f32, z: f32, s: String, c: Rgb) {
        self.texts.push(FloatText { x, y, z, text: s, color: c, life: 1.0 });
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
        self.bolts.push(Bolt { x0, y0, x1, y1, t: 0.0, dur, ch, color, light, end, z: 0.5 });
    }

    pub fn light(&mut self, x: f32, y: f32, color: [f32; 3], radius: f32, life: f32) {
        self.lights.push(DynLight { x, y, color, radius, life, max: life });
    }

    pub fn flash(&mut self, c: Rgb, amt: f32) {
        self.flash = c;
        self.flash_amt = self.flash_amt.max(amt);
    }
}

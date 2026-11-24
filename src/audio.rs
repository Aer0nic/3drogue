//! Audio engine. Every sound is synthesised procedurally (no sample files):
//! monster voices, combat, ambience and a generative music score. Positional
//! sounds are filtered, panned, echoed and reverberated according to how the
//! sound travels through the dungeon (see `Game::spatial`).

use crate::data::{Material, VoiceKind};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rand::Rng;
use std::f32::consts::{PI, TAU};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    Alert,
    Attack,
    Hurt,
    Die,
    Idle,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sfx {
    Step,
    Swing,
    Hit,
    Clang,
    Miss,
    PlayerHurt,
    PlayerDie,
    Voice(VoiceKind, Mood),
    Drip,
    Rumble,
    Chains,
    Whisper,
    Creak,
    Wind,
    Bones,
    Fireball,
    Explosion,
    Frost,
    /// A bowstring released and an arrow whistling away.
    Arrow,
    /// A blow or an arrow landing on something made of this.
    Impact(Material),
    /// A handful of grit flung through the air: a quick, sandy swish.
    GritThrow,
    /// Grit spattering across your face.
    GritHit,
    Bolt,
    Heal,
    LevelUp,
    Pickup,
    Stairs,
    Revive,
    Trap,
    Amulet,
    Blink,
    Stomp,
    Menu,
}

/// How a sound reaches the listener.
#[derive(Clone, Debug)]
pub struct Spatial {
    pub gain: f32,
    /// -1 = hard left, +1 = hard right.
    pub pan: f32,
    /// Low-pass cutoff in Hz (occlusion / distance muffling).
    pub cutoff: f32,
    /// (delay seconds, gain, pan) discrete echoes.
    pub echoes: Vec<(f32, f32, f32)>,
    /// Reverb wet amount.
    pub reverb: f32,
    /// Room size 0..1.
    pub room: f32,
}

impl Spatial {
    pub fn center(gain: f32) -> Spatial {
        Spatial { gain, pan: 0.0, cutoff: 16000.0, echoes: vec![], reverb: 0.12, room: 0.4 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MusicMode {
    Title,
    Dungeon,
    Death,
    Victory,
}

struct PlayVoice {
    data: Arc<Vec<f32>>,
    pos: usize,
}

pub struct Mixer {
    voices: Vec<PlayVoice>,
    music: Music,
    master: f32,
}

impl Mixer {
    fn new(sr: f32) -> Mixer {
        Mixer { voices: Vec::new(), music: Music::new(sr), master: 0.8 }
    }

    fn fill(&mut self, out: &mut [f32]) {
        for s in out.iter_mut() {
            *s = 0.0;
        }
        let frames = out.len() / 2;
        self.music.render(out, frames);
        for v in self.voices.iter_mut() {
            let n = v.data.len();
            let mut i = 0;
            while i < frames && v.pos + 1 < n {
                out[i * 2] += v.data[v.pos];
                out[i * 2 + 1] += v.data[v.pos + 1];
                v.pos += 2;
                i += 1;
            }
        }
        self.voices.retain(|v| v.pos + 1 < v.data.len());
        for s in out.iter_mut() {
            *s = (*s * self.master).tanh();
        }
    }
}

pub struct Audio {
    pub sr: f32,
    mixer: Option<Arc<Mutex<Mixer>>>,
    _stream: Option<cpal::Stream>,
}

impl Audio {
    pub fn new(enabled: bool) -> Audio {
        if enabled {
            if let Ok(a) = Self::try_init() {
                return a;
            }
        }
        Audio { sr: 44100.0, mixer: None, _stream: None }
    }

    pub fn enabled(&self) -> bool {
        self.mixer.is_some()
    }

    fn try_init() -> Result<Audio, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no output device")?;
        let supported = device.default_output_config()?;
        let fmt = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let sr = config.sample_rate.0 as f32;
        let ch = config.channels as usize;
        let mixer = Arc::new(Mutex::new(Mixer::new(sr)));
        let stream = match fmt {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, ch, mixer.clone())?,
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, ch, mixer.clone())?,
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, ch, mixer.clone())?,
            cpal::SampleFormat::I32 => build::<i32>(&device, &config, ch, mixer.clone())?,
            _ => return Err("unsupported sample format".into()),
        };
        stream.play()?;
        Ok(Audio { sr, mixer: Some(mixer), _stream: Some(stream) })
    }

    pub fn play(&self, sfx: Sfx, sp: &Spatial) {
        self.play_after(sfx, sp, 0.0);
    }

    /// Play a sound `delay` seconds from now (an arrow still in flight).
    pub fn play_after(&self, sfx: Sfx, sp: &Spatial, delay: f32) {
        let Some(m) = &self.mixer else { return };
        let mut rng = rand::thread_rng();
        let mono = synth(sfx, self.sr, &mut rng);
        let mut st = spatialize(&mono, sp, self.sr);
        if delay > 0.0 {
            let pad = (delay.min(2.0) * self.sr) as usize * 2;
            st.splice(0..0, std::iter::repeat(0.0).take(pad));
        }
        if let Ok(mut mx) = m.lock() {
            if mx.voices.len() > 40 {
                mx.voices.remove(0);
            }
            mx.voices.push(PlayVoice { data: Arc::new(st), pos: 0 });
        }
    }

    pub fn set_music(&self, mode: MusicMode, depth: u32) {
        if let Some(m) = &self.mixer {
            if let Ok(mut mx) = m.lock() {
                mx.music.mode = mode;
                mx.music.depth = depth;
                if mode != MusicMode::Dungeon {
                    mx.music.amulet = false;
                }
            }
        }
    }

    /// The Amulet is in your hands: the score turns frantic, and grows more
    /// hopeful with every floor you climb.
    pub fn set_amulet(&self, on: bool) {
        if let Some(m) = &self.mixer {
            if let Ok(mut mx) = m.lock() {
                mx.music.amulet = on;
            }
        }
    }

    pub fn set_intensity(&self, i: f32) {
        if let Some(m) = &self.mixer {
            if let Ok(mut mx) = m.lock() {
                mx.music.target = i.clamp(0.0, 1.0);
            }
        }
    }

    /// Adjust master volume; returns the new value.
    pub fn adjust_volume(&self, d: f32) -> f32 {
        if let Some(m) = &self.mixer {
            if let Ok(mut mx) = m.lock() {
                mx.master = (mx.master + d).clamp(0.0, 1.5);
                return mx.master;
            }
        }
        0.0
    }

    pub fn set_volume(&self, v: f32) {
        if let Some(m) = &self.mixer {
            if let Ok(mut mx) = m.lock() {
                mx.master = v.clamp(0.0, 1.5);
            }
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    ch: usize,
    mixer: Arc<Mutex<Mixer>>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let mut buf: Vec<f32> = Vec::new();
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let frames = data.len() / ch.max(1);
            if buf.len() < frames * 2 {
                buf.resize(frames * 2, 0.0);
            }
            match mixer.lock() {
                Ok(mut m) => m.fill(&mut buf[..frames * 2]),
                Err(_) => {
                    for s in buf.iter_mut() {
                        *s = 0.0;
                    }
                }
            }
            for f in 0..frames {
                let l = buf[f * 2];
                let r = buf[f * 2 + 1];
                for c in 0..ch {
                    let v = if ch == 1 {
                        (l + r) * 0.5
                    } else if c == 0 {
                        l
                    } else if c == 1 {
                        r
                    } else {
                        0.0
                    };
                    data[f * ch + c] = <T as cpal::Sample>::from_sample::<f32>(v);
                }
            }
        },
        |_err| {},
        None,
    )
}

// ================================================================ DSP helpers

struct Svf {
    low: f32,
    band: f32,
}

impl Svf {
    fn new() -> Svf {
        Svf { low: 0.0, band: 0.0 }
    }
    /// Chamberlin state-variable filter. `q` is damping (small = resonant).
    fn process(&mut self, input: f32, fc: f32, q: f32, sr: f32) -> (f32, f32, f32) {
        let fc = fc.clamp(20.0, sr / 6.5);
        let f = 2.0 * (PI * fc / sr).sin();
        self.low += f * self.band;
        let high = input - self.low - q * self.band;
        self.band += f * high;
        (self.low, self.band, high)
    }
}

fn nz<R: Rng>(rng: &mut R) -> f32 {
    rng.gen::<f32>() * 2.0 - 1.0
}

fn rr<R: Rng>(rng: &mut R, a: f32, b: f32) -> f32 {
    a + (b - a) * rng.gen::<f32>()
}

fn normalize(v: &mut [f32], peak: f32) {
    let m = v.iter().fold(0.0f32, |a, &x| a.max(x.abs()));
    if m > 1e-6 {
        let k = peak / m;
        for x in v.iter_mut() {
            *x *= k;
        }
    }
}

fn mix_into(dst: &mut Vec<f32>, src: &[f32], offset: usize, gain: f32) {
    if dst.len() < offset + src.len() {
        dst.resize(offset + src.len(), 0.0);
    }
    for (i, s) in src.iter().enumerate() {
        dst[offset + i] += s * gain;
    }
}

/// Noise through a sweeping filter. mode 0 = low, 1 = band, 2 = high.
#[allow(clippy::too_many_arguments)]
fn filtered_noise<R: Rng>(sr: f32, rng: &mut R, dur: f32, fc0: f32, fc1: f32, q: f32, attack: f32, mode: u8) -> Vec<f32> {
    let n = (dur * sr) as usize;
    let mut f = Svf::new();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sr;
        let x = t / dur;
        let fc = fc0 + (fc1 - fc0) * x;
        let env = (t / attack.max(1e-4)).min(1.0) * (1.0 - x).powf(2.0);
        let (l, b, h) = f.process(nz(rng), fc, q, sr);
        let v = match mode {
            0 => l,
            1 => b,
            _ => h,
        };
        out.push(v * env);
    }
    out
}

/// Sine sweep f0 -> f1 with exponential decay (time constant `decay` s).
fn tone(sr: f32, dur: f32, f0: f32, f1: f32, decay: f32) -> Vec<f32> {
    let n = (dur * sr) as usize;
    let mut out = Vec::with_capacity(n);
    let mut ph = 0.0f32;
    for i in 0..n {
        let t = i as f32 / sr;
        let x = t / dur;
        let f = f0 + (f1 - f0) * x;
        ph += f / sr;
        let env = (t / 0.003).min(1.0) * (-t / decay).exp();
        out.push((ph * TAU).sin() * env);
    }
    out
}

/// Inharmonic struck-metal / bell tone.
fn bell(sr: f32, f: f32, dur: f32, partials: &[(f32, f32)], decay: f32) -> Vec<f32> {
    let n = (dur * sr) as usize;
    let mut out = vec![0.0; n];
    for &(ratio, amp) in partials {
        let fr = f * ratio;
        let dk = decay / ratio.sqrt();
        for (i, o) in out.iter_mut().enumerate() {
            let t = i as f32 / sr;
            *o += (TAU * fr * t).sin() * amp * (-t / dk).exp();
        }
    }
    out
}

struct VP {
    pitch: f32,
    dur: f32,
    glide: f32,
    vib: f32,
    vib_rate: f32,
    noise: f32,
    formant: f32,
    formant2: f32,
    rough: f32,
    q: f32,
}

/// Generic vocal synth: glottal saw + noise through two formant filters.
fn vocal<R: Rng>(p: &VP, sr: f32, rng: &mut R) -> Vec<f32> {
    let n = (p.dur.max(0.02) * sr) as usize;
    let mut out = vec![0.0; n];
    let mut ph = 0.0f32;
    let mut f1 = Svf::new();
    let mut f2 = Svf::new();
    let mut jitter = 0.0f32;
    for (i, o) in out.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let x = t / p.dur;
        let env = (t / 0.025).min(1.0) * (1.0 - x).max(0.0).powf(0.8);
        if i % 64 == 0 {
            jitter = (rng.gen::<f32>() - 0.5) * p.rough * 0.15;
        }
        let f = p.pitch * (1.0 + p.glide * x) * (1.0 + p.vib * (TAU * p.vib_rate * t).sin()) * (1.0 + jitter);
        ph += f / sr;
        ph -= ph.floor();
        let saw = 2.0 * ph - 1.0;
        let src = saw * (1.0 - p.noise) + nz(rng) * p.noise;
        let am = 1.0 - p.rough * 0.25 * (1.0 + (TAU * 33.0 * t).sin());
        let (_, b1, _) = f1.process(src, p.formant * (1.0 + 0.15 * x), p.q, sr);
        let (l2, b2, _) = f2.process(src, p.formant2, p.q, sr);
        *o = (b1 + b2 * 0.6 + l2 * 0.3) * env * am;
    }
    normalize(&mut out, 0.9);
    out
}

fn distort(v: &mut [f32], k: f32) {
    for x in v.iter_mut() {
        *x = (*x * k).tanh();
    }
}

fn repeat_calls<R: Rng, F: FnMut(&mut R) -> Vec<f32>>(rng: &mut R, sr: f32, count: usize, gap: (f32, f32), mut f: F) -> Vec<f32> {
    let mut out = Vec::new();
    for _ in 0..count {
        let v = f(rng);
        out.extend_from_slice(&v);
        let g = (rr(rng, gap.0, gap.1) * sr) as usize;
        out.extend(std::iter::repeat(0.0).take(g));
    }
    out
}

fn clicks<R: Rng>(sr: f32, rng: &mut R, count: usize, total: f32, f0: f32, f1: f32) -> Vec<f32> {
    let mut out = vec![0.0; ((total + 0.05) * sr) as usize];
    for _ in 0..count {
        let pos = (rr(rng, 0.0, total) * sr) as usize;
        let fc = rr(rng, f0, f1);
        let mut f = Svf::new();
        let len = (0.008 * sr) as usize;
        let amp = rr(rng, 0.4, 1.0);
        for i in 0..len {
            let env = (-(i as f32) / (0.0015 * sr)).exp();
            let (_, b, _) = f.process(nz(rng), fc, 0.3, sr);
            if pos + i < out.len() {
                out[pos + i] += b * env * amp;
            }
        }
    }
    normalize(&mut out, 0.9);
    out
}

// ================================================================ voices

fn monster_voice<R: Rng>(kind: VoiceKind, mood: Mood, sr: f32, rng: &mut R) -> Vec<f32> {
    let (pm, dm) = match mood {
        Mood::Alert => (1.0, 1.0),
        Mood::Attack => (1.1, 0.7),
        Mood::Hurt => (1.35, 0.5),
        Mood::Die => (0.85, 1.6),
        Mood::Idle => (0.95, 0.8),
    };
    let die = mood == Mood::Die;
    let mut out = match kind {
        VoiceKind::Squeak => {
            let count = if die { 1 } else { rng.gen_range(1..4) };
            repeat_calls(rng, sr, count, (0.03, 0.08), |rng| {
                let p = VP { pitch: rr(rng, 1800.0, 2600.0) * pm, dur: rr(rng, 0.07, 0.14) * dm, glide: if die { -0.5 } else { 0.3 }, vib: 0.02, vib_rate: 30.0, noise: 0.15, formant: 3000.0, formant2: 4500.0, rough: 0.2, q: 0.5 };
                vocal(&p, sr, rng)
            })
        }
        VoiceKind::Screech => {
            let count = if die { 2 } else { rng.gen_range(3..7) };
            repeat_calls(rng, sr, count, (0.02, 0.05), |rng| {
                let p = VP { pitch: rr(rng, 3000.0, 4200.0) * pm, dur: 0.05 * dm, glide: -0.3, vib: 0.0, vib_rate: 1.0, noise: 0.1, formant: 4000.0, formant2: 5500.0, rough: 0.1, q: 0.5 };
                vocal(&p, sr, rng)
            })
        }
        VoiceKind::Yap => {
            let count = if die { 1 } else { rng.gen_range(1..3) };
            repeat_calls(rng, sr, count, (0.05, 0.12), |rng| {
                let p = VP { pitch: rr(rng, 280.0, 420.0) * pm, dur: rr(rng, 0.18, 0.3) * dm, glide: rr(rng, -0.3, 0.4), vib: 0.05, vib_rate: 9.0, noise: 0.25, formant: 1100.0, formant2: 2400.0, rough: 0.4, q: 0.4 };
                vocal(&p, sr, rng)
            })
        }
        VoiceKind::Rattle => {
            let n = if die { 22 } else { rng.gen_range(6..14) };
            clicks(sr, rng, n, 0.6 * dm, 1500.0, 4000.0)
        }
        VoiceKind::Moan => {
            let p = VP { pitch: rr(rng, 85.0, 130.0) * pm, dur: rr(rng, 0.9, 1.5) * dm, glide: -0.25, vib: 0.03, vib_rate: 4.0, noise: 0.35, formant: 450.0, formant2: 900.0, rough: 0.3, q: 0.3 };
            vocal(&p, sr, rng)
        }
        VoiceKind::Hiss => {
            let p = VP { pitch: 200.0, dur: rr(rng, 0.5, 0.8) * dm, glide: 0.0, vib: 0.0, vib_rate: 1.0, noise: 1.0, formant: 4500.0 * pm, formant2: 6000.0, rough: 0.0, q: 0.6 };
            vocal(&p, sr, rng)
        }
        VoiceKind::Growl => {
            let p = VP { pitch: rr(rng, 65.0, 100.0) * pm, dur: rr(rng, 0.45, 0.75) * dm, glide: -0.1, vib: 0.04, vib_rate: 7.0, noise: 0.35, formant: 550.0, formant2: 1100.0, rough: 0.9, q: 0.35 };
            vocal(&p, sr, rng)
        }
        VoiceKind::Snarl => {
            if mood == Mood::Alert || die {
                let p = VP { pitch: rr(rng, 380.0, 460.0) * pm, dur: 1.2 * dm, glide: if die { -0.5 } else { 0.4 }, vib: 0.02, vib_rate: 5.0, noise: 0.05, formant: 900.0, formant2: 1800.0, rough: 0.05, q: 0.4 };
                vocal(&p, sr, rng)
            } else {
                let p = VP { pitch: rr(rng, 130.0, 190.0) * pm, dur: 0.35 * dm, glide: 0.0, vib: 0.05, vib_rate: 8.0, noise: 0.4, formant: 800.0, formant2: 1600.0, rough: 0.8, q: 0.35 };
                vocal(&p, sr, rng)
            }
        }
        VoiceKind::Roar => {
            let p = VP { pitch: rr(rng, 45.0, 75.0) * pm, dur: rr(rng, 0.8, 1.3) * dm, glide: -0.2, vib: 0.05, vib_rate: 5.0, noise: 0.5, formant: 380.0, formant2: 800.0, rough: 1.0, q: 0.3 };
            let mut v = vocal(&p, sr, rng);
            distort(&mut v, 3.0);
            v
        }
        VoiceKind::Wail => {
            let base = rr(rng, 480.0, 650.0) * pm;
            let p1 = VP { pitch: base, dur: 1.3 * dm, glide: -0.4, vib: 0.05, vib_rate: 5.0, noise: 0.3, formant: 1400.0, formant2: 2600.0, rough: 0.1, q: 0.3 };
            let p2 = VP { pitch: base * 1.07, ..p1 };
            let mut a = vocal(&p1, sr, rng);
            let b = vocal(&p2, sr, rng);
            mix_into(&mut a, &b, 0, 0.7);
            a
        }
        VoiceKind::Chant => {
            let p = VP { pitch: rr(rng, 105.0, 120.0) * pm, dur: 1.3 * dm, glide: 0.0, vib: 0.01, vib_rate: 3.0, noise: 0.2, formant: 600.0, formant2: 1200.0, rough: 0.1, q: 0.25 };
            let mut v = vocal(&p, sr, rng);
            for (i, x) in v.iter_mut().enumerate() {
                let t = i as f32 / sr;
                *x *= 0.6 + 0.4 * (TAU * 7.0 * t).sin();
            }
            v
        }
        VoiceKind::Cackle => {
            let count = if die { 2 } else { rng.gen_range(4..8) };
            repeat_calls(rng, sr, count, (0.02, 0.05), |rng| {
                let p = VP { pitch: rr(rng, 650.0, 850.0) * pm, dur: 0.07 * dm, glide: -0.2, vib: 0.0, vib_rate: 1.0, noise: 0.2, formant: 1600.0, formant2: 3000.0, rough: 0.3, q: 0.4 };
                vocal(&p, sr, rng)
            })
        }
        VoiceKind::Demon => {
            let base = rr(rng, 35.0, 50.0) * pm;
            let p1 = VP { pitch: base, dur: 1.6 * dm, glide: -0.15, vib: 0.04, vib_rate: 4.0, noise: 0.5, formant: 350.0, formant2: 700.0, rough: 1.0, q: 0.3 };
            let p2 = VP { pitch: base * 3.02, formant: 1200.0, formant2: 2200.0, noise: 0.3, ..p1 };
            let mut a = vocal(&p1, sr, rng);
            let b = vocal(&p2, sr, rng);
            mix_into(&mut a, &b, 0, 0.4);
            distort(&mut a, 3.5);
            a
        }
        VoiceKind::Human => {
            let p = VP { pitch: rr(rng, 120.0, 170.0) * pm, dur: rr(rng, 0.3, 0.5) * dm, glide: if die { -0.5 } else { 0.1 }, vib: 0.03, vib_rate: 6.0, noise: 0.3, formant: 700.0, formant2: 1500.0, rough: 0.3, q: 0.35 };
            vocal(&p, sr, rng)
        }
        VoiceKind::Gurgle => {
            // Bubbles: each a sine that swoops upward as it pops, over a
            // low slosh of jelly. Dying, it sags into one long falling gloop.
            let count = if die { 8 } else { rng.gen_range(3..7) };
            let mut a = repeat_calls(rng, sr, count, (0.01, 0.09), |rng| {
                let f0 = rr(rng, 110.0, 230.0) * pm;
                tone(sr, rr(rng, 0.05, 0.12) * dm.min(1.2), f0, f0 * rr(rng, 1.8, 3.2), 0.04)
            });
            if die {
                let g = tone(sr, 0.7, 260.0, 55.0, 0.3);
                let at = a.len();
                mix_into(&mut a, &g, at, 0.9);
            }
            let dur = a.len() as f32 / sr;
            let slosh = filtered_noise(sr, rng, dur, 520.0 * pm, 160.0, 0.9, 0.05, 0);
            mix_into(&mut a, &slosh, 0, 1.6);
            a
        }
        VoiceKind::Chitter => {
            // Bursts of hard, high clicks with a dry rasp under them.
            let bursts = if die { 4 } else { rng.gen_range(2..4) };
            let mut a = repeat_calls(rng, sr, bursts, (0.03, 0.08), |rng| {
                let n = rng.gen_range(7..13);
                let len = rr(rng, 0.08, 0.14) * dm.min(1.3);
                clicks(sr, rng, n, len, 2800.0 * pm, 6500.0)
            });
            let dur = a.len() as f32 / sr;
            let rasp = filtered_noise(sr, rng, dur, 3800.0 * pm, 3000.0, 0.5, 0.01, 1);
            mix_into(&mut a, &rasp, 0, 0.35);
            if die {
                // Legs twitching to a stop.
                let n = 10;
                let t = clicks(sr, rng, n, 0.5, 1500.0, 3500.0);
                let at = a.len();
                mix_into(&mut a, &t, at, 0.5);
            }
            a
        }
        VoiceKind::Grind => {
            // Stick-slip scraping: band noise jerked on and off, a low groan
            // of shifting mass, and grit falling out of the joints.
            let dur = rr(rng, 0.7, 1.0) * dm;
            let mut a = filtered_noise(sr, rng, dur, 380.0 * pm, 170.0 * pm, 0.6, 0.06, 1);
            let mut slip = 1.0f32;
            for (i, x) in a.iter_mut().enumerate() {
                if i % (sr as usize / 40) == 0 {
                    slip = rr(rng, 0.15, 1.0);
                }
                *x *= slip;
            }
            let b = tone(sr, dur, 62.0 * pm, 44.0 * pm, dur * 0.6);
            mix_into(&mut a, &b, 0, 0.4);
            let grit = clicks(sr, rng, if die { 60 } else { 18 }, dur * if die { 1.4 } else { 1.0 }, 700.0, 2600.0);
            mix_into(&mut a, &grit, 0, if die { 0.8 } else { 0.35 });
            distort(&mut a, 1.8);
            a
        }
        VoiceKind::Crackle => {
            // A whoosh of flame flaring up (or guttering out), full of pops.
            let dur = rr(rng, 0.6, 0.9) * dm;
            let (f0, f1) = if die { (1400.0, 180.0) } else { (300.0 * pm, 1300.0 * pm) };
            let mut a = filtered_noise(sr, rng, dur, f0, f1, 0.8, if die { 0.01 } else { dur * 0.4 }, 1);
            let roar = filtered_noise(sr, rng, dur, 140.0, 90.0, 0.7, 0.05, 0);
            mix_into(&mut a, &roar, 0, 0.8);
            let n = rng.gen_range(8..16);
            let pops = clicks(sr, rng, n, dur, 1800.0, 5500.0);
            mix_into(&mut a, &pops, 0, 0.5);
            a
        }
        VoiceKind::Chime => {
            // Glassy struck tones: a rising arpeggio when roused, a sour
            // cluster when struck, a shatter of falling shards when broken.
            let glass: &[(f32, f32)] = &[(1.0, 1.0), (2.76, 0.5), (5.4, 0.3), (8.93, 0.15)];
            let base = rr(rng, 620.0, 760.0) * pm;
            let notes: Vec<f32> = match mood {
                Mood::Hurt => vec![1.0, 1.06, 1.12],
                Mood::Die => vec![2.0, 1.68, 1.5, 1.26, 1.0, 0.84],
                Mood::Attack => vec![1.0, 1.5],
                _ => vec![1.0, 1.26, 1.5],
            };
            let step = if mood == Mood::Hurt { 0.0 } else { 0.09 };
            let mut a = Vec::new();
            for (i, r) in notes.iter().enumerate() {
                let b = bell(sr, base * r, 0.9 * dm.min(1.2), glass, 0.35);
                mix_into(&mut a, &b, (i as f32 * step * sr) as usize, 1.0 - i as f32 * 0.08);
            }
            if die {
                let shards = clicks(sr, rng, 50, 0.6, 3500.0, 6500.0);
                mix_into(&mut a, &shards, 0, 0.7);
            }
            a
        }
        VoiceKind::Spore => {
            // Soft, dull puffs of spores and the creak of fibrous flesh.
            let count = if die { 5 } else { rng.gen_range(2..4) };
            let mut a = repeat_calls(rng, sr, count, (0.05, 0.15), |rng| {
                let len = rr(rng, 0.1, 0.18);
                filtered_noise(sr, rng, len, 900.0 * pm, 250.0, 0.9, 0.01, 0)
            });
            let p = VP { pitch: rr(rng, 38.0, 55.0) * pm, dur: rr(rng, 0.4, 0.6) * dm, glide: if die { -0.4 } else { 0.1 }, vib: 0.0, vib_rate: 1.0, noise: 0.15, formant: 320.0, formant2: 700.0, rough: 1.0, q: 0.4 };
            let creak = vocal(&p, sr, rng);
            mix_into(&mut a, &creak, 0, 0.35);
            a
        }
        VoiceKind::Mimic => {
            // Old hinges and swollen boards creaking under a slow, hungry
            // weight. Roused or striking, the lid slams down on teeth.
            let count = match mood {
                Mood::Idle => rng.gen_range(2..4),
                Mood::Die => 4,
                _ => 1,
            };
            let mut a = repeat_calls(rng, sr, count, (0.08, 0.25), |rng| {
                let p = VP { pitch: rr(rng, 55.0, 90.0) * pm, dur: rr(rng, 0.35, 0.7) * dm, glide: rr(rng, -0.3, 0.4), vib: 0.25, vib_rate: rr(rng, 2.0, 5.0), noise: 0.08, formant: 850.0, formant2: 1900.0, rough: 1.0, q: 0.3 };
                vocal(&p, sr, rng)
            });
            if mood != Mood::Idle {
                // The slam: a hollow wooden clap and a wet crunch.
                let clap = bell(sr, rr(rng, 140.0, 190.0), 0.25, &[(1.0, 1.0), (2.3, 0.5), (4.1, 0.2)], 0.05);
                let at = if mood == Mood::Alert { (0.25 * sr) as usize } else { 0 };
                mix_into(&mut a, &clap, at, 1.0);
                let thump = tone(sr, 0.25, 110.0, 45.0, 0.06);
                mix_into(&mut a, &thump, at, 0.9);
                let wet = filtered_noise(sr, rng, 0.2, 1400.0, 500.0, 0.7, 0.003, 1);
                mix_into(&mut a, &wet, at + (0.02 * sr) as usize, 0.5);
            }
            if die {
                // Boards splitting.
                let crack = clicks(sr, rng, 30, 0.6, 600.0, 2400.0);
                mix_into(&mut a, &crack, 0, 0.7);
            }
            a
        }
    };
    normalize(&mut out, if mood == Mood::Idle { 0.5 } else { 0.85 });
    out
}

// ================================================================ impacts

/// The sound of a blow or an arrow landing on `m`.
fn impact<R: Rng>(m: Material, sr: f32, rng: &mut R) -> Vec<f32> {
    let mut a = match m {
        Material::Flesh | Material::Ooze => {
            let ooze = m == Material::Ooze;
            // The smack: a soft, low body thump.
            let mut a = tone(sr, 0.16, rr(rng, 95.0, 130.0), 55.0, if ooze { 0.07 } else { 0.04 });
            // The squelch: muffled noise whose filter quivers open and shut.
            let dur = if ooze { 0.38 } else { 0.24 };
            let n = (dur * sr) as usize;
            let mut f = Svf::new();
            let mut wob = 1.0f32;
            for i in 0..n {
                if i % (sr as usize / 90) == 0 {
                    wob = rr(rng, 0.4, 1.6);
                }
                let x = i as f32 / n as f32;
                let fc = (900.0 - 600.0 * x) * wob;
                let (_, b, _) = f.process(nz(rng), fc, 0.35, sr);
                let env = (i as f32 / (0.004 * sr)).min(1.0) * (1.0 - x).powf(1.6);
                let v = b * env * if ooze { 1.4 } else { 1.0 };
                if i < a.len() {
                    a[i] += v;
                } else {
                    a.push(v);
                }
            }
            // A wet suck as the blade or arrow pulls free.
            let suck = tone(sr, 0.09, 220.0, if ooze { 520.0 } else { 380.0 }, 0.03);
            mix_into(&mut a, &suck, (0.06 * sr) as usize, if ooze { 0.5 } else { 0.25 });
            if ooze {
                for k in 0..3 {
                    let f0 = rr(rng, 140.0, 260.0);
                    let b = tone(sr, 0.08, f0, f0 * 2.4, 0.03);
                    mix_into(&mut a, &b, ((0.1 + 0.07 * k as f32) * sr) as usize, 0.35);
                }
            }
            a
        }
        Material::Bone => {
            // A dull, hollow knock and a dry crack, then a rattle of loose bones.
            let mut a = tone(sr, 0.14, rr(rng, 170.0, 210.0), 110.0, 0.035);
            let crack = filtered_noise(sr, rng, 0.04, 2400.0, 1200.0, 0.5, 0.0005, 1);
            mix_into(&mut a, &crack, 0, 0.7);
            let rattle = clicks(sr, rng, 5, 0.12, 1200.0, 2800.0);
            mix_into(&mut a, &rattle, (0.02 * sr) as usize, 0.3);
            a
        }
        Material::Shell => {
            // Chitin giving way: a brittle crunch over a small thump.
            let mut a = tone(sr, 0.1, 150.0, 90.0, 0.03);
            let crunch = clicks(sr, rng, 22, 0.07, 1500.0, 4500.0);
            mix_into(&mut a, &crunch, 0, 1.0);
            let body = filtered_noise(sr, rng, 0.08, 1600.0, 700.0, 0.5, 0.001, 1);
            mix_into(&mut a, &body, 0, 0.5);
            a
        }
        Material::Stone | Material::Wall => {
            let wall = m == Material::Wall;
            // A heavy, dead thud: no ring at all, only mass and grit.
            let mut a = tone(sr, 0.2, rr(rng, 70.0, 90.0), 45.0, 0.06);
            let body = filtered_noise(sr, rng, 0.12, 700.0, 200.0, 0.8, 0.001, 0);
            mix_into(&mut a, &body, 0, 0.9);
            let grit = clicks(sr, rng, if wall { 4 } else { 10 }, 0.15, 900.0, 2600.0);
            mix_into(&mut a, &grit, (0.01 * sr) as usize, 0.35);
            if wall {
                // The arrow shaft quivering where it struck.
                let q = bell(sr, rr(rng, 260.0, 320.0), 0.25, &[(1.0, 1.0), (2.7, 0.3)], 0.06);
                mix_into(&mut a, &q, 0, 0.3);
            }
            a
        }
        Material::Crystal => {
            // A bright glassy ring with a shimmer of high partials and a tink.
            let glass: &[(f32, f32)] = &[(1.0, 1.0), (2.76, 0.55), (5.4, 0.35), (8.93, 0.2)];
            let f = rr(rng, 1300.0, 1900.0);
            let mut a = bell(sr, f, 0.9, glass, 0.3);
            let b = bell(sr, f * 1.5, 0.6, glass, 0.18);
            mix_into(&mut a, &b, (0.012 * sr) as usize, 0.4);
            let tink = clicks(sr, rng, 4, 0.05, 4500.0, 6500.0);
            mix_into(&mut a, &tink, 0, 0.5);
            a
        }
        Material::Armor => {
            // A muffled iron thunk: struck mail over a body, not a bell.
            let mut a = tone(sr, 0.15, 130.0, 80.0, 0.045);
            let iron = bell(sr, rr(rng, 260.0, 340.0), 0.15, &[(1.0, 1.0), (2.41, 0.4), (3.93, 0.2)], 0.025);
            mix_into(&mut a, &iron, 0, 0.45);
            let mail = clicks(sr, rng, 8, 0.06, 2000.0, 4000.0);
            mix_into(&mut a, &mail, 0, 0.25);
            a
        }
        Material::Spirit => {
            // A hollow rush through cold mist, and a faint low moan of air.
            let mut a = filtered_noise(sr, rng, 0.35, 2200.0, 500.0, 0.3, 0.02, 1);
            let low = tone(sr, 0.35, 90.0, 70.0, 0.15);
            mix_into(&mut a, &low, 0, 0.4);
            a
        }
        Material::Flame => {
            // A sizzling puff: hiss over a soft whump of air.
            let mut a = filtered_noise(sr, rng, 0.3, 4500.0, 3000.0, 0.6, 0.005, 2);
            let whump = filtered_noise(sr, rng, 0.2, 400.0, 150.0, 0.8, 0.005, 0);
            mix_into(&mut a, &whump, 0, 1.0);
            let pops = clicks(sr, rng, 6, 0.25, 2000.0, 5000.0);
            mix_into(&mut a, &pops, 0, 0.4);
            a
        }
    };
    normalize(&mut a, if m == Material::Wall { 0.55 } else { 0.9 });
    a
}

// ================================================================ effects

fn synth<R: Rng>(sfx: Sfx, sr: f32, rng: &mut R) -> Vec<f32> {
    let mut v = match sfx {
        Sfx::Step => {
            let f0 = rr(rng, 350.0, 500.0);
            let mut a = filtered_noise(sr, rng, 0.08, f0, 150.0, 0.7, 0.002, 0);
            let b = tone(sr, 0.07, 75.0, 50.0, 0.03);
            mix_into(&mut a, &b, 0, 0.5);
            normalize(&mut a, 0.45);
            a
        }
        Sfx::Swing | Sfx::Miss => {
            let dur = rr(rng, 0.18, 0.26);
            let n = (dur * sr) as usize;
            let mut f = Svf::new();
            let mut a = Vec::with_capacity(n);
            for i in 0..n {
                let x = i as f32 / n as f32;
                let fc = 400.0 + 2600.0 * (PI * x).sin();
                let (_, b, _) = f.process(nz(rng), fc, 0.6, sr);
                a.push(b * (PI * x).sin().powf(1.5));
            }
            normalize(&mut a, if sfx == Sfx::Miss { 0.4 } else { 0.6 });
            a
        }
        Sfx::Hit => {
            let mut a = filtered_noise(sr, rng, 0.12, 1500.0, 300.0, 0.6, 0.001, 0);
            let b = tone(sr, 0.15, 110.0, 55.0, 0.05);
            mix_into(&mut a, &b, 0, 0.8);
            normalize(&mut a, 0.9);
            a
        }
        Sfx::Clang => {
            let mut a = bell(sr, rr(rng, 500.0, 800.0), 0.5, &[(1.0, 1.0), (2.41, 0.6), (3.93, 0.4), (5.4, 0.25)], 0.12);
            let c = filtered_noise(sr, rng, 0.02, 4000.0, 2000.0, 0.5, 0.0005, 1);
            mix_into(&mut a, &c, 0, 1.0);
            normalize(&mut a, 0.8);
            a
        }
        Sfx::PlayerHurt => {
            let p = VP { pitch: rr(rng, 130.0, 160.0), dur: 0.25, glide: -0.2, vib: 0.02, vib_rate: 6.0, noise: 0.3, formant: 700.0, formant2: 1400.0, rough: 0.4, q: 0.35 };
            let mut a = vocal(&p, sr, rng);
            let b = filtered_noise(sr, rng, 0.1, 1200.0, 300.0, 0.6, 0.001, 0);
            mix_into(&mut a, &b, 0, 0.6);
            normalize(&mut a, 0.85);
            a
        }
        Sfx::PlayerDie => {
            let p = VP { pitch: 150.0, dur: 1.4, glide: -0.6, vib: 0.03, vib_rate: 5.0, noise: 0.35, formant: 700.0, formant2: 1300.0, rough: 0.5, q: 0.35 };
            let mut a = vocal(&p, sr, rng);
            let b = filtered_noise(sr, rng, 2.0, 200.0, 60.0, 1.0, 0.3, 0);
            mix_into(&mut a, &b, 0, 1.0);
            normalize(&mut a, 0.9);
            a
        }
        Sfx::Voice(k, m) => monster_voice(k, m, sr, rng),
        Sfx::Drip => {
            let mut a = tone(sr, 0.09, rr(rng, 700.0, 1000.0), rr(rng, 1500.0, 2000.0), 0.025);
            if rng.gen_bool(0.6) {
                let b = tone(sr, 0.07, 1200.0, 2100.0, 0.02);
                let off = (rr(rng, 0.1, 0.2) * sr) as usize;
                mix_into(&mut a, &b, off, 0.4);
            }
            normalize(&mut a, 0.5);
            a
        }
        Sfx::Rumble => {
            let dur = rr(rng, 2.0, 3.2);
            let mut a = filtered_noise(sr, rng, dur, 140.0, 50.0, 1.0, 0.6, 0);
            normalize(&mut a, 0.9);
            a
        }
        Sfx::Chains => {
            let n = rng.gen_range(5..10);
            let mut a = Vec::new();
            let mut off = 0usize;
            for _ in 0..n {
                let b = bell(sr, rr(rng, 1800.0, 3200.0), 0.15, &[(1.0, 1.0), (2.7, 0.5), (5.1, 0.3)], 0.03);
                mix_into(&mut a, &b, off, rr(rng, 0.4, 1.0));
                off += (rr(rng, 0.04, 0.12) * sr) as usize;
            }
            normalize(&mut a, 0.6);
            a
        }
        Sfx::Whisper => {
            let dur = rr(rng, 1.4, 2.2);
            let n = (dur * sr) as usize;
            let mut a = Vec::with_capacity(n);
            let mut f1 = Svf::new();
            let mut f2 = Svf::new();
            let mut syl_left = 0usize;
            let mut syl_len = 1usize;
            let mut fa = 1000.0;
            let mut fb = 2000.0;
            for i in 0..n {
                if syl_left == 0 {
                    syl_len = (rr(rng, 0.12, 0.22) * sr) as usize;
                    syl_left = syl_len;
                    fa = rr(rng, 600.0, 1400.0);
                    fb = rr(rng, 1600.0, 2800.0);
                }
                let sx = 1.0 - syl_left as f32 / syl_len as f32;
                syl_left -= 1;
                let x = i as f32 / n as f32;
                let env = (PI * sx).sin() * (PI * x).sin();
                let s = nz(rng);
                let (_, b1, _) = f1.process(s, fa, 0.2, sr);
                let (_, b2, _) = f2.process(s, fb, 0.2, sr);
                a.push((b1 + b2 * 0.7) * env);
            }
            normalize(&mut a, 0.5);
            a
        }
        Sfx::Creak => {
            let p = VP { pitch: rr(rng, 45.0, 75.0), dur: rr(rng, 0.6, 0.9), glide: 0.3, vib: 0.2, vib_rate: 3.0, noise: 0.1, formant: 900.0, formant2: 1700.0, rough: 1.0, q: 0.3 };
            let mut a = vocal(&p, sr, rng);
            normalize(&mut a, 0.6);
            a
        }
        Sfx::Wind => {
            let mut a = filtered_noise(sr, rng, 3.0, 300.0, 550.0, 0.5, 1.0, 1);
            normalize(&mut a, 0.5);
            a
        }
        Sfx::Bones => clicks(sr, rng, 10, 0.3, 1200.0, 3000.0),
        Sfx::Fireball => {
            let mut a = filtered_noise(sr, rng, 0.55, 200.0, 1800.0, 0.6, 0.05, 0);
            let c = clicks(sr, rng, 14, 0.5, 2000.0, 5000.0);
            mix_into(&mut a, &c, 0, 0.3);
            normalize(&mut a, 0.8);
            a
        }
        Sfx::Explosion => {
            let mut a = filtered_noise(sr, rng, 1.4, 1200.0, 80.0, 0.8, 0.003, 0);
            let b = tone(sr, 0.9, 60.0, 30.0, 0.3);
            mix_into(&mut a, &b, 0, 1.0);
            let c = clicks(sr, rng, 25, 0.9, 1500.0, 4500.0);
            mix_into(&mut a, &c, 0, 0.25);
            normalize(&mut a, 1.0);
            a
        }
        Sfx::Frost => {
            let mut a = Vec::new();
            for _ in 0..7 {
                let b = bell(sr, rr(rng, 2000.0, 5000.0), 0.6, &[(1.0, 1.0), (2.3, 0.3)], 0.15);
                let off = (rr(rng, 0.0, 0.3) * sr) as usize;
                mix_into(&mut a, &b, off, 0.5);
            }
            let c = filtered_noise(sr, rng, 0.5, 6000.0, 7000.0, 0.5, 0.01, 2);
            mix_into(&mut a, &c, 0, 0.3);
            normalize(&mut a, 0.7);
            a
        }
        Sfx::Arrow => {
            // The string: a taut, low pluck that dies fast, with a little
            // buzz of harmonics and the slap of the string on the bracer.
            let f = rr(rng, 95.0, 125.0);
            let mut a = bell(sr, f, 0.22, &[(1.0, 1.0), (2.0, 0.45), (3.0, 0.25), (4.0, 0.12)], 0.05);
            let slap = filtered_noise(sr, rng, 0.03, 1800.0, 900.0, 0.6, 0.0005, 1);
            mix_into(&mut a, &slap, 0, 0.6);
            // The arrow: a thin whistle of air falling away as it flies off.
            let fly = filtered_noise(sr, rng, 0.42, 5200.0, 1700.0, 0.25, 0.015, 1);
            mix_into(&mut a, &fly, (0.015 * sr) as usize, 0.55);
            normalize(&mut a, 0.6);
            a
        }
        Sfx::Impact(m) => impact(m, sr, rng),
        Sfx::GritThrow => {
            // The arm whipping through: a breathy swish that rises then falls...
            let dur = rr(rng, 0.24, 0.3);
            let n = (dur * sr) as usize;
            let mut f = Svf::new();
            let mut a = Vec::with_capacity(n);
            for i in 0..n {
                let x = i as f32 / n as f32;
                let fc = 700.0 + 3800.0 * (PI * x).sin();
                let (_, b, _) = f.process(nz(rng), fc, 0.45, sr);
                a.push(b * (PI * x).sin().powf(1.3));
            }
            // ...and the sand itself hissing as the cloud spreads out.
            let hiss = filtered_noise(sr, rng, 0.35, 7000.0, 4500.0, 0.3, 0.04, 2);
            mix_into(&mut a, &hiss, (0.08 * sr) as usize, 0.35);
            let sand = clicks(sr, rng, 26, 0.3, 3000.0, 7000.0);
            mix_into(&mut a, &sand, (0.06 * sr) as usize, 0.18);
            normalize(&mut a, 0.6);
            a
        }
        Sfx::GritHit => {
            // A dry, scratchy spatter, close and all around.
            let mut a = clicks(sr, rng, 70, 0.22, 2500.0, 8000.0);
            let puff = filtered_noise(sr, rng, 0.18, 3000.0, 1200.0, 0.4, 0.003, 1);
            mix_into(&mut a, &puff, 0, 0.6);
            normalize(&mut a, 0.55);
            a
        }
        Sfx::Bolt => {
            let p = VP { pitch: rr(rng, 1200.0, 1500.0), dur: 0.35, glide: -0.85, vib: 0.08, vib_rate: 25.0, noise: 0.1, formant: 1500.0, formant2: 3000.0, rough: 0.0, q: 0.4 };
            let mut a = vocal(&p, sr, rng);
            normalize(&mut a, 0.7);
            a
        }
        Sfx::Heal => {
            let mut a = Vec::new();
            for (i, f) in [523.25f32, 659.25, 783.99, 1046.5].iter().enumerate() {
                let b = bell(sr, *f, 0.8, &[(1.0, 1.0), (2.0, 0.2)], 0.3);
                mix_into(&mut a, &b, (i as f32 * 0.1 * sr) as usize, 0.5);
            }
            normalize(&mut a, 0.6);
            a
        }
        Sfx::LevelUp => {
            let mut a = Vec::new();
            for (i, f) in [392.0f32, 523.25, 659.25, 783.99, 1046.5].iter().enumerate() {
                let b = bell(sr, *f, 1.0, &[(1.0, 1.0), (2.0, 0.3), (3.0, 0.1)], 0.4);
                mix_into(&mut a, &b, (i as f32 * 0.09 * sr) as usize, 0.5);
            }
            for f in [523.25f32, 659.25, 783.99] {
                let b = bell(sr, f, 1.5, &[(1.0, 1.0), (2.0, 0.2)], 0.6);
                mix_into(&mut a, &b, (0.5 * sr) as usize, 0.35);
            }
            normalize(&mut a, 0.8);
            a
        }
        Sfx::Pickup | Sfx::Menu => {
            let mut a = tone(sr, 0.08, 880.0, 880.0, 0.04);
            let b = tone(sr, 0.1, 1320.0, 1320.0, 0.05);
            mix_into(&mut a, &b, (0.06 * sr) as usize, 0.8);
            normalize(&mut a, if sfx == Sfx::Menu { 0.3 } else { 0.5 });
            a
        }
        Sfx::Stairs => {
            let mut a = Vec::new();
            for i in 0..4 {
                let b = filtered_noise(sr, rng, 0.09, 450.0, 150.0, 0.7, 0.002, 0);
                mix_into(&mut a, &b, (i as f32 * 0.22 * sr) as usize, 1.0 - i as f32 * 0.15);
            }
            normalize(&mut a, 0.6);
            a
        }
        Sfx::Revive => {
            let mut a = monster_voice(VoiceKind::Moan, Mood::Die, sr, rng);
            a.reverse();
            let b = filtered_noise(sr, rng, 1.5, 100.0, 400.0, 0.8, 1.2, 0);
            mix_into(&mut a, &b, 0, 0.5);
            normalize(&mut a, 0.8);
            a
        }
        Sfx::Trap => {
            let mut a = bell(sr, 900.0, 0.3, &[(1.0, 1.0), (2.6, 0.5)], 0.05);
            let b = filtered_noise(sr, rng, 0.25, 600.0, 3000.0, 0.6, 0.01, 1);
            mix_into(&mut a, &b, (0.03 * sr) as usize, 0.8);
            normalize(&mut a, 0.8);
            a
        }
        Sfx::Amulet => {
            let mut a = Vec::new();
            for f in [55.0f32, 82.4, 110.0, 130.8, 164.8] {
                let b = bell(sr, f, 3.0, &[(1.0, 1.0), (2.0, 0.4), (3.01, 0.2)], 1.5);
                mix_into(&mut a, &b, 0, 0.3);
            }
            let c = filtered_noise(sr, rng, 3.0, 3000.0, 6000.0, 0.4, 1.0, 2);
            mix_into(&mut a, &c, 0, 0.15);
            normalize(&mut a, 0.9);
            a
        }
        Sfx::Blink => {
            let mut a = tone(sr, 0.3, 300.0, 3000.0, 0.2);
            let mut b = a.clone();
            b.reverse();
            mix_into(&mut a, &b, 0, 0.5);
            normalize(&mut a, 0.5);
            a
        }
        Sfx::Stomp => {
            let mut a = tone(sr, 0.6, 55.0, 28.0, 0.25);
            let b = filtered_noise(sr, rng, 0.5, 400.0, 80.0, 0.8, 0.002, 0);
            mix_into(&mut a, &b, 0, 0.8);
            normalize(&mut a, 1.0);
            a
        }
    };
    if v.is_empty() {
        v.push(0.0);
    }
    v
}

// ================================================================ spatialisation

struct Comb {
    buf: Vec<f32>,
    idx: usize,
    fb: f32,
    damp: f32,
    store: f32,
}

impl Comb {
    fn new(len: usize, fb: f32, damp: f32) -> Comb {
        Comb { buf: vec![0.0; len.max(1)], idx: 0, fb, damp, store: 0.0 }
    }
    fn process(&mut self, x: f32) -> f32 {
        let o = self.buf[self.idx];
        self.store = o * (1.0 - self.damp) + self.store * self.damp;
        self.buf[self.idx] = x + self.store * self.fb;
        self.idx = (self.idx + 1) % self.buf.len();
        o
    }
}

struct Allpass {
    buf: Vec<f32>,
    idx: usize,
}

impl Allpass {
    fn new(len: usize) -> Allpass {
        Allpass { buf: vec![0.0; len.max(1)], idx: 0 }
    }
    fn process(&mut self, x: f32) -> f32 {
        let b = self.buf[self.idx];
        let o = -x + b;
        self.buf[self.idx] = x + b * 0.5;
        self.idx = (self.idx + 1) % self.buf.len();
        o
    }
}

fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
    (a.cos(), a.sin())
}

/// Turn a mono effect into a stereo buffer with filtering, panning, echoes
/// and a Schroeder reverb tail sized to the surrounding room.
pub fn spatialize(mono: &[f32], sp: &Spatial, sr: f32) -> Vec<f32> {
    let n = mono.len();
    let max_echo = sp.echoes.iter().fold(0.0f32, |a, e| a.max(e.0));
    let tail = if sp.reverb > 0.01 { (sr * (0.4 + sp.room * 1.6)) as usize } else { 0 };
    let len = n + tail + (max_echo * sr) as usize + 16;
    let mut out = vec![0.0f32; len * 2];

    let a = 1.0 - (-2.0 * PI * sp.cutoff.min(sr * 0.45) / sr).exp();
    let mut lp = 0.0;
    let mut lp2 = 0.0;
    let mut dry = vec![0.0f32; n];
    for i in 0..n {
        lp += a * (mono[i] - lp);
        lp2 += a * (lp - lp2);
        dry[i] = lp2 * sp.gain;
    }
    let (gl, gr) = pan_gains(sp.pan);
    for i in 0..n {
        out[i * 2] += dry[i] * gl;
        out[i * 2 + 1] += dry[i] * gr;
    }
    for &(d, g, p) in &sp.echoes {
        let off = (d * sr) as usize;
        let (l, r) = pan_gains(p);
        for i in 0..n {
            let j = i + off;
            if j >= len {
                break;
            }
            out[j * 2] += dry[i] * g * l;
            out[j * 2 + 1] += dry[i] * g * r;
        }
    }
    if sp.reverb > 0.01 {
        let scale = 0.7 + sp.room * 0.8;
        let fb = (0.72 + sp.room * 0.2).min(0.92);
        let damp = 0.35;
        for ch in 0..2 {
            let spread = if ch == 0 { 0 } else { 23 };
            let mut combs: Vec<Comb> = [1116usize, 1188, 1277, 1356]
                .iter()
                .map(|&l| Comb::new(((l + spread) as f32 * scale * sr / 44100.0) as usize, fb, damp))
                .collect();
            let mut aps: Vec<Allpass> = [556usize, 441]
                .iter()
                .map(|&l| Allpass::new(((l + spread) as f32 * sr / 44100.0) as usize))
                .collect();
            for i in 0..len {
                let x = if i < n { dry[i] * sp.reverb * 0.25 } else { 0.0 };
                let mut y = 0.0;
                for c in combs.iter_mut() {
                    y += c.process(x);
                }
                for ap in aps.iter_mut() {
                    y = ap.process(y);
                }
                out[i * 2 + ch] += y;
            }
        }
    }
    out
}

// ================================================================ music

struct Note {
    f: f32,
    ph: f32,
    ph2: f32,
    amp: f32,
    dec: f32,
    pan: f32,
    ratio: f32,
}

struct Drum {
    ph: f32,
    amp: f32,
    t: f32,
    kind: u8,
}

/// Generative score: a drone, sparse modal bell melodies with a ping-pong
/// delay, wind ambience, and combat drums / tremolo strings driven by
/// `intensity`.
struct Music {
    sr: f32,
    mode: MusicMode,
    depth: u32,
    intensity: f32,
    target: f32,
    rng: u32,
    t: u64,
    drone_ph: [f32; 3],
    lp1: f32,
    lp2: f32,
    lfo_ph: f32,
    notes: Vec<Note>,
    next_note: u64,
    deg: i32,
    next_beat: u64,
    beat: u32,
    drums: Vec<Drum>,
    dl: Vec<f32>,
    dr: Vec<f32>,
    dpos: usize,
    wind_lp: f32,
    wind_lp2: f32,
    wind_ph: f32,
    str_ph: f32,
    gain: f32,
    /// Carrying the Amulet back up.
    amulet: bool,
    /// Smoothed 0..1 of how deep and bleak the music is.
    dark: f32,
    /// Smoothed 0..1 of how hopeful the climb sounds.
    hope: f32,
    /// Smoothed 0..1 of the Amulet's frantic edge.
    panic: f32,
    /// Next soft heartbeat while carrying the Amulet.
    next_pulse: u64,
}

const AEOLIAN: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];
const HARMONIC: [i32; 7] = [0, 2, 3, 5, 7, 8, 11];
const PHRYGIAN: [i32; 7] = [0, 1, 3, 5, 7, 8, 10];
const LOCRIAN: [i32; 7] = [0, 1, 3, 5, 6, 8, 10];
const MAJOR: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
/// Minor with a raised sixth: dark, but with light coming through.
const DORIAN: [i32; 7] = [0, 2, 3, 5, 7, 9, 10];

impl Music {
    fn new(sr: f32) -> Music {
        let dlen = (sr * 0.45) as usize;
        Music {
            sr,
            mode: MusicMode::Title,
            depth: 1,
            intensity: 0.0,
            target: 0.0,
            rng: 0x1234_5678,
            t: 0,
            drone_ph: [0.0; 3],
            lp1: 0.0,
            lp2: 0.0,
            lfo_ph: 0.0,
            notes: Vec::new(),
            next_note: (sr * 1.5) as u64,
            deg: 7,
            next_beat: 0,
            beat: 0,
            drums: Vec::new(),
            dl: vec![0.0; dlen.max(1)],
            dr: vec![0.0; dlen.max(1)],
            dpos: 0,
            wind_lp: 0.0,
            wind_lp2: 0.0,
            wind_ph: 0.0,
            str_ph: 0.0,
            gain: 0.0,
            amulet: false,
            dark: 0.0,
            hope: 0.0,
            panic: 0.0,
            next_pulse: 0,
        }
    }

    /// Where the music is heading: (dark, hope, panic), each 0..1. Darkness
    /// grows with depth. With the Amulet the music is frantic, and hope grows
    /// with each floor climbed back toward the surface.
    fn moods(&self) -> (f32, f32, f32) {
        if self.mode != MusicMode::Dungeon {
            return (0.0, 0.0, 0.0);
        }
        let deep = (self.depth.clamp(1, 10) - 1) as f32 / 9.0;
        if self.amulet {
            let hope = 1.0 - deep;
            (deep * 0.85, hope, 1.0 - 0.35 * hope)
        } else {
            (deep, 0.0, 0.0)
        }
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32) / (u32::MAX as f32)
    }

    fn scale(&self) -> &'static [i32; 7] {
        match self.mode {
            MusicMode::Victory => &MAJOR,
            MusicMode::Title => &AEOLIAN,
            MusicMode::Death => &PHRYGIAN,
            // The way back up: the further you climb, the brighter the mode.
            MusicMode::Dungeon if self.amulet => match self.depth {
                1..=2 => &DORIAN,
                3..=5 => &AEOLIAN,
                6..=8 => &PHRYGIAN,
                _ => &LOCRIAN,
            },
            MusicMode::Dungeon => match self.depth {
                1..=3 => &AEOLIAN,
                4..=6 => &HARMONIC,
                7..=9 => &PHRYGIAN,
                _ => &LOCRIAN,
            },
        }
    }

    fn root(&self) -> f32 {
        let semi = match self.mode {
            MusicMode::Title => -2,
            MusicMode::Death => -5,
            MusicMode::Victory => 3,
            // A semitone lower for every floor down: depth 10 sits nine
            // semitones under the first floor.
            MusicMode::Dungeon => -(self.depth.clamp(1, 10) as i32 - 1),
        };
        110.0 * 2f32.powf(semi as f32 / 12.0)
    }

    fn spawn_note(&mut self) {
        let step = (self.rand() * 5.0) as i32 - 2;
        self.deg = (self.deg + step).clamp(0, 14);
        let sc = self.scale();
        let semi = sc[(self.deg % 7) as usize] + 12 * (self.deg / 7);
        let f = self.root() * 2f32.powf(semi as f32 / 12.0) * 2.0;
        let dark = self.dark;
        let pluck = self.rand() < 0.35 * (1.0 - 0.6 * dark) + 0.3 * self.panic;
        let dec_time = if pluck { 0.6 } else { 2.2 + 1.8 * dark };
        let pan = (self.rand() - 0.5) * 1.2;
        self.notes.push(Note {
            f,
            ph: 0.0,
            ph2: 0.0,
            amp: if pluck { 0.07 } else { 0.06 },
            dec: (-1.0 / (self.sr * dec_time)).exp(),
            pan,
            ratio: if pluck { 2.0 } else { 2.76 },
        });
        // The deep: now and then a second note grinds against the first, a
        // semitone or a tritone away.
        if self.rand() < 0.35 * dark {
            let rub = if self.rand() < 0.5 { 1.0595 } else { 1.4142 };
            self.notes.push(Note { f: f * rub, ph: 0.0, ph2: 0.0, amp: 0.035, dec: (-1.0 / (self.sr * dec_time)).exp(), pan: -pan, ratio: 2.76 });
        }
        // The climb: the consonant third that the victory music uses.
        let hope_third = self.mode == MusicMode::Dungeon && self.rand() < 0.45 * self.hope;
        if (self.mode == MusicMode::Victory && self.rand() < 0.5) || hope_third {
            // add a third above for consonance
            let semi3 = sc[((self.deg + 2) % 7) as usize] + 12 * ((self.deg + 2) / 7);
            let f3 = self.root() * 2f32.powf(semi3 as f32 / 12.0) * 2.0;
            self.notes.push(Note { f: f3, ph: 0.0, ph2: 0.0, amp: 0.04, dec: (-1.0 / (self.sr * 2.0)).exp(), pan: -pan, ratio: 2.0 });
        }
        let base = 0.5 + self.rand() * 2.3;
        let mul = match self.mode {
            MusicMode::Death => 2.0,
            MusicMode::Victory => 0.35,
            MusicMode::Title => 1.2,
            // Deeper, the notes thin out into silence; the Amulet hurries them.
            MusicMode::Dungeon => {
                (1.0 - 0.5 * self.intensity) * (1.0 + 0.9 * dark) * (1.0 - 0.45 * self.panic)
            }
        };
        self.next_note = self.t + (self.sr * base * mul) as u64;
    }

    fn render(&mut self, out: &mut [f32], frames: usize) {
        let sr = self.sr;
        let root = self.root();
        let dungeon = self.mode == MusicMode::Dungeon;
        let drone_gain = match self.mode {
            MusicMode::Victory => 0.05,
            _ => 0.1,
        };
        let fifth = if self.mode == MusicMode::Dungeon && self.depth >= 10 { 1.4142 } else { 1.4983 };
        let (dk, hp, pn) = self.moods();
        for i in 0..frames {
            self.t += 1;
            // Ease into each floor's mood over a few seconds.
            let ease = 1.0 / (sr * 4.0);
            self.dark += (dk - self.dark) * ease;
            self.hope += (hp - self.hope) * ease;
            self.panic += (pn - self.panic) * ease;
            // Carrying the Amulet: a soft, quickened heartbeat under it all.
            if dungeon && self.panic > 0.05 && self.t >= self.next_pulse {
                let bpm = 84.0 + 20.0 * self.panic + 30.0 * self.intensity;
                self.next_pulse = self.t + (sr * 60.0 / bpm) as u64;
                let a = 0.16 * self.panic;
                self.drums.push(Drum { ph: 0.0, amp: a, t: 0.0, kind: 0 });
                self.drums.push(Drum { ph: 0.0, amp: a * 0.6, t: -0.16, kind: 0 });
            }
            self.gain = (self.gain + 1.0 / (sr * 3.0)).min(1.0);
            self.intensity += (self.target - self.intensity) * (1.0 / (sr * 1.5));
            if self.t >= self.next_note {
                self.spawn_note();
            }
            if dungeon && self.intensity > 0.25 && self.t >= self.next_beat {
                let bpm = 70.0 + 60.0 * self.intensity;
                self.next_beat = self.t + (sr * 60.0 / bpm) as u64;
                if self.beat % 2 == 0 {
                    self.drums.push(Drum { ph: 0.0, amp: 0.35 * self.intensity, t: 0.0, kind: 0 });
                }
                if self.intensity > 0.6 && self.beat % 4 == 3 {
                    self.drums.push(Drum { ph: 0.0, amp: 0.2 * self.intensity, t: 0.0, kind: 1 });
                }
                self.beat = self.beat.wrapping_add(1);
            }

            // drone
            self.lfo_ph += TAU * 0.05 / sr;
            if self.lfo_ph > TAU {
                self.lfo_ph -= TAU;
            }
            let lfo = self.lfo_ph.sin();
            // The third voice drifts further out of tune the deeper you are,
            // so the drone beats and wavers.
            let freqs = [root * 0.5, root * 0.5 * fifth, root * 0.5 * (1.003 + 0.014 * self.dark)];
            let mut d = 0.0;
            for k in 0..3 {
                self.drone_ph[k] += freqs[k] / sr;
                self.drone_ph[k] -= self.drone_ph[k].floor();
                d += 2.0 * self.drone_ph[k] - 1.0;
            }
            let cutoff = (160.0 + 220.0 * (lfo * 0.5 + 0.5)) * (1.0 - 0.35 * self.dark)
                + self.intensity * 500.0
                + 120.0 * self.hope;
            let a = 1.0 - (-TAU * cutoff / sr).exp();
            self.lp1 += a * (d - self.lp1);
            self.lp2 += a * (self.lp1 - self.lp2);
            let drone = self.lp2 * drone_gain * (0.8 + 0.2 * lfo);

            // bell notes
            let mut nl = 0.0;
            let mut nr = 0.0;
            for n in self.notes.iter_mut() {
                n.ph += n.f / sr;
                n.ph -= n.ph.floor();
                n.ph2 += n.f * n.ratio / sr;
                n.ph2 -= n.ph2.floor();
                let v = ((TAU * n.ph).sin() + 0.35 * (TAU * n.ph2).sin()) * n.amp;
                n.amp *= n.dec;
                let pa = (n.pan + 1.0) * PI / 4.0;
                nl += v * pa.cos();
                nr += v * pa.sin();
            }
            if i % 256 == 0 {
                self.notes.retain(|n| n.amp > 0.0005);
            }

            // tension strings
            let mut strings = 0.0;
            // Tension strings: in a fight, and always (if softly) with the
            // Amulet, trembling faster.
            let str_amt = ((self.intensity - 0.4) / 0.6).max(0.0).max(0.45 * self.panic);
            if dungeon && str_amt > 0.0 {
                self.str_ph += root * 2.0 * 1.0595 / sr;
                self.str_ph -= self.str_ph.floor();
                let rate = 7.0 + 4.0 * self.panic;
                let trem = 0.5 + 0.5 * (TAU * rate * self.t as f32 / sr).sin();
                strings = (2.0 * self.str_ph - 1.0) * 0.02 * str_amt.min(1.0) * trem;
            }

            // drums
            let mut dsum = 0.0;
            let mut noise_v = 0.0;
            for dr in self.drums.iter_mut() {
                if dr.t < 0.0 {
                    // Not struck yet (the second beat of a heartbeat).
                    dr.t += 1.0 / sr;
                    continue;
                }
                if dr.kind == 0 {
                    let f = 40.0 + 80.0 * (-dr.t * 30.0).exp();
                    dr.ph += f / sr;
                    dsum += (TAU * dr.ph).sin() * dr.amp * (-dr.t * 7.0).exp();
                } else {
                    let f = 90.0 + 40.0 * (-dr.t * 20.0).exp();
                    dr.ph += f / sr;
                    noise_v += dr.amp * (-dr.t * 18.0).exp();
                    dsum += (TAU * dr.ph).sin() * dr.amp * 0.8 * (-dr.t * 10.0).exp();
                }
                dr.t += 1.0 / sr;
            }
            if noise_v > 0.0 {
                dsum += (self.rand() * 2.0 - 1.0) * noise_v * 0.4;
            }
            if i % 256 == 0 {
                self.drums.retain(|d| d.t < 1.0);
            }

            // ping-pong delay on the bells
            let dlv = self.dl[self.dpos];
            let drv = self.dr[self.dpos];
            // The deep echoes on and on.
            let fb = 0.45 + 0.2 * self.dark;
            self.dl[self.dpos] = nl * 0.5 + drv * fb;
            self.dr[self.dpos] = nr * 0.5 + dlv * fb;
            self.dpos = (self.dpos + 1) % self.dl.len();

            // wind
            let mut wind = 0.0;
            if dungeon || self.mode == MusicMode::Title {
                self.wind_ph += TAU * 0.07 / sr;
                if self.wind_ph > TAU {
                    self.wind_ph -= TAU;
                }
                let wc = (250.0 + 200.0 * self.wind_ph.sin()) * (1.0 - 0.4 * self.dark);
                let wa = 1.0 - (-TAU * wc / sr).exp();
                let w = self.rand() * 2.0 - 1.0;
                self.wind_lp += wa * (w - self.wind_lp);
                self.wind_lp2 += wa * (self.wind_lp - self.wind_lp2);
                let howl = 1.0 + 1.3 * self.dark - 0.4 * self.hope;
                wind = self.wind_lp2 * 0.09 * howl * (0.6 + 0.4 * (self.wind_ph * 0.5).sin());
            }

            let l = drone + nl + dlv * 0.6 + strings + dsum + wind;
            let r = drone + nr + drv * 0.6 - strings * 0.5 + dsum + wind * 0.9;
            out[i * 2] += l * self.gain * 0.8;
            out[i * 2 + 1] += r * self.gain * 0.8;
        }
    }
}

#[cfg(test)]
pub fn synth_for_test<R: Rng>(sfx: Sfx, sr: f32, rng: &mut R) -> Vec<f32> {
    synth(sfx, sr, rng)
}

#[cfg(test)]
mod music_tests {
    use super::*;

    fn at(depth: u32, amulet: bool) -> Music {
        let mut m = Music::new(44100.0);
        m.mode = MusicMode::Dungeon;
        m.depth = depth;
        m.amulet = amulet;
        m
    }

    #[test]
    fn music_sinks_and_darkens_with_depth() {
        let roots: Vec<f32> = (1..=10).map(|d| at(d, false).root()).collect();
        assert!(roots.windows(2).all(|w| w[1] < w[0]), "pitch drops every floor");
        assert!((roots[0] / roots[9] - 2f32.powf(9.0 / 12.0)).abs() < 1e-3);
        let (d1, _, _) = at(1, false).moods();
        let (d10, h10, p10) = at(10, false).moods();
        assert!(d10 > d1);
        assert_eq!((h10, p10), (0.0, 0.0));
    }

    #[test]
    fn the_amulet_makes_it_frantic_then_hopeful() {
        let (_, h10, p10) = at(10, true).moods();
        let (_, h5, _) = at(5, true).moods();
        let (d1, h1, p1) = at(1, true).moods();
        assert!(p10 > 0.9 && p1 > 0.5, "frantic all the way up");
        assert!(h1 > h5 && h5 > h10, "more hope with each floor climbed");
        assert!(d1 < 0.01);
        assert_eq!(at(1, true).scale(), &DORIAN);
        assert_eq!(at(10, true).scale(), &LOCRIAN);
    }

    #[test]
    fn music_stays_finite_in_every_mood() {
        for (depth, amulet) in [(1, false), (10, false), (10, true), (1, true)] {
            let mut m = at(depth, amulet);
            m.target = 0.9;
            let mut buf = vec![0.0f32; 44100 * 2 * 6];
            m.render(&mut buf, 44100 * 6);
            assert!(buf.iter().all(|x| x.is_finite() && x.abs() < 4.0), "depth {} amulet {}", depth, amulet);
            assert!(m.dark >= 0.0 && m.hope >= 0.0 && m.panic >= 0.0);
        }
    }
}

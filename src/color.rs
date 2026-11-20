//! Colour type and small deterministic noise helpers used by the renderer.

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[inline]
fn cl(v: f32) -> u8 {
    if v <= 0.0 {
        0
    } else if v >= 255.0 {
        255
    } else {
        v as u8
    }
}

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);
    pub const WHITE: Rgb = Rgb(255, 255, 255);

    pub fn scale(self, f: f32) -> Rgb {
        Rgb(cl(self.0 as f32 * f), cl(self.1 as f32 * f), cl(self.2 as f32 * f))
    }

    /// Multiply by an RGB light value.
    pub fn lit(self, l: [f32; 3]) -> Rgb {
        Rgb(
            cl(self.0 as f32 * l[0]),
            cl(self.1 as f32 * l[1]),
            cl(self.2 as f32 * l[2]),
        )
    }

    pub fn lerp(self, o: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        Rgb(
            cl(self.0 as f32 + (o.0 as f32 - self.0 as f32) * t),
            cl(self.1 as f32 + (o.1 as f32 - self.1 as f32) * t),
            cl(self.2 as f32 + (o.2 as f32 - self.2 as f32) * t),
        )
    }

    pub fn add(self, o: Rgb) -> Rgb {
        Rgb(
            self.0.saturating_add(o.0),
            self.1.saturating_add(o.1),
            self.2.saturating_add(o.2),
        )
    }

    pub fn luma(self) -> f32 {
        (0.3 * self.0 as f32 + 0.59 * self.1 as f32 + 0.11 * self.2 as f32) / 255.0
    }
}

pub fn hash2(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

pub fn hashf(x: i32, y: i32) -> f32 {
    (hash2(x, y) & 0xffff) as f32 / 65535.0
}

/// Smooth value noise in [0,1].
pub fn vnoise(x: f32, y: f32) -> f32 {
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let (ix, iy) = (x0 as i32, y0 as i32);
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let a = hashf(ix, iy);
    let b = hashf(ix + 1, iy);
    let c = hashf(ix, iy + 1);
    let d = hashf(ix + 1, iy + 1);
    let ab = a + (b - a) * sx;
    let cd = c + (d - c) * sx;
    ab + (cd - ab) * sy
}

//! Mars-like planet model.
//!
//! Turns a contribution count into a life stage ([`PlanetBiology`]): a gray cratered
//! protoplanet that rusts into a red planet, then gains dark maria, polar ice caps, a thin dusty
//! atmosphere, the Valles Marineris canyon, the Olympus Mons volcano, and the moons Phobos and
//! Deimos. [`surface_at`] describes the terrain at any point of the (rotating) sphere.

use std::f64::consts::TAU;

/// Contribution thresholds where each life stage begins (stage 1 starts at `0`).
pub const STAGE_THRESHOLDS: [u64; 4] = [150, 500, 1200, 2500];

/// Contribution count at which growth saturates (`growth == 1.0`).
const FULL_GROWTH_COMMITS: f64 = 2800.0;

pub(crate) fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

pub(crate) fn smooth_step(t: f64) -> f64 {
    let c = t.clamp(0.0, 1.0);
    c * c * (3.0 - 2.0 * c)
}

/// Life-stage features derived from the contribution count. Every `0.0..=1.0` field fades a
/// feature in so growth is continuous between stages.
#[derive(Clone, Copy, Debug)]
pub struct PlanetBiology {
    pub commits: u64,
    /// Overall growth on a log-like curve, `0.0` to `1.0`.
    pub growth: f64,
    /// Planet radius in scene pixels.
    pub radius: f64,
    /// Gray protoplanet rock (`0.0`) to rusty Martian soil (`1.0`).
    pub redness: f64,
    /// Dark albedo regions.
    pub maria: f64,
    pub north_cap: f64,
    pub south_cap: f64,
    /// Thin dusty haze on the lit limb.
    pub atmosphere: f64,
    /// Valles Marineris canyon and Olympus Mons volcano.
    pub landmarks: f64,
}

impl PlanetBiology {
    /// Compute features for `commits` contributions:
    /// - `0..150`: Stage 1: Protoplanet
    /// - `150..500`: Stage 2: Red Planet
    /// - `500..1,200`: Stage 3: Dark Maria (and the north ice cap)
    /// - `1,200..2,500`: Stage 4: Atmosphere (south cap, haze, Phobos)
    /// - `2,500+`: Stage 5: Full Mars (canyon, volcano, Deimos)
    pub fn from_commits(commits: u64) -> Self {
        let g = ((commits as f64) / FULL_GROWTH_COMMITS)
            .powf(0.45)
            .clamp(0.0, 1.0);
        Self {
            commits,
            growth: g,
            radius: lerp(6.0, 22.0, g),
            redness: smooth_step((g - 0.12) / 0.15),
            maria: smooth_step((g - 0.36) / 0.12),
            north_cap: smooth_step((g - 0.38) / 0.08),
            south_cap: smooth_step((g - 0.60) / 0.08),
            atmosphere: smooth_step((g - 0.60) / 0.08),
            landmarks: smooth_step((g - 0.88) / 0.07),
        }
    }

    /// Zero-based life stage index (`0..=4`).
    pub fn stage_index(&self) -> usize {
        STAGE_THRESHOLDS
            .iter()
            .take_while(|&&t| self.commits >= t)
            .count()
    }

    pub fn stage_name(&self) -> &'static str {
        match self.stage_index() {
            0 => "Protoplanet",
            1 => "Red Planet",
            2 => "Dark Maria & North Ice Cap",
            3 => "Polar Caps, Atmosphere & Phobos",
            _ => "Full Mars (Olympus Mons, Valles Marineris & Deimos)",
        }
    }

    pub fn hud_stage_label(&self) -> &'static str {
        match self.stage_index() {
            0 => "STAGE 1: PROTOPLANET",
            1 => "STAGE 2: RED PLANET",
            2 => "STAGE 3: DARK MARIA",
            3 => "STAGE 4: ATMOSPHERE",
            _ => "STAGE 5: FULL MARS",
        }
    }

    /// Moons that have formed by this stage (Phobos from stage 4, Deimos from stage 5).
    pub fn moons(&self) -> Vec<Moon> {
        let r = self.radius;
        let stage = self.stage_index();
        let mut moons = Vec::with_capacity(2);
        if stage >= 3 {
            moons.push(Moon {
                kind: MoonKind::Phobos,
                orbit: r + 7.0,
                laps_per_loop: 2.0,
                phase_offset: 0.0,
                size: 1.6,
            });
        }
        if stage >= 4 {
            moons.push(Moon {
                kind: MoonKind::Deimos,
                orbit: r + 15.0,
                laps_per_loop: 1.0,
                phase_offset: 2.4,
                size: 1.1,
            });
        }
        moons
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MoonKind {
    Phobos,
    Deimos,
}

/// A small lumpy moon on a tilted elliptical orbit.
#[derive(Clone, Copy, Debug)]
pub struct Moon {
    pub kind: MoonKind,
    pub orbit: f64,
    /// Whole orbits per animation loop (keeps the GIF seamless).
    pub laps_per_loop: f64,
    pub phase_offset: f64,
    pub size: f64,
}

impl Moon {
    /// Orbit angle at animation phase `loop_turn` (`0..TAU`).
    pub fn angle(&self, loop_turn: f64) -> f64 {
        (loop_turn * self.laps_per_loop + self.phase_offset).rem_euclid(TAU)
    }

    /// The moon is in front of the planet during the lower half of its orbit.
    pub fn in_front(&self, loop_turn: f64) -> bool {
        self.angle(loop_turn).sin() > 0.0
    }

    /// Scene-space center relative to the planet center.
    pub fn offset(&self, loop_turn: f64) -> (f64, f64) {
        let a = self.angle(loop_turn);
        (
            a.cos() * self.orbit,
            a.sin() * self.orbit * 0.28 - a.cos() * 1.5,
        )
    }
}

fn hash3(x: i64, y: i64, z: i64, seed: u64) -> f64 {
    let mut n = (x as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((y as u32).wrapping_mul(668_265_263))
        .wrapping_add((z as u32).wrapping_mul(1_274_126_177))
        .wrapping_add((seed as u32).wrapping_mul(1_442_695_041));
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    f64::from((n ^ (n >> 16)) & 0xFFFF) / 65_535.0
}

fn value_noise3(x: f64, y: f64, z: f64, seed: u64) -> f64 {
    let (ix, iy, iz) = (x.floor(), y.floor(), z.floor());
    let fade = |f: f64| f * f * (3.0 - 2.0 * f);
    let (ux, uy, uz) = (fade(x - ix), fade(y - iy), fade(z - iz));
    let (ix, iy, iz) = (ix as i64, iy as i64, iz as i64);
    let mut v = 0.0;
    for dx in 0..2_i64 {
        for dy in 0..2_i64 {
            for dz in 0..2_i64 {
                let wx = if dx == 1 { ux } else { 1.0 - ux };
                let wy = if dy == 1 { uy } else { 1.0 - uy };
                let wz = if dz == 1 { uz } else { 1.0 - uz };
                v += wx * wy * wz * hash3(ix + dx, iy + dy, iz + dz, seed);
            }
        }
    }
    v
}

/// Fractal value noise in `0.0..=1.0`.
fn fbm(x: f64, y: f64, z: f64, seed: u64, octaves: u32) -> f64 {
    let (mut v, mut amp, mut freq, mut total) = (0.0, 0.5, 1.0, 0.0);
    for octave in 0..octaves {
        v += amp
            * value_noise3(
                x * freq,
                y * freq,
                z * freq,
                seed.wrapping_add(u64::from(octave) * 17),
            );
        total += amp;
        amp *= 0.5;
        freq *= 2.1;
    }
    v / total
}

/// Terrain at one point of the sphere.
#[derive(Clone, Copy, Debug)]
pub struct Surface {
    /// Albedo on a `0.0..6.0` scale (index into a 6-shade soil ramp before lighting).
    pub tone: f64,
    pub ice: bool,
}

/// Terrain at body-fixed unit vector `(sx, sy, sz)` (`sy = -1` is the north pole).
pub fn surface_at(bio: &PlanetBiology, seed: u64, sx: f64, sy: f64, sz: f64) -> Surface {
    let lat = -sy;
    let lon = sx.atan2(sz);

    let soil = fbm(sx * 2.2 + 5.0, sy * 2.2, sz * 2.2, seed, 4);
    let mut tone = 3.4 + (soil - 0.5) * 2.2;

    let maria = fbm(sx * 1.4, sy * 1.4 + 9.0, sz * 1.4, seed + 3, 3);
    if maria > 0.55 {
        tone -= bio.maria * (1.0 + (maria - 0.55) * 6.0);
    }

    let crater = fbm(sx * 7.0, sy * 7.0, sz * 7.0, seed + 11, 2);
    if crater > 0.72 {
        tone -= 0.9;
    } else if crater > 0.68 {
        tone += 0.5;
    }

    if bio.landmarks > 0.0 {
        // Valles Marineris: a dark gash along the equator.
        let canyon_lat = 0.08 + 0.05 * (lon * 4.0).sin();
        if (lat - canyon_lat).abs() < 0.045 && (-0.2..1.3).contains(&lon) {
            tone -= 1.8 * bio.landmarks;
        }
        // Olympus Mons: a bright shield volcano ringed by shadow.
        let d = ((lon + 1.1) * 1.2).hypot(lat - 0.3);
        if d < 0.16 {
            tone += 1.2 * bio.landmarks * (1.0 - d / 0.16);
        } else if d < 0.2 {
            tone -= 0.8 * bio.landmarks;
        }
    }

    let ragged = fbm(sx * 5.0, sy * 5.0, sz * 5.0, seed + 7, 2) * 0.12;
    let ice = (bio.north_cap > 0.0 && lat > 1.0 - 0.17 * bio.north_cap - ragged)
        || (bio.south_cap > 0.0 && lat < -1.0 + 0.11 * bio.south_cap + ragged);

    Surface { tone, ice }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planet_grows_and_gains_features() {
        let young = PlanetBiology::from_commits(0);
        let full = PlanetBiology::from_commits(2800);
        assert!(full.radius > young.radius * 3.0);
        assert!(young.redness < 0.01 && full.redness > 0.99);
        assert!(young.moons().is_empty());
        assert_eq!(full.moons().len(), 2);
        assert!(full.landmarks > 0.99);
    }

    #[test]
    fn moon_orbits_are_seamless() {
        let bio = PlanetBiology::from_commits(2800);
        for moon in bio.moons() {
            let (x0, y0) = moon.offset(0.0);
            let (x1, y1) = moon.offset(TAU);
            assert!((x0 - x1).abs() < 1e-9 && (y0 - y1).abs() < 1e-9);
        }
    }

    #[test]
    fn noise_stays_in_unit_range() {
        for i in 0..200 {
            let v = fbm(f64::from(i) * 0.37 - 20.0, f64::from(i) * 0.11, -3.3, 99, 4);
            assert!((0.0..=1.0).contains(&v));
        }
    }
}

//! Procedural soundscape: every sound is synthesised from noise and oscillators, no recording.
//!
//! - Wind: brown noise (white noise integrated: deep, soft, like air moving) through a
//!   low-pass filter opening with the gusts, swelling slowly.
//! - Foliage: high, crackling noise when the wind passes through trees.
//! - Water: a band of rumbling noise and random bubbles (short rising tones).
//! - Rain: a soft hiss and the patter of single drops.
//! - Birds by day (a dawn chorus), crickets at night: tones with a few harmonics, glides and
//!   trills, started at random moments (a Poisson process: constant chance per instant).
//! - Footsteps: two bursts of noise (heel, then toe) shaped by the ground.
//!
//! Left and right use independent noise, so the beds of sound are wide instead of sitting
//! inside the head; birds, crickets and steps go through a small reverb (echoes of a place).
//! The game sets the scene (`Shared`, atomics: no lock in the audio thread); `Soundscape::fill`
//! runs in the audio thread and allocates nothing.

use std::f32::consts::{PI, TAU};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// What the ground under a footstep sounds like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Surface {
    Grass = 0,
    Sand = 1,
    Stone = 2,
    Water = 3,
}

/// The scene as heard, written by the game, read by the audio thread. Values are f32 stored
/// as bits.
#[derive(Default)]
pub struct Shared {
    wind: AtomicU32,
    foliage: AtomicU32,
    water: AtomicU32,
    birds: AtomicU32,
    crickets: AtomicU32,
    rain: AtomicU32,
    /// Incremented at every footstep; the audio thread plays one step per increment.
    steps: AtomicU32,
    step_surface: AtomicU32,
    step_strength: AtomicU32,
}

/// Levels of the ambient sounds, each in [0, 1].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scene {
    pub wind: f32,
    pub foliage: f32,
    pub water: f32,
    pub birds: f32,
    pub crickets: f32,
    pub rain: f32,
}

fn store(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

impl Shared {
    pub fn set_scene(&self, scene: &Scene) {
        store(&self.wind, scene.wind);
        store(&self.foliage, scene.foliage);
        store(&self.water, scene.water);
        store(&self.birds, scene.birds);
        store(&self.crickets, scene.crickets);
        store(&self.rain, scene.rain);
    }

    /// A footstep on `surface`, `strength` 0.3 (walking softly) to 1 (running).
    pub fn step(&self, surface: Surface, strength: f32) {
        self.step_surface.store(surface as u32, Ordering::Relaxed);
        store(&self.step_strength, strength);
        self.steps.fetch_add(1, Ordering::Release);
    }

    fn scene(&self) -> Scene {
        Scene {
            wind: load(&self.wind),
            foliage: load(&self.foliage),
            water: load(&self.water),
            birds: load(&self.birds),
            crickets: load(&self.crickets),
            rain: load(&self.rain),
        }
    }
}

// ---------- Building blocks ----------

/// Fast white noise generator (xorshift), uniform in [−1, 1].
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    fn unit(&mut self) -> f32 {
        self.next() * 0.5 + 0.5
    }
}

/// One-pole low-pass filter: y ← y + a·(x − y). Lets slow changes through, smooths fast ones;
/// `a` = 1 − e^(−2π·fc/fs) puts the cutoff at fc.
#[derive(Default)]
struct LowPass(f32);

impl LowPass {
    fn run(&mut self, x: f32, a: f32) -> f32 {
        self.0 += a * (x - self.0);
        self.0
    }
}

fn coefficient(cutoff: f32, rate: f32) -> f32 {
    1.0 - (-TAU * cutoff / rate).exp()
}

/// Brown noise: white noise integrated, with a slight leak so it does not drift away. Its
/// power falls as 1/f²: mostly low, rolling sound, like wind or distant water.
#[derive(Default)]
struct Brown(f32);

impl Brown {
    fn run(&mut self, white: f32) -> f32 {
        self.0 = (self.0 * 0.996 + white * 0.06).clamp(-1.0, 1.0);
        self.0
    }
}

/// A value that glides towards its target instead of jumping (no clicks when a level
/// changes).
#[derive(Default)]
struct Smooth(f32);

impl Smooth {
    fn towards(&mut self, target: f32, a: f32) -> f32 {
        self.0 += a * (target - self.0);
        self.0
    }
}

/// Ambient levels, smoothed.
#[derive(Default)]
struct Levels {
    wind: f32,
    foliage: f32,
    water: f32,
    crickets: f32,
    rain: f32,
}

/// The steady sounds of one ear (left or right), each with its own noise.
struct Bed {
    noise: Noise,
    brown: Brown,
    wind: LowPass,
    leaf_high: LowPass,
    leaf_envelope: LowPass,
    river_brown: Brown,
    river_low: LowPass,
    river_high: LowPass,
    rain_high: LowPass,
    patter: LowPass,
}

impl Bed {
    fn new(seed: u32) -> Self {
        Self {
            noise: Noise(seed | 1),
            brown: Brown::default(),
            wind: LowPass::default(),
            leaf_high: LowPass::default(),
            leaf_envelope: LowPass::default(),
            river_brown: Brown::default(),
            river_low: LowPass::default(),
            river_high: LowPass::default(),
            rain_high: LowPass::default(),
            patter: LowPass::default(),
        }
    }

    /// Wind, foliage, river, rain for this ear. `swell` slowly varies the wind's breath.
    fn sample(&mut self, levels: &Levels, swell: f32, rate: f32) -> f32 {
        let white = self.noise.next();

        // Wind: deep brown noise, opening up with the gusts.
        let cutoff = 180.0 + 520.0 * levels.wind;
        let air = self
            .wind
            .run(self.brown.run(white), coefficient(cutoff, rate));
        let wind = air * (0.012 + 0.16 * levels.wind * levels.wind) * swell;

        // Foliage: high noise (white minus its low part) with a crackling envelope.
        let high = white - self.leaf_high.run(white, coefficient(3000.0, rate));
        let spike = if self.noise.unit() < 0.003 { 1.0 } else { 0.0 };
        let crackle = self.leaf_envelope.run(spike * 5.0, coefficient(25.0, rate));
        let leaves = high * (0.15 + crackle) * 0.03 * levels.foliage * (0.3 + levels.wind);

        // River: a band of rolling noise.
        let rolling = self.river_brown.run(white) * 0.6 + white * 0.15;
        let low = self.river_low.run(rolling, coefficient(1200.0, rate));
        let band = low - self.river_high.run(low, coefficient(150.0, rate));
        let river = band * 0.35 * levels.water;

        // Rain: soft hiss and the patter of single drops.
        let hiss = white - self.rain_high.run(white, coefficient(1200.0, rate));
        let drop = if self.noise.unit() < 0.0015 * levels.rain {
            self.noise.next() * 2.5
        } else {
            0.0
        };
        let patter = self.patter.run(drop, coefficient(2500.0, rate));
        let rain = (hiss * 0.025 + patter * 0.2) * levels.rain;

        wind + leaves + river + rain
    }
}

/// Small reverb (Schroeder / Freeverb): parallel feedback delays (combs) build a dense tail of
/// echoes, all-pass filters smear them; slightly different lengths for each ear.
struct Reverb {
    combs: [Delay; 4],
    allpasses: [Delay; 2],
}

struct Delay {
    buffer: Vec<f32>,
    index: usize,
    /// Low-pass in the feedback: high echoes die sooner, like in a real place.
    damped: f32,
}

impl Delay {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
            damped: 0.0,
        }
    }

    fn comb(&mut self, x: f32, feedback: f32, damping: f32) -> f32 {
        let out = self.buffer[self.index];
        self.damped = out * (1.0 - damping) + self.damped * damping;
        self.buffer[self.index] = x + self.damped * feedback;
        self.index = (self.index + 1) % self.buffer.len();
        out
    }

    fn allpass(&mut self, x: f32) -> f32 {
        let delayed = self.buffer[self.index];
        let out = delayed - x;
        self.buffer[self.index] = x + delayed * 0.5;
        self.index = (self.index + 1) % self.buffer.len();
        out
    }
}

impl Reverb {
    fn new(rate: f32, spread: usize) -> Self {
        let scale = |n: usize| ((n + spread) as f32 * rate / 44_100.0) as usize;
        Self {
            combs: [1116, 1188, 1277, 1356].map(|n| Delay::new(scale(n))),
            allpasses: [556, 441].map(|n| Delay::new(scale(n))),
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let mut sum = 0.0;
        for c in &mut self.combs {
            sum += c.comb(x, 0.8, 0.35);
        }
        let mut out = sum * 0.25;
        for a in &mut self.allpasses {
            out = a.allpass(out);
        }
        out
    }
}

// ---------- Voices ----------

/// A bird call: a few notes, each a tone gliding from one pitch to another.
#[derive(Clone, Copy, Default)]
struct Call {
    active: bool,
    /// Seconds into the call.
    t: f32,
    notes: u32,
    note_length: f32,
    gap: f32,
    from: f32,
    to: f32,
    /// Vibrato depth (trills).
    warble: f32,
    phase: f32,
    loudness: f32,
    pan: f32,
}

/// A footstep: heel then toe, each a burst of filtered noise.
#[derive(Clone, Copy, Default)]
struct Step {
    active: bool,
    t: f32,
    surface: u32,
    strength: f32,
    low: f32,
    band_low: f32,
    band: f32,
    phase: f32,
}

/// A bubble in running water: a short tone rising in pitch.
#[derive(Clone, Copy, Default)]
struct Bubble {
    active: bool,
    t: f32,
    frequency: f32,
    length: f32,
    phase: f32,
    loudness: f32,
    pan: f32,
}

const CALLS: usize = 4;
const STEPS: usize = 4;
const BUBBLES: usize = 6;
/// Crickets, each with its own pitch, rhythm and place.
const CRICKETS: [(f32, f32, f32); 3] = [
    (4400.0, 0.62, -0.6),
    (4750.0, 0.81, 0.4),
    (4150.0, 0.47, 0.1),
];
/// Share of the birds, crickets and steps sent to the reverb.
const REVERB_SEND: f32 = 0.35;
/// Overall volume.
const MASTER: f32 = 0.8;

pub struct Soundscape {
    rate: f32,
    shared: Arc<Shared>,
    noise: Noise,
    steps_heard: u32,
    wind: Smooth,
    foliage: Smooth,
    water: Smooth,
    crickets: Smooth,
    rain: Smooth,
    beds: [Bed; 2],
    reverbs: [Reverb; 2],
    swell: f32,
    bubbles: [Bubble; BUBBLES],
    calls: [Call; CALLS],
    cricket_phase: [f32; 3],
    cricket_time: [f32; 3],
    footsteps: [Step; STEPS],
}

impl Soundscape {
    pub fn new(rate: f32, shared: Arc<Shared>, seed: u32) -> Self {
        Self {
            rate,
            shared,
            noise: Noise(seed | 1),
            steps_heard: 0,
            wind: Smooth::default(),
            foliage: Smooth::default(),
            water: Smooth::default(),
            crickets: Smooth::default(),
            rain: Smooth::default(),
            beds: [
                Bed::new(seed.wrapping_mul(0x9e37_79b9) ^ 0x51),
                Bed::new(seed.wrapping_mul(0x85eb_ca6b) ^ 0xa3),
            ],
            reverbs: [Reverb::new(rate, 0), Reverb::new(rate, 23)],
            swell: 0.0,
            bubbles: [Bubble::default(); BUBBLES],
            calls: [Call::default(); CALLS],
            cricket_phase: [0.0; 3],
            cricket_time: [0.0, 0.21, 0.37],
            footsteps: [Step::default(); STEPS],
        }
    }

    /// Fills an interleaved buffer of `channels` channels (the first two get stereo, others
    /// the mix).
    pub fn fill(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let scene = self.shared.scene();
        self.start_steps();
        // Random events are drawn once per buffer: their chance scales with its duration.
        let frames = out.len() / channels;
        let seconds = frames as f32 / self.rate;
        self.maybe_call(scene.birds, seconds);
        self.maybe_bubble(scene.water, seconds);

        for frame in out.chunks_mut(channels) {
            let [left, right] = self.sample(&scene);
            for (c, sample) in frame.iter_mut().enumerate() {
                *sample = match c {
                    0 => left,
                    1 => right,
                    _ => 0.5 * (left + right),
                };
            }
        }
    }

    fn start_steps(&mut self) {
        let count = self.shared.steps.load(Ordering::Acquire);
        while self.steps_heard != count {
            self.steps_heard = self.steps_heard.wrapping_add(1);
            let surface = self.shared.step_surface.load(Ordering::Relaxed);
            let strength = load(&self.shared.step_strength);
            if let Some(slot) = self.footsteps.iter_mut().find(|s| !s.active) {
                *slot = Step {
                    active: true,
                    surface,
                    strength,
                    ..Step::default()
                };
            }
        }
    }

    fn maybe_call(&mut self, activity: f32, seconds: f32) {
        // Up to about 1.2 calls per second in a full dawn chorus.
        if self.noise.unit() >= activity * 1.2 * seconds {
            return;
        }
        let Some(slot) = self.calls.iter_mut().find(|c| !c.active) else {
            return;
        };
        let n = &mut self.noise;
        // A few kinds of song: a descending whistle, a quick trill, a two-note call.
        let kind = (n.unit() * 3.0) as u32;
        let base = 2000.0 + 2000.0 * n.unit();
        *slot = match kind {
            0 => Call {
                notes: 1,
                note_length: 0.3 + 0.25 * n.unit(),
                gap: 0.0,
                from: base * 1.25,
                to: base * 0.85,
                warble: 0.01,
                ..Call::default()
            },
            1 => Call {
                notes: 5 + (n.unit() * 6.0) as u32,
                note_length: 0.05,
                gap: 0.035,
                from: base,
                to: base * 1.12,
                warble: 0.04,
                ..Call::default()
            },
            _ => Call {
                notes: 2,
                note_length: 0.16,
                gap: 0.1,
                from: base * 1.08,
                to: base * 0.96,
                warble: 0.0,
                ..Call::default()
            },
        };
        slot.active = true;
        slot.loudness = 0.012 + 0.025 * self.noise.unit();
        slot.pan = self.noise.next() * 0.8;
    }

    fn maybe_bubble(&mut self, water: f32, seconds: f32) {
        if self.noise.unit() >= water * 10.0 * seconds {
            return;
        }
        let Some(slot) = self.bubbles.iter_mut().find(|b| !b.active) else {
            return;
        };
        let n = &mut self.noise;
        *slot = Bubble {
            active: true,
            t: 0.0,
            frequency: 350.0 + 600.0 * n.unit(),
            length: 0.02 + 0.04 * n.unit(),
            phase: 0.0,
            loudness: 0.008 + 0.015 * n.unit(),
            pan: n.next() * 0.7,
        };
    }

    fn sample(&mut self, scene: &Scene) -> [f32; 2] {
        let dt = 1.0 / self.rate;
        let glide = coefficient(0.5, self.rate);
        let levels = Levels {
            wind: self.wind.towards(scene.wind, glide),
            foliage: self.foliage.towards(scene.foliage, glide),
            water: self.water.towards(scene.water, glide),
            crickets: self.crickets.towards(scene.crickets, glide),
            rain: self.rain.towards(scene.rain, glide),
        };
        // Slow breathing of the wind, a little different in each ear.
        self.swell += dt;
        let swell_left = 0.8 + 0.2 * (self.swell * 0.23).sin();
        let swell_right = 0.8 + 0.2 * (self.swell * 0.19 + 1.3).sin();
        let mut left = self.beds[0].sample(&levels, swell_left, self.rate);
        let mut right = self.beds[1].sample(&levels, swell_right, self.rate);

        // Sounds placed in space: dry part panned, a share sent to the reverb.
        let mut send = [0.0f32; 2];
        let mut place = |s: f32, pan: f32, left: &mut f32, right: &mut f32| {
            let l = s * (1.0 - pan) * 0.5;
            let r = s * (1.0 + pan) * 0.5;
            *left += l;
            *right += r;
            send[0] += l * REVERB_SEND;
            send[1] += r * REVERB_SEND;
        };

        for b in &mut self.bubbles {
            if !b.active {
                continue;
            }
            b.t += dt;
            let k = b.t / b.length;
            if k >= 1.0 {
                b.active = false;
                continue;
            }
            // Pitch rises as the bubble closes: a "bloop".
            b.phase = (b.phase + b.frequency * (1.0 + 1.5 * k) * dt).fract();
            let s = (b.phase * TAU).sin() * (1.0 - k) * (k * 20.0).min(1.0) * b.loudness;
            place(s * levels.water, b.pan, &mut left, &mut right);
        }

        for c in &mut self.calls {
            if !c.active {
                continue;
            }
            c.t += dt;
            let period = c.note_length + c.gap;
            let note = (c.t / period) as u32;
            if note >= c.notes {
                c.active = false;
                continue;
            }
            let within = c.t - note as f32 * period;
            if within > c.note_length {
                continue;
            }
            let k = within / c.note_length;
            let frequency =
                (c.from + (c.to - c.from) * k) * (1.0 + c.warble * (k * TAU * 4.0).sin());
            c.phase = (c.phase + frequency * dt).fract();
            // A voice is not a pure tone: a few soft harmonics. Quick attack, gentle release.
            let p = c.phase * TAU;
            let voice = p.sin() + 0.22 * (2.0 * p).sin() + 0.08 * (3.0 * p).sin();
            let envelope = (k * 12.0).min(1.0) * (1.0 - k).powf(1.5);
            place(voice * envelope * c.loudness, c.pan, &mut left, &mut right);
        }

        if levels.crickets > 1e-3 {
            for (i, &(pitch, period, pan)) in CRICKETS.iter().enumerate() {
                self.cricket_time[i] = (self.cricket_time[i] + dt) % period;
                self.cricket_phase[i] = (self.cricket_phase[i] + pitch * dt).fract();
                let t = self.cricket_time[i];
                // Three quick pulses, then silence.
                let pulse = if t < 0.09 {
                    ((t / 0.03).fract() * PI).sin()
                } else {
                    0.0
                };
                let s = (self.cricket_phase[i] * TAU).sin() * pulse * 0.006 * levels.crickets;
                place(s, pan, &mut left, &mut right);
            }
        }

        for s in &mut self.footsteps {
            if !s.active {
                continue;
            }
            s.t += dt;
            // (length of each burst, low-pass cutoff, ringing band, share of ringing)
            let (length, cutoff, centre, ring) = match s.surface {
                0 => (0.07, 1600.0, 2400.0, 0.2),  // grass: soft swish
                1 => (0.06, 900.0, 1400.0, 0.4),   // sand: dull crunch
                2 => (0.035, 4000.0, 1800.0, 0.8), // stone: short tap
                _ => (0.12, 800.0, 600.0, 0.5),    // water: splash
            };
            // Heel, then toe a little later and softer.
            let toe_at = 0.045;
            if s.t >= toe_at + length {
                s.active = false;
                continue;
            }
            let burst = |t: f32| {
                if (0.0..length).contains(&t) {
                    let k = t / length;
                    (k * 30.0).min(1.0) * (1.0 - k).powi(2)
                } else {
                    0.0
                }
            };
            let envelope = burst(s.t) + 0.6 * burst(s.t - toe_at);
            let n = self.noise.next();
            s.low += coefficient(cutoff, self.rate) * (n - s.low);
            let f = 2.0 * (PI * centre / self.rate).sin();
            s.band_low += f * s.band;
            let high = s.low - s.band_low - 0.6 * s.band;
            s.band += f * high;
            let mut v = (s.low * (1.0 - ring) + s.band * ring) * envelope;
            if s.surface == Surface::Water as u32 {
                let k = (s.t / (toe_at + length)).min(1.0);
                s.phase = (s.phase + (160.0 + 200.0 * k) * dt).fract();
                v += (s.phase * TAU).sin() * envelope * 0.4;
            }
            place(v * 0.35 * s.strength, 0.0, &mut left, &mut right);
        }

        left += self.reverbs[0].run(send[0]);
        right += self.reverbs[1].run(send[1]);
        // Gentle limiter: never clips, however many sounds add up.
        [soft_clip(left * MASTER), soft_clip(right * MASTER)]
    }
}

fn soft_clip(x: f32) -> f32 {
    x.tanh()
}

// ---------- Listening tests ----------

/// Renders `seconds` of `scene` into interleaved stereo samples, with a footstep on
/// `steps` every half second if given.
pub fn render(scene: &Scene, steps: Option<Surface>, seconds: f32, rate: f32) -> Vec<f32> {
    let shared = Arc::new(Shared::default());
    shared.set_scene(scene);
    let mut sound = Soundscape::new(rate, shared.clone(), 7);
    let frames = (seconds * rate) as usize;
    let mut out = vec![0.0; frames * 2];
    let block = 512;
    let mut next_step = 0.3;
    for (k, chunk) in out.chunks_mut(block * 2).enumerate() {
        let t = (k * block) as f32 / rate;
        if let Some(surface) = steps
            && t >= next_step
        {
            shared.step(surface, 0.6);
            next_step += 0.5;
        }
        sound.fill(chunk, 2);
    }
    out
}

/// Writes interleaved stereo samples as a 16-bit WAV file.
pub fn write_wav(path: &std::path::Path, samples: &[f32], rate: u32) -> std::io::Result<()> {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&2u16.to_le_bytes()); // stereo
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 4).to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(path, bytes)
}

/// One file per kind of sound, to listen to them one at a time.
pub fn demo(dir: &std::path::Path) -> std::io::Result<Vec<String>> {
    std::fs::create_dir_all(dir)?;
    let quiet = Scene::default();
    let cases: [(&str, Scene, Option<Surface>); 9] = [
        ("1-vent-leger", Scene { wind: 0.4, ..quiet }, None),
        ("2-vent-fort", Scene { wind: 1.0, ..quiet }, None),
        (
            "3-foret-jour",
            Scene {
                wind: 0.5,
                foliage: 1.0,
                birds: 0.6,
                ..quiet
            },
            None,
        ),
        (
            "4-aube-oiseaux",
            Scene {
                wind: 0.3,
                foliage: 0.6,
                birds: 1.0,
                ..quiet
            },
            None,
        ),
        (
            "5-riviere",
            Scene {
                wind: 0.3,
                water: 1.0,
                ..quiet
            },
            None,
        ),
        (
            "6-nuit-grillons",
            Scene {
                wind: 0.2,
                crickets: 1.0,
                ..quiet
            },
            None,
        ),
        (
            "7-pluie",
            Scene {
                wind: 0.5,
                rain: 1.0,
                ..quiet
            },
            None,
        ),
        (
            "8-pas-herbe",
            Scene { wind: 0.3, ..quiet },
            Some(Surface::Grass),
        ),
        (
            "9-pas-eau",
            Scene {
                wind: 0.3,
                water: 0.5,
                ..quiet
            },
            Some(Surface::Water),
        ),
    ];
    let rate = 48_000;
    let mut written = Vec::new();
    for (name, scene, steps) in cases {
        let samples = render(&scene, steps, 8.0, rate as f32);
        let path = dir.join(format!("{name}.wav"));
        write_wav(&path, &samples, rate)?;
        written.push(path.display().to_string());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn a_full_scene_stays_in_range_and_makes_sound() {
        let scene = Scene {
            wind: 1.0,
            foliage: 1.0,
            water: 1.0,
            birds: 1.0,
            crickets: 1.0,
            rain: 1.0,
        };
        let out = render(&scene, Some(Surface::Stone), 3.0, 48_000.0);
        assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert!(rms(&out) > 0.01, "rms {}", rms(&out));
    }

    #[test]
    fn wind_makes_it_louder_but_stays_a_background() {
        let calm = rms(&render(&Scene::default(), None, 2.0, 48_000.0)[96_000..]);
        let windy = Scene {
            wind: 1.0,
            ..Scene::default()
        };
        let loud = rms(&render(&windy, None, 3.0, 48_000.0)[96_000..]);
        assert!(loud > 2.0 * calm, "calm {calm}, windy {loud}");
        assert!(loud < 0.15, "too loud: {loud}");
    }

    #[test]
    fn left_and_right_differ_so_the_sound_has_width() {
        let windy = Scene {
            wind: 0.8,
            ..Scene::default()
        };
        let out = render(&windy, None, 2.0, 48_000.0);
        let difference: f32 =
            out.chunks(2).map(|f| (f[0] - f[1]).abs()).sum::<f32>() / (out.len() / 2) as f32;
        assert!(difference > 1e-3);
    }

    #[test]
    fn footsteps_stand_out_of_the_background() {
        let peak = |v: &[f32]| v.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let quiet = peak(&render(&Scene::default(), None, 2.0, 48_000.0));
        let steps = peak(&render(
            &Scene::default(),
            Some(Surface::Grass),
            2.0,
            48_000.0,
        ));
        assert!(steps > quiet * 3.0, "{quiet} → {steps}");
        assert!(steps < 0.3, "steps too loud: {steps}");
    }
}

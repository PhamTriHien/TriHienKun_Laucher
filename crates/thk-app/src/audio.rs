use rodio::buffer::SamplesBuffer;
use rodio::source::Source;
use rodio::{Decoder, DeviceSinkBuilder, Player};
use std::f32::consts::{FRAC_1_SQRT_2, PI, TAU};
use std::fs::File;
use std::io::BufReader;
use std::num::{NonZeroU16, NonZeroU32};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

const SAMPLE_RATE_HZ: u32 = 44_100;
const SAMPLE_RATE: f32 = SAMPLE_RATE_HZ as f32;
const BGM_BEAT_SEC: f32 = 0.90;
const BGM_TOTAL_BEATS: usize = 64;
const BGM_TOTAL_FRAMES: usize = 2_540_160;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundEffect {
    Click,
    Success,
    Launch,
    Error,
}

enum AudioCommand {
    PlaySfx(SoundEffect),
    ConfigureMusic {
        enabled: bool,
        volume: f32,
        custom_file: Option<PathBuf>,
    },
    Shutdown,
}

#[derive(Clone)]
pub struct AudioEngine {
    tx: mpsc::Sender<AudioCommand>,
}

impl AudioEngine {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<AudioCommand>();
        thread::Builder::new()
            .name("thk-audio-worker".into())
            .spawn(move || audio_worker_loop(rx))
            .ok();
        Self { tx }
    }

    pub fn play_sfx(&self, enabled: bool, effect: SoundEffect) {
        if enabled {
            let _ = self.tx.send(AudioCommand::PlaySfx(effect));
        }
    }

    pub fn sync_music(&self, enabled: bool, volume: f32, custom_file: Option<PathBuf>) {
        let _ = self.tx.send(AudioCommand::ConfigureMusic {
            enabled,
            volume: volume.clamp(0.0, 1.0),
            custom_file,
        });
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(AudioCommand::Shutdown);
    }
}

fn audio_worker_loop(rx: mpsc::Receiver<AudioCommand>) {
    let Ok(sink_handle) = DeviceSinkBuilder::open_default_sink() else {
        while let Ok(cmd) = rx.recv() {
            if matches!(cmd, AudioCommand::Shutdown) {
                break;
            }
        }
        return;
    };

    let sfx_player = Player::connect_new(sink_handle.mixer());
    let music_player = Player::connect_new(sink_handle.mixer());
    let mut active_music_path: Option<PathBuf> = None;
    let mut active_ambient = false;

    while let Ok(cmd) = rx.recv() {
        match cmd {
            AudioCommand::Shutdown => {
                sfx_player.stop();
                music_player.stop();
                break;
            }
            AudioCommand::PlaySfx(kind) => {
                let pcm = synthesize_sfx_samples(kind);
                let channels = NonZeroU16::new(1).expect("non-zero channel");
                let sample_rate = NonZeroU32::new(SAMPLE_RATE_HZ).expect("non-zero sample rate");
                let buf = SamplesBuffer::new(channels, sample_rate, pcm);
                sfx_player.append(buf);
            }
            AudioCommand::ConfigureMusic {
                enabled,
                volume,
                custom_file,
            } => {
                music_player.set_volume(volume);
                if !enabled {
                    music_player.stop();
                    active_music_path = None;
                    active_ambient = false;
                    continue;
                }

                if let Some(path) = custom_file.filter(|p| p.is_file()) {
                    if active_music_path.as_ref() != Some(&path) || music_player.empty() {
                        music_player.stop();
                        if let Ok(file) = File::open(&path) {
                            if let Ok(decoder) = Decoder::try_from(BufReader::new(file)) {
                                music_player.append(decoder.repeat_infinite());
                                active_music_path = Some(path);
                                active_ambient = false;
                            }
                        }
                    }
                } else if !active_ambient || music_player.empty() {
                    music_player.stop();
                    let stereo_bgm = synthesize_ambient_loop();
                    let channels = NonZeroU16::new(2).expect("stereo channels");
                    let sample_rate =
                        NonZeroU32::new(SAMPLE_RATE_HZ).expect("non-zero sample rate");
                    let buf = SamplesBuffer::new(channels, sample_rate, stereo_bgm);
                    music_player.append(buf.repeat_infinite());
                    active_music_path = None;
                    active_ambient = true;
                }
            }
        }
    }
}

pub(crate) fn synthesize_sfx_samples(effect: SoundEffect) -> Vec<f32> {
    match effect {
        SoundEffect::Click => synth_notes(&[(880.0, 0.032, 0.14)], SAMPLE_RATE),
        SoundEffect::Success => {
            synth_notes(&[(587.33, 0.075, 0.16), (880.0, 0.12, 0.18)], SAMPLE_RATE)
        }
        SoundEffect::Launch => synth_notes(
            &[
                (440.0, 0.07, 0.16),
                (554.37, 0.07, 0.16),
                (659.25, 0.08, 0.17),
                (880.0, 0.18, 0.20),
            ],
            SAMPLE_RATE,
        ),
        SoundEffect::Error => {
            synth_notes(&[(311.13, 0.09, 0.20), (233.08, 0.16, 0.20)], SAMPLE_RATE)
        }
    }
}

fn synth_notes(notes: &[(f32, f32, f32)], sr: f32) -> Vec<f32> {
    let total_samples: usize = notes.iter().map(|(_, dur, _)| (dur * sr) as usize).sum();
    let mut out = Vec::with_capacity(total_samples);
    for &(freq, dur, amp) in notes {
        let count = (dur * sr) as usize;
        for i in 0..count {
            let t = i as f32 / sr;
            let attack = (t / 0.004).clamp(0.0, 1.0);
            let decay = (-t * (4.5 / dur.max(0.02))).exp();
            let fundamental = (TAU * freq * t).sin();
            let overtone = 0.25 * (TAU * freq * 2.0 * t).sin();
            out.push((fundamental + overtone) * amp * attack * decay);
        }
    }
    out
}

#[derive(Clone, Copy)]
struct Phasor {
    re: f32,
    im: f32,
    step_re: f32,
    step_im: f32,
}

impl Phasor {
    fn new(freq_hz: f32, sr: f32, initial_phase: f32) -> Self {
        let w = TAU * freq_hz / sr;
        Self {
            re: initial_phase.cos(),
            im: initial_phase.sin(),
            step_re: w.cos(),
            step_im: w.sin(),
        }
    }

    #[inline]
    fn next_sin(&mut self) -> f32 {
        let out = self.im;
        let next_re = self.re * self.step_re - self.im * self.step_im;
        let next_im = self.re * self.step_im + self.im * self.step_re;
        self.re = next_re;
        self.im = next_im;
        out
    }

    #[inline]
    fn renormalize(&mut self) {
        let mag_sq = self.re * self.re + self.im * self.im;
        if mag_sq > 1e-12 {
            let inv = mag_sq.sqrt().recip();
            self.re *= inv;
            self.im *= inv;
        }
    }
}

#[inline]
fn midi_to_hz(midi: u8) -> f32 {
    440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0)
}

#[inline]
fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * PI;
    (angle.cos(), angle.sin())
}

#[inline]
fn smooth_cosine_ramp(x: f32) -> f32 {
    0.5 * (1.0 - (PI * x.clamp(0.0, 1.0)).cos())
}

pub(crate) fn synthesize_ambient_loop() -> Vec<f32> {
    debug_assert_eq!(
        (BGM_TOTAL_BEATS as f32 * BGM_BEAT_SEC * SAMPLE_RATE).round() as usize,
        BGM_TOTAL_FRAMES
    );
    let mut dry = vec![0.0_f32; BGM_TOTAL_FRAMES * 2];

    // 1. Warm breathing analog pad progression (16 bars in F Lydian / C Major / A Aeolian)
    let pad_chords: &[(f32, f32, &[u8], f32)] = &[
        (0.0, 4.0, &[53, 57, 60, 64], 0.105),  // Fmaj7
        (4.0, 4.0, &[52, 55, 59, 62], 0.100),  // Em7
        (8.0, 4.0, &[50, 53, 57, 64], 0.105),  // Dm9
        (12.0, 4.0, &[48, 52, 55, 62], 0.100), // Cmaj9
        (16.0, 4.0, &[53, 57, 60, 64], 0.115), // Fmaj7#11
        (20.0, 4.0, &[55, 59, 62, 64], 0.110), // G6add9
        (24.0, 4.0, &[57, 60, 64, 67], 0.115), // Am9
        (28.0, 4.0, &[52, 55, 59, 62], 0.105), // Em7
        (32.0, 4.0, &[50, 53, 57, 60], 0.120), // Dm7
        (36.0, 4.0, &[48, 53, 57, 64], 0.115), // Fmaj7/C
        (40.0, 4.0, &[45, 52, 55, 60], 0.120), // Am7
        (44.0, 4.0, &[53, 57, 62, 67], 0.115), // F6/9
        (48.0, 4.0, &[52, 55, 59, 62], 0.105), // Cmaj9/E
        (52.0, 4.0, &[50, 53, 57, 64], 0.105), // Dm9
        (56.0, 4.0, &[55, 60, 62, 65], 0.100), // G9sus4
        (60.0, 4.0, &[48, 55, 59, 64], 0.100), // Cmaj7 -> resolves into Fmaj7
    ];
    for &(beat, dur, notes, amp) in pad_chords {
        render_warm_pad_chord(&mut dry, beat, dur, notes, amp);
    }

    // 2. Deep gentle sub-bass foundation
    let bass_notes: &[(f32, f32, u8, f32)] = &[
        (0.0, 3.8, 41, 0.145),
        (4.0, 3.8, 40, 0.140),
        (8.0, 3.8, 38, 0.145),
        (12.0, 3.8, 36, 0.140),
        (16.0, 3.8, 41, 0.155),
        (20.0, 3.8, 43, 0.150),
        (24.0, 3.8, 45, 0.150),
        (28.0, 3.8, 40, 0.145),
        (32.0, 3.8, 38, 0.160),
        (36.0, 1.9, 36, 0.155),
        (38.0, 1.9, 35, 0.150),
        (40.0, 3.8, 33, 0.160),
        (44.0, 3.8, 41, 0.155),
        (48.0, 3.8, 40, 0.145),
        (52.0, 3.8, 38, 0.145),
        (56.0, 3.8, 43, 0.140),
        (60.0, 3.6, 36, 0.140),
    ];
    for &(beat, dur, midi, amp) in bass_notes {
        render_sub_bass_note(&mut dry, beat, dur, midi, amp);
    }

    // 3. Intimate felt upright piano (rolled voicings + lyrical nostalgic motif)
    let piano_score: &[(f32, f32, u8, f32)] = &[
        // Phrase I: Morning Mist
        (0.00, 3.6, 41, 0.42),
        (0.06, 3.5, 48, 0.40),
        (0.12, 3.4, 57, 0.44),
        (0.18, 3.2, 64, 0.48),
        (1.52, 2.2, 67, 0.52),
        (2.50, 1.8, 69, 0.55),
        (3.26, 1.6, 64, 0.44),
        // Bar 1 (Cmaj7/E)
        (4.00, 3.5, 40, 0.40),
        (4.06, 3.4, 47, 0.39),
        (4.12, 3.3, 55, 0.43),
        (5.02, 2.2, 67, 0.50),
        (6.00, 2.0, 71, 0.56),
        (7.02, 1.8, 69, 0.48),
        // Bar 2 (Dm9)
        (8.00, 3.6, 38, 0.42),
        (8.06, 3.5, 45, 0.40),
        (8.12, 3.4, 53, 0.44),
        (8.18, 3.2, 60, 0.46),
        (9.02, 2.1, 64, 0.52),
        (10.00, 2.0, 69, 0.56),
        (11.02, 1.8, 67, 0.48),
        // Bar 3 (Cmaj9)
        (12.00, 3.6, 36, 0.42),
        (12.07, 3.5, 43, 0.40),
        (12.14, 3.4, 52, 0.44),
        (12.52, 2.0, 62, 0.48),
        (13.50, 2.0, 64, 0.50),
        (14.52, 2.4, 67, 0.53),
        // Phrase II: Sunlight Through Birch Leaves (Lydian theme)
        // Bar 4 (Fmaj7#11)
        (16.00, 3.5, 41, 0.44),
        (16.50, 3.0, 48, 0.40),
        (17.00, 2.6, 57, 0.44),
        (16.05, 2.2, 69, 0.60),
        (17.52, 1.8, 71, 0.58),
        (18.50, 1.8, 72, 0.62),
        (19.26, 2.0, 76, 0.64),
        // Bar 5 (G6add9)
        (20.00, 3.5, 43, 0.44),
        (20.50, 3.0, 50, 0.40),
        (21.00, 2.6, 59, 0.43),
        (20.04, 2.2, 74, 0.62),
        (21.52, 1.8, 71, 0.56),
        (22.50, 1.8, 69, 0.54),
        (23.26, 2.0, 67, 0.50),
        // Bar 6 (Am9)
        (24.00, 3.5, 45, 0.45),
        (24.50, 3.0, 52, 0.41),
        (25.00, 2.6, 59, 0.44),
        (24.04, 2.2, 72, 0.60),
        (25.52, 1.8, 71, 0.55),
        (26.50, 1.8, 67, 0.52),
        (27.50, 1.8, 64, 0.48),
        // Bar 7 (Em7 lift)
        (28.00, 3.5, 40, 0.42),
        (28.50, 3.0, 47, 0.39),
        (29.00, 2.6, 55, 0.42),
        (28.52, 1.8, 67, 0.50),
        (29.50, 1.8, 69, 0.54),
        (30.52, 2.2, 71, 0.58),
        // Phrase III: Amethyst Geode & Starry Sky
        // Bar 8 (Dm11)
        (32.00, 3.6, 38, 0.48),
        (32.50, 3.0, 45, 0.43),
        (33.00, 2.6, 53, 0.45),
        (32.04, 2.2, 77, 0.68),
        (33.52, 1.8, 76, 0.64),
        (34.50, 1.8, 72, 0.60),
        (35.26, 2.0, 69, 0.56),
        // Bar 9 (Fmaj9/C -> G/B)
        (36.00, 2.0, 48, 0.44),
        (36.50, 1.8, 57, 0.42),
        (38.00, 2.0, 47, 0.44),
        (38.50, 1.8, 55, 0.42),
        (36.04, 1.6, 67, 0.55),
        (37.02, 1.6, 69, 0.58),
        (38.04, 1.6, 71, 0.60),
        (39.02, 1.8, 74, 0.64),
        // Bar 10 (Am11)
        (40.00, 3.6, 33, 0.46),
        (40.50, 3.0, 40, 0.42),
        (41.00, 2.6, 48, 0.44),
        (40.04, 2.2, 76, 0.66),
        (41.52, 1.8, 74, 0.62),
        (42.50, 1.8, 72, 0.58),
        (43.26, 2.0, 67, 0.54),
        // Bar 11 (F6/9)
        (44.00, 3.6, 41, 0.46),
        (44.50, 3.0, 48, 0.42),
        (45.00, 2.6, 57, 0.44),
        (44.04, 2.0, 69, 0.58),
        (45.52, 1.8, 72, 0.60),
        (46.50, 1.8, 74, 0.62),
        (47.26, 2.0, 76, 0.60),
        // Phrase IV: Campfire Embers (Reflective Dusk Resolution)
        // Bar 12 (Cmaj9/E)
        (48.00, 3.6, 40, 0.42),
        (48.06, 3.4, 48, 0.40),
        (48.12, 3.2, 55, 0.42),
        (48.52, 2.2, 72, 0.55),
        (50.02, 2.0, 71, 0.52),
        (51.02, 2.0, 67, 0.48),
        // Bar 13 (Dm9)
        (52.00, 3.6, 38, 0.42),
        (52.06, 3.4, 45, 0.40),
        (52.12, 3.2, 53, 0.42),
        (52.52, 2.2, 69, 0.52),
        (54.02, 2.0, 64, 0.48),
        (55.02, 2.0, 65, 0.46),
        // Bar 14 (G9sus4 -> Gadd9)
        (56.00, 3.6, 43, 0.40),
        (56.07, 3.4, 50, 0.39),
        (56.52, 2.0, 60, 0.46),
        (58.00, 2.0, 59, 0.45),
        (58.52, 2.0, 62, 0.48),
        (59.26, 2.2, 67, 0.50),
        // Bar 15 (Cmaj9 open pivot -> E4 common-tone rings smoothly into Bar 0 Fmaj9)
        (60.00, 3.6, 36, 0.40),
        (60.07, 3.5, 43, 0.38),
        (60.14, 3.4, 52, 0.42),
        (60.22, 3.2, 62, 0.45),
        (61.50, 3.2, 64, 0.48),
    ];
    for &(beat, dur, midi, vel) in piano_score {
        render_felt_piano_note(&mut dry, beat, dur, midi, vel);
    }

    // 4. Sparse crystalline celesta / amethyst chimes
    let celesta_notes: &[(f32, u8, f32, f32)] = &[
        (19.50, 76, 0.34, 0.32),
        (26.00, 76, 0.32, -0.28),
        (27.00, 79, 0.30, 0.34),
        (32.50, 69, 0.30, -0.35),
        (33.00, 72, 0.32, -0.12),
        (34.00, 76, 0.35, 0.18),
        (35.00, 79, 0.36, 0.38),
        (41.00, 81, 0.34, 0.30),
        (42.00, 76, 0.32, -0.24),
        (43.00, 74, 0.30, 0.16),
        (46.00, 79, 0.33, -0.30),
        (47.50, 72, 0.28, 0.25),
        (57.50, 74, 0.28, -0.22),
        (59.00, 79, 0.26, 0.28),
    ];
    for &(beat, midi, vel, pan) in celesta_notes {
        render_celesta_note(&mut dry, beat, midi, vel, pan);
    }

    // 5. Circular ping-pong tape delay + stereo concert hall reverb + soft mastering
    apply_circular_echo_and_hall_reverb(&dry)
}

fn render_felt_piano_note(
    buf: &mut [f32],
    start_beat: f32,
    dur_beats: f32,
    midi: u8,
    velocity: f32,
) {
    let start_frame =
        ((start_beat * BGM_BEAT_SEC * SAMPLE_RATE).round() as usize) % BGM_TOTAL_FRAMES;
    let total_sec = (dur_beats * BGM_BEAT_SEC + 2.2).min(8.2);
    let note_frames = (total_sec * SAMPLE_RATE) as usize;
    if note_frames == 0 {
        return;
    }

    let f0 = midi_to_hz(midi);
    let pan = ((midi as f32 - 60.0) / 30.0).clamp(-0.36, 0.36);
    let (gain_l, gain_r) = pan_gains(pan);
    let vel = velocity.clamp(0.1, 1.0);
    let base_amp = 0.21 * vel * vel.sqrt();
    let inharm = 0.00018_f32;

    let mut partials: [(Phasor, f32, f32, f32); 8] =
        [(Phasor::new(0.0, SAMPLE_RATE, 0.0), 0.0, 1.0, 1.0); 8];
    let mut partial_count = 0usize;

    for k in 1..=6u32 {
        let kf = k as f32;
        let fk = f0 * kf * (1.0 + inharm * kf * kf).sqrt();
        if fk >= 18_000.0 || partial_count >= partials.len() {
            break;
        }
        let weight = kf.powf(-1.72) * (-0.44 * (1.0 - vel) * (kf - 1.0)).exp();
        let tau = (dur_beats.max(1.2) * BGM_BEAT_SEC * 1.35) / (1.0 + 0.42 * (kf - 1.0));
        let decay_step = (-1.0 / (tau * SAMPLE_RATE)).exp();
        let phase_seed = (midi as f32 * 0.618 + kf * 1.13) % TAU;
        partials[partial_count] = (
            Phasor::new(fk, SAMPLE_RATE, phase_seed),
            weight,
            1.0,
            decay_step,
        );
        partial_count += 1;

        if midi >= 50 && k <= 2 && partial_count < partials.len() {
            let detuned_fk = fk * 1.00052;
            partials[partial_count] = (
                Phasor::new(detuned_fk, SAMPLE_RATE, phase_seed + 0.9),
                weight * 0.55,
                1.0,
                decay_step,
            );
            partial_count += 1;
        }
    }

    let mut hammer_osc = Phasor::new(138.0, SAMPLE_RATE, 0.0);
    let mut hammer_env = 0.038 * vel;
    let hammer_decay = (-1.0 / (0.006 * SAMPLE_RATE)).exp();

    let attack_frames = (0.011 * SAMPLE_RATE) as usize;
    let release_frames = (1.05 * SAMPLE_RATE) as usize;
    let release_start = note_frames.saturating_sub(release_frames);

    for i in 0..note_frames {
        if i > 0 && (i & 1023) == 0 {
            for p in &mut partials[..partial_count] {
                p.0.renormalize();
            }
            hammer_osc.renormalize();
        }

        let attack = if i < attack_frames {
            smooth_cosine_ramp(i as f32 / attack_frames.max(1) as f32)
        } else {
            1.0
        };
        let damper = if i >= release_start {
            let rel_pos = (i - release_start) as f32 / release_frames.max(1) as f32;
            1.0 - smooth_cosine_ramp(rel_pos)
        } else {
            1.0
        };

        let mut sig = hammer_osc.next_sin() * hammer_env;
        hammer_env *= hammer_decay;

        for p in &mut partials[..partial_count] {
            sig += p.0.next_sin() * (p.1 * p.2);
            p.2 *= p.3;
        }

        let sample = sig * base_amp * attack * damper;
        let frame = (start_frame + i) % BGM_TOTAL_FRAMES;
        let idx = frame * 2;
        buf[idx] += sample * gain_l;
        buf[idx + 1] += sample * gain_r;
    }
}

fn render_warm_pad_chord(buf: &mut [f32], start_beat: f32, dur_beats: f32, notes: &[u8], amp: f32) {
    if notes.is_empty() {
        return;
    }
    let start_frame =
        ((start_beat * BGM_BEAT_SEC * SAMPLE_RATE).round() as usize) % BGM_TOTAL_FRAMES;
    let total_sec = dur_beats * BGM_BEAT_SEC + 1.35;
    let total_frames = (total_sec * SAMPLE_RATE) as usize;
    let fade_in_frames = (1.35 * SAMPLE_RATE) as usize;
    let fade_out_frames = (1.45 * SAMPLE_RATE) as usize;
    let fade_out_start = total_frames.saturating_sub(fade_out_frames);
    let voice_scale = amp / (notes.len() as f32).sqrt();

    for (n_idx, &midi) in notes.iter().enumerate() {
        let f0 = midi_to_hz(midi);
        if f0 >= 6_000.0 {
            continue;
        }
        let mut osc_l1 = Phasor::new(f0 * 0.9980, SAMPLE_RATE, n_idx as f32 * 0.8);
        let mut osc_l2 = Phasor::new(f0 * 2.0 * 0.9984, SAMPLE_RATE, n_idx as f32 * 1.5);
        let mut osc_r1 = Phasor::new(f0 * 1.0020, SAMPLE_RATE, n_idx as f32 * 1.2 + 0.7);
        let mut osc_r2 = Phasor::new(f0 * 2.0 * 1.0016, SAMPLE_RATE, n_idx as f32 * 2.1 + 0.4);
        let mut lfo = Phasor::new(0.16 + 0.03 * n_idx as f32, SAMPLE_RATE, n_idx as f32);

        for i in 0..total_frames {
            if i > 0 && (i & 1023) == 0 {
                osc_l1.renormalize();
                osc_l2.renormalize();
                osc_r1.renormalize();
                osc_r2.renormalize();
                lfo.renormalize();
            }

            let env_in = if i < fade_in_frames {
                smooth_cosine_ramp(i as f32 / fade_in_frames.max(1) as f32)
            } else {
                1.0
            };
            let env_out = if i >= fade_out_start {
                1.0 - smooth_cosine_ramp(
                    (i - fade_out_start) as f32 / fade_out_frames.max(1) as f32,
                )
            } else {
                1.0
            };
            let breath = 0.84 + 0.16 * lfo.next_sin();
            let env = env_in * env_out * breath * voice_scale;

            let sig_l = (osc_l1.next_sin() + 0.24 * osc_l2.next_sin()) * env;
            let sig_r = (osc_r1.next_sin() + 0.24 * osc_r2.next_sin()) * env;

            let frame = (start_frame + i) % BGM_TOTAL_FRAMES;
            let idx = frame * 2;
            buf[idx] += sig_l * 0.82 + sig_r * 0.18;
            buf[idx + 1] += sig_r * 0.82 + sig_l * 0.18;
        }
    }
}

fn render_celesta_note(buf: &mut [f32], start_beat: f32, midi: u8, velocity: f32, pan: f32) {
    let start_frame =
        ((start_beat * BGM_BEAT_SEC * SAMPLE_RATE).round() as usize) % BGM_TOTAL_FRAMES;
    let total_frames = (2.7 * SAMPLE_RATE) as usize;
    let attack_frames = (0.0045 * SAMPLE_RATE) as usize;
    let release_frames = (0.65 * SAMPLE_RATE) as usize;
    let release_start = total_frames.saturating_sub(release_frames);

    let f0 = midi_to_hz(midi);
    let (gain_l, gain_r) = pan_gains(pan);
    let base_amp = 0.115 * velocity.clamp(0.05, 1.0);

    let mut osc1 = Phasor::new(f0, SAMPLE_RATE, 0.0);
    let mut osc2 = Phasor::new((f0 * 2.0).min(18_500.0), SAMPLE_RATE, 0.5);
    let tine_freq = (f0 * 5.38).min(18_500.0);
    let mut osc_tine = Phasor::new(tine_freq, SAMPLE_RATE, 1.1);

    let mut env1 = 1.0_f32;
    let mut env2 = 0.28_f32;
    let mut env_tine = if f0 * 5.38 < 18_500.0 { 0.085_f32 } else { 0.0 };
    let dec1 = (-1.0 / (1.35 * SAMPLE_RATE)).exp();
    let dec2 = (-1.0 / (0.62 * SAMPLE_RATE)).exp();
    let dec_tine = (-1.0 / (0.055 * SAMPLE_RATE)).exp();

    for i in 0..total_frames {
        if i > 0 && (i & 1023) == 0 {
            osc1.renormalize();
            osc2.renormalize();
            osc_tine.renormalize();
        }
        let attack = if i < attack_frames {
            smooth_cosine_ramp(i as f32 / attack_frames.max(1) as f32)
        } else {
            1.0
        };
        let tail = if i >= release_start {
            1.0 - smooth_cosine_ramp((i - release_start) as f32 / release_frames.max(1) as f32)
        } else {
            1.0
        };

        let sig = osc1.next_sin() * env1 + osc2.next_sin() * env2 + osc_tine.next_sin() * env_tine;
        env1 *= dec1;
        env2 *= dec2;
        env_tine *= dec_tine;

        let sample = sig * base_amp * attack * tail;
        let frame = (start_frame + i) % BGM_TOTAL_FRAMES;
        let idx = frame * 2;
        buf[idx] += sample * gain_l;
        buf[idx + 1] += sample * gain_r;
    }
}

fn render_sub_bass_note(buf: &mut [f32], start_beat: f32, dur_beats: f32, midi: u8, amp: f32) {
    let start_frame =
        ((start_beat * BGM_BEAT_SEC * SAMPLE_RATE).round() as usize) % BGM_TOTAL_FRAMES;
    let total_sec = dur_beats * BGM_BEAT_SEC + 0.55;
    let total_frames = (total_sec * SAMPLE_RATE) as usize;
    let attack_frames = (0.20 * SAMPLE_RATE) as usize;
    let release_frames = (0.65 * SAMPLE_RATE) as usize;
    let release_start = total_frames.saturating_sub(release_frames);

    let f0 = midi_to_hz(midi);
    let mut osc1 = Phasor::new(f0, SAMPLE_RATE, 0.0);
    let mut osc2 = Phasor::new(f0 * 2.0, SAMPLE_RATE, 0.25);
    let mut sustain_env = 1.0_f32;
    let sustain_dec = (-1.0 / (5.5 * SAMPLE_RATE)).exp();

    for i in 0..total_frames {
        if i > 0 && (i & 1023) == 0 {
            osc1.renormalize();
            osc2.renormalize();
        }
        let attack = if i < attack_frames {
            smooth_cosine_ramp(i as f32 / attack_frames.max(1) as f32)
        } else {
            1.0
        };
        let release = if i >= release_start {
            1.0 - smooth_cosine_ramp((i - release_start) as f32 / release_frames.max(1) as f32)
        } else {
            1.0
        };

        let sig = (osc1.next_sin() * 0.84 + osc2.next_sin() * 0.20)
            * amp
            * sustain_env
            * attack
            * release;
        sustain_env *= sustain_dec;

        let frame = (start_frame + i) % BGM_TOTAL_FRAMES;
        let idx = frame * 2;
        buf[idx] += sig * FRAC_1_SQRT_2;
        buf[idx + 1] += sig * FRAC_1_SQRT_2;
    }
}

struct DampedComb {
    buf: Vec<f32>,
    pos: usize,
    filter_store: f32,
    feedback: f32,
    damp1: f32,
    damp2: f32,
}

impl DampedComb {
    fn new(delay_samples: usize, feedback: f32, damp: f32) -> Self {
        Self {
            buf: vec![0.0; delay_samples.max(1)],
            pos: 0,
            filter_store: 0.0,
            feedback,
            damp1: damp,
            damp2: 1.0 - damp,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let out = self.buf[self.pos];
        self.filter_store = out * self.damp2 + self.filter_store * self.damp1;
        self.buf[self.pos] = input + self.filter_store * self.feedback;
        self.pos += 1;
        if self.pos >= self.buf.len() {
            self.pos = 0;
        }
        out
    }
}

struct AllPass {
    buf: Vec<f32>,
    pos: usize,
    feedback: f32,
}

impl AllPass {
    fn new(delay_samples: usize, feedback: f32) -> Self {
        Self {
            buf: vec![0.0; delay_samples.max(1)],
            pos: 0,
            feedback,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let buf_out = self.buf[self.pos];
        let out = -input + buf_out;
        self.buf[self.pos] = input + buf_out * self.feedback;
        self.pos += 1;
        if self.pos >= self.buf.len() {
            self.pos = 0;
        }
        out
    }
}

fn apply_circular_echo_and_hall_reverb(dry: &[f32]) -> Vec<f32> {
    let total_frames = dry.len() / 2;
    let mut out = vec![0.0_f32; dry.len()];

    let echo_l_len = (0.675 * SAMPLE_RATE) as usize;
    let echo_r_len = (0.900 * SAMPLE_RATE) as usize;
    let mut echo_l = vec![0.0_f32; echo_l_len];
    let mut echo_r = vec![0.0_f32; echo_r_len];
    let mut echo_pos_l = 0usize;
    let mut echo_pos_r = 0usize;
    let mut echo_lp_l = 0.0_f32;
    let mut echo_lp_r = 0.0_f32;

    let predelay_len = (0.024 * SAMPLE_RATE) as usize;
    let mut predelay_l = vec![0.0_f32; predelay_len];
    let mut predelay_r = vec![0.0_f32; predelay_len];
    let mut pre_pos = 0usize;

    let mut combs_l = [
        DampedComb::new(1559, 0.80, 0.36),
        DampedComb::new(1789, 0.80, 0.36),
        DampedComb::new(2063, 0.80, 0.36),
        DampedComb::new(2381, 0.80, 0.36),
    ];
    let mut combs_r = [
        DampedComb::new(1583, 0.80, 0.36),
        DampedComb::new(1811, 0.80, 0.36),
        DampedComb::new(2089, 0.80, 0.36),
        DampedComb::new(2411, 0.80, 0.36),
    ];
    let mut ap_l = [AllPass::new(227, 0.5), AllPass::new(557, 0.5)];
    let mut ap_r = [AllPass::new(241, 0.5), AllPass::new(577, 0.5)];

    let warmup_frames = (5.0 * SAMPLE_RATE) as usize;
    let total_passes = warmup_frames + total_frames;
    let start_offset = total_frames - (warmup_frames % total_frames);

    for step in 0..total_passes {
        let frame = (start_offset + step) % total_frames;
        let idx = frame * 2;
        let in_l = dry[idx];
        let in_r = dry[idx + 1];

        let dl = echo_l[echo_pos_l];
        let dr = echo_r[echo_pos_r];
        echo_lp_l = dl * 0.32 + echo_lp_l * 0.68;
        echo_lp_r = dr * 0.32 + echo_lp_r * 0.68;
        echo_l[echo_pos_l] = in_l + echo_lp_r * 0.21;
        echo_r[echo_pos_r] = in_r + echo_lp_l * 0.21;
        echo_pos_l = (echo_pos_l + 1) % echo_l_len;
        echo_pos_r = (echo_pos_r + 1) % echo_r_len;

        let post_echo_l = in_l + echo_lp_l * 0.11;
        let post_echo_r = in_r + echo_lp_r * 0.11;

        let rev_in_l = predelay_l[pre_pos];
        let rev_in_r = predelay_r[pre_pos];
        predelay_l[pre_pos] = post_echo_l * 0.25;
        predelay_r[pre_pos] = post_echo_r * 0.25;
        pre_pos = (pre_pos + 1) % predelay_len;

        let mut wet_l = 0.0_f32;
        let mut wet_r = 0.0_f32;
        for c in &mut combs_l {
            wet_l += c.process(rev_in_l);
        }
        for c in &mut combs_r {
            wet_r += c.process(rev_in_r);
        }
        for a in &mut ap_l {
            wet_l = a.process(wet_l);
        }
        for a in &mut ap_r {
            wet_r = a.process(wet_r);
        }

        if step >= warmup_frames {
            let out_frame = step - warmup_frames;
            let out_idx = out_frame * 2;
            out[out_idx] = post_echo_l * 0.78 + wet_l * 0.22 + wet_r * 0.04;
            out[out_idx + 1] = post_echo_r * 0.78 + wet_r * 0.22 + wet_l * 0.04;
        }
    }

    // Remove DC offset and normalize peak cleanly to 0.72 (-2.85 dBFS)
    let mut sum_l = 0.0_f64;
    let mut sum_r = 0.0_f64;
    for &[l, r] in out.as_chunks::<2>().0 {
        sum_l += l as f64;
        sum_r += r as f64;
    }
    let dc_l = (sum_l / total_frames.max(1) as f64) as f32;
    let dc_r = (sum_r / total_frames.max(1) as f64) as f32;

    let mut peak = 1e-6_f32;
    for [l, r] in out.as_chunks_mut::<2>().0 {
        *l -= dc_l;
        *r -= dc_r;
        peak = peak.max(l.abs()).max(r.abs());
    }

    let gain = (0.72 / peak).min(2.5);
    for s in &mut out {
        *s = (*s * gain).clamp(-0.98, 0.98);
    }

    out
}

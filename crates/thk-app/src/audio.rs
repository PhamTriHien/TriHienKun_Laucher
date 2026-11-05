use rodio::buffer::SamplesBuffer;
use rodio::source::Source;
use rodio::{Decoder, DeviceSinkBuilder, Player};
use std::f32::consts::TAU;
use std::fs::File;
use std::io::BufReader;
use std::num::{NonZeroU16, NonZeroU32};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

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
                let sample_rate = NonZeroU32::new(44_100).expect("non-zero sample rate");
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
                    let pad = synthesize_ambient_loop();
                    let channels = NonZeroU16::new(1).expect("non-zero channel");
                    let sample_rate = NonZeroU32::new(44_100).expect("non-zero sample rate");
                    let buf = SamplesBuffer::new(channels, sample_rate, pad);
                    music_player.append(buf.repeat_infinite());
                    active_music_path = None;
                    active_ambient = true;
                }
            }
        }
    }
}

pub(crate) fn synthesize_sfx_samples(effect: SoundEffect) -> Vec<f32> {
    let sr = 44_100.0_f32;
    match effect {
        SoundEffect::Click => synth_notes(&[(880.0, 0.032, 0.14)], sr),
        SoundEffect::Success => synth_notes(&[(587.33, 0.075, 0.16), (880.0, 0.12, 0.18)], sr),
        SoundEffect::Launch => synth_notes(
            &[
                (440.0, 0.07, 0.16),
                (554.37, 0.07, 0.16),
                (659.25, 0.08, 0.17),
                (880.0, 0.18, 0.20),
            ],
            sr,
        ),
        SoundEffect::Error => synth_notes(&[(311.13, 0.09, 0.20), (233.08, 0.16, 0.20)], sr),
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

fn synthesize_ambient_loop() -> Vec<f32> {
    let sr = 44_100.0_f32;
    let duration = 8.0_f32;
    let total = (sr * duration) as usize;
    let freqs = [220.0_f32, 277.18_f32, 329.63_f32, 440.0_f32];
    let mut out = Vec::with_capacity(total);
    for i in 0..total {
        let t = i as f32 / sr;
        let lfo = 0.65 + 0.35 * (TAU * (t / duration)).sin();
        let mut sample = 0.0_f32;
        for &f in &freqs {
            sample += (TAU * f * t).sin();
        }
        out.push((sample / freqs.len() as f32) * 0.06 * lfo);
    }
    out
}

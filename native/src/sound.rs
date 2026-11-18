//! Start-up sound: an original synthesized "systems coming online" sound (a low swell, a rising sweep, three chimes
//! and a shimmer), or an audio file the user picks. Nothing from any film is used.

use rodio::buffer::SamplesBuffer;
use std::{f32::consts::TAU, fs::File, io::BufReader, thread, time::Duration};

const RATE: u32 = 44_100;

fn env(t: f32, attack: f32, decay: f32) -> f32 {
    if t < 0.0 {
        0.0
    } else if t < attack {
        t / attack
    } else {
        (-(t - attack) / decay).exp()
    }
}

/// About 2.4 s of mono samples.
pub fn original() -> Vec<f32> {
    let len = (RATE as f32 * 2.4) as usize;
    let mut out = vec![0.0f32; len];
    let mut sweep_phase = 0.0f32;
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        // Low swell: two octaves of A, rising in and fading out.
        let swell = ((t / 1.2).min(1.0) * (1.0 - ((t - 1.6) / 0.8).clamp(0.0, 1.0))) * 0.22;
        let drone = swell * ((TAU * 55.0 * t).sin() * 0.6 + (TAU * 110.0 * t).sin() * 0.4);
        // Rising sweep 300 → 1400 Hz in the first second, quiet.
        let sweep = if t < 1.0 {
            let f = 300.0 * (1400.0f32 / 300.0).powf(t);
            sweep_phase += TAU * f / RATE as f32;
            sweep_phase.sin() * 0.08 * (t * 3.0).min(1.0) * (1.0 - t).max(0.0)
        } else {
            0.0
        };
        // Three chimes (A4, E5, A5), slightly detuned pairs, each ringing out.
        let mut chimes = 0.0;
        for (k, f) in [440.0f32, 659.25, 880.0].iter().enumerate() {
            let t0 = 0.95 + k as f32 * 0.13;
            let e = env(t - t0, 0.006, 0.55);
            chimes += e * ((TAU * f * (t - t0)).sin() + 0.5 * (TAU * f * 1.003 * (t - t0)).sin() + 0.25 * (TAU * f * 2.0 * (t - t0)).sin()) * 0.16;
        }
        // Shimmer at the end: a high, soft, fast tremolo.
        let shimmer = env(t - 1.35, 0.25, 0.6) * (TAU * 1760.0 * t).sin() * (0.5 + 0.5 * (TAU * 9.0 * t).sin()) * 0.04;
        *s = drone + sweep + chimes + shimmer;
    }
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    out.iter_mut().for_each(|v| *v = *v / peak * 0.5);
    // Fade the last 120 ms to silence.
    let fade = (RATE as f32 * 0.12) as usize;
    for k in 0..fade {
        out[len - 1 - k] *= k as f32 / fade as f32;
    }
    out
}

/// Plays the start-up sound chosen in Definições on a background thread ("original" | "file" | "off").
pub fn play_startup(choice: &str, file: &str) {
    let (choice, file) = (choice.to_owned(), file.to_owned());
    if choice == "off" {
        return;
    }
    thread::spawn(move || {
        let Ok(sink) = rodio::DeviceSinkBuilder::open_default_sink() else { return };
        let player = if choice == "file" && !file.is_empty() {
            let Ok(f) = File::open(&file) else { return };
            match rodio::play(sink.mixer(), BufReader::new(f)) {
                Ok(p) => p,
                Err(_) => return,
            }
        } else {
            let player = rodio::Player::connect_new(sink.mixer());
            player.append(SamplesBuffer::new(1.try_into().unwrap(), RATE.try_into().unwrap(), original()));
            player
        };
        // A user file may be a whole song: the start-up plays at most 20 seconds of it.
        let started = std::time::Instant::now();
        while !player.empty() && started.elapsed() < Duration::from_secs(20) {
            thread::sleep(Duration::from_millis(100));
        }
        player.stop();
    });
}

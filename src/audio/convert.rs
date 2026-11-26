//! Brings decoded frames to the output's layout: its channels first, then
//! its rate (Rubato's FFT resampler).

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler as _, WindowFunction};

use super::Format;

/// The frames the resampler's filter spans of a 44.1 or 48 kHz file; of a
/// faster one, as many more as it takes to span the same time. At this
/// length 20 kHz passes untouched from every rate, and what would alias is
/// gone (the tests measure both).
const FILTER_FRAMES: usize = 1024;
/// The filters the resampler overlaps in the chunk it takes at a time.
const FILTERS_PER_CHUNK: usize = 2;
/// A center or surround channel folded into a front one (-3 dB).
const FOLD: f32 = std::f32::consts::FRAC_1_SQRT_2;

pub(super) struct Converter {
    from: Format,
    to: Format,
    remapped: Vec<f32>,
    resampler: Option<Resampler>,
}

impl Converter {
    pub(super) fn new(from: Format, to: Format) -> Result<Self, String> {
        let resampler = (from.rate != to.rate)
            .then(|| Resampler::new(from.rate, to.rate, to.channels))
            .transpose()?;
        Ok(Self {
            from,
            to,
            remapped: Vec::new(),
            resampler,
        })
    }

    /// The layout of the frames this takes.
    pub(super) fn from(&self) -> Format {
        self.from
    }

    /// Appends `samples`, converted, to `out`. The resampler keeps what
    /// does not fill a chunk for the next call.
    pub(super) fn push(&mut self, samples: &[f32], out: &mut Vec<f32>) {
        let samples = if self.from.channels == self.to.channels {
            samples
        } else {
            self.remapped.clear();
            remap(
                samples,
                self.from.channels,
                self.to.channels,
                &mut self.remapped,
            );
            &self.remapped
        };
        match &mut self.resampler {
            Some(resampler) => resampler.push(samples, out),
            None => out.extend_from_slice(samples),
        }
    }

    /// The frames still to come out for what was pushed.
    pub(super) fn pending(&self) -> u64 {
        self.resampler.as_ref().map_or(0, Resampler::pending)
    }

    /// Appends what the resampler still holds: nothing follows what was
    /// pushed. What is pushed after this stands alone.
    pub(super) fn finish(&mut self, out: &mut Vec<f32>) {
        if let Some(resampler) = &mut self.resampler {
            resampler.finish(out);
        }
    }

    /// Forgets what the resampler holds: the next frames are from elsewhere
    /// in the file.
    pub(super) fn reset(&mut self) {
        if let Some(resampler) = &mut self.resampler {
            resampler.reset();
        }
    }
}

struct Resampler {
    inner: Fft<f32>,
    channels: usize,
    from_rate: u32,
    to_rate: u32,
    /// Frames taken but not yet resampled.
    input: Vec<f32>,
    /// One chunk's output.
    chunk: Vec<f32>,
    /// Frames of the resampler's delay still to drop from its output.
    delay: usize,
    taken: u64,
    given: u64,
}

impl Resampler {
    fn new(from_rate: u32, to_rate: u32, channels: usize) -> Result<Self, String> {
        let filter = FILTER_FRAMES * (from_rate as usize / 44_100).max(1).next_power_of_two();
        let inner = Fft::<f32>::new_custom(
            from_rate as usize,
            to_rate as usize,
            filter * FILTERS_PER_CHUNK,
            FILTERS_PER_CHUNK,
            channels,
            WindowFunction::BlackmanHarris2,
            FixedSync::Input,
        )
        .map_err(|error| format!("resampler: {error}"))?;
        Ok(Self {
            chunk: vec![0.0; inner.output_frames_max() * channels],
            delay: inner.output_delay(),
            inner,
            channels,
            from_rate,
            to_rate,
            input: Vec::new(),
            taken: 0,
            given: 0,
        })
    }

    fn push(&mut self, samples: &[f32], out: &mut Vec<f32>) {
        self.input.extend_from_slice(samples);
        self.taken += (samples.len() / self.channels) as u64;

        let mut start = 0;
        loop {
            let needed = self.inner.input_frames_next() * self.channels;
            if self.input.len() - start < needed {
                break;
            }
            self.process(start, out);
            start += needed;
        }
        self.input.drain(..start);
    }

    /// The frames that come out for the ones taken.
    fn expected(&self) -> u64 {
        (self.taken * u64::from(self.to_rate)).div_ceil(u64::from(self.from_rate.max(1)))
    }

    fn pending(&self) -> u64 {
        self.expected().saturating_sub(self.given)
    }

    fn finish(&mut self, out: &mut Vec<f32>) {
        let expected = self.expected();
        // Silence pushes out what the delay holds back; no file needs more
        // than a few chunks of it.
        for _ in 0..16 {
            if self.given >= expected {
                break;
            }
            let needed = self.inner.input_frames_next() * self.channels;
            self.input.resize(needed.max(self.input.len()), 0.0);
            self.process(0, out);
            self.input.clear();
        }
        let surplus = (self.given.saturating_sub(expected)) as usize * self.channels;
        out.truncate(out.len().saturating_sub(surplus));
        // The silence is still in there.
        self.reset();
    }

    /// Resamples one chunk of the input, from sample `start`.
    fn process(&mut self, start: usize, out: &mut Vec<f32>) {
        let channels = self.channels;
        let frames = self.inner.input_frames_next();
        let capacity = self.chunk.len() / channels;
        let written = InterleavedSlice::new(&self.input[start..], channels, frames)
            .ok()
            .zip(InterleavedSlice::new_mut(&mut self.chunk, channels, capacity).ok())
            .and_then(|(input, mut output)| {
                self.inner
                    .process_into_buffer(&input, &mut output, None)
                    .ok()
            })
            .map_or(0, |(_, written)| written);

        let dropped = self.delay.min(written);
        self.delay -= dropped;
        self.given += (written - dropped) as u64;
        out.extend_from_slice(&self.chunk[dropped * channels..written * channels]);
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.input.clear();
        self.delay = self.inner.output_delay();
        self.taken = 0;
        self.given = 0;
    }
}

/// Appends `samples` (frames of `from` channels) to `out` as frames of
/// `to` channels: mono goes to the front pair, a wider layout folds down to
/// stereo or mono, and anything else keeps the channels both have.
fn remap(samples: &[f32], from: usize, to: usize, out: &mut Vec<f32>) {
    out.reserve(samples.len() / from * to);
    for frame in samples.chunks_exact(from) {
        let (left, right) = match *frame {
            [mono] => (mono, mono),
            [left, right] => (left, right),
            // 5.1 and wider: left, right, center, LFE, surrounds.
            [left, right, center, _, side_left, side_right, ..] => {
                let scale = 1.0 / (1.0 + 2.0 * FOLD);
                (
                    (left + FOLD * (center + side_left)) * scale,
                    (right + FOLD * (center + side_right)) * scale,
                )
            }
            [left, right, ..] => (left, right),
            [] => (0.0, 0.0),
        };
        match to {
            1 => out.push((left + right) * 0.5),
            2 => out.extend([left, right]),
            _ if from <= 2 => {
                out.extend([left, right]);
                out.extend(std::iter::repeat_n(0.0, to - 2));
            }
            _ => out.extend((0..to).map(|channel| frame.get(channel).copied().unwrap_or(0.0))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remap_spreads_mono_and_folds_wider_layouts() {
        let mut out = Vec::new();
        remap(&[0.5, -0.5], 1, 2, &mut out);
        assert_eq!(out, [0.5, 0.5, -0.5, -0.5]);

        out.clear();
        remap(&[1.0, 0.0], 2, 1, &mut out);
        assert_eq!(out, [0.5]);

        out.clear();
        remap(&[0.25, 0.75], 2, 6, &mut out);
        assert_eq!(out, [0.25, 0.75, 0.0, 0.0, 0.0, 0.0]);

        out.clear();
        remap(&[1.0, 1.0, 1.0, 1.0, 1.0, 1.0], 6, 2, &mut out);
        assert!(out.iter().all(|sample| (sample - 1.0).abs() < 1e-6));

        out.clear();
        remap(&[0.1, 0.2, 0.3, 0.4], 4, 6, &mut out);
        assert_eq!(out, [0.1, 0.2, 0.3, 0.4, 0.0, 0.0]);
    }

    #[test]
    fn same_layout_passes_through() {
        let format = Format {
            rate: 48_000,
            channels: 2,
        };
        let mut converter = Converter::new(format, format).unwrap();
        let mut out = Vec::new();
        converter.push(&[0.1, 0.2, 0.3, 0.4], &mut out);
        converter.finish(&mut out);
        assert_eq!(out, [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn resampling_keeps_the_length_and_the_signal() {
        let from = Format {
            rate: 44_100,
            channels: 1,
        };
        let to = Format {
            rate: 48_000,
            channels: 2,
        };
        // One second of a 441 Hz sine, in uneven pieces.
        let input: Vec<f32> = (0..44_100)
            .map(|frame| (frame as f32 * 441.0 * std::f32::consts::TAU / 44_100.0).sin())
            .collect();
        let mut converter = Converter::new(from, to).unwrap();
        let mut out = Vec::new();
        for piece in input.chunks(1_500) {
            converter.push(piece, &mut out);
        }
        converter.finish(&mut out);

        assert_eq!(out.len(), 48_000 * 2);
        // Past the edges the output is the sine, as loud, in both channels
        // (the tests below measure how true it is).
        let (mut energy, mut frames) = (0.0_f64, 0);
        for frame in out[2_000 * 2..46_000 * 2].chunks(2) {
            assert_eq!(frame[0], frame[1]);
            energy += f64::from(frame[0]) * f64::from(frame[0]);
            frames += 1;
        }
        let level = (energy / f64::from(frames)).sqrt();
        assert!(
            (level - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-3,
            "{level}"
        );

        // After a reset the next piece stands alone.
        converter.reset();
        out.clear();
        converter.push(&input[..4_410], &mut out);
        converter.finish(&mut out);
        assert_eq!(out.len(), 4_800 * 2);
    }

    /// The level, in dB, at which a `frequency` tone leaves the conversion
    /// from one rate to another: as itself, or as what it aliases to.
    fn tone_level(from: u32, to: u32, frequency: f64) -> f64 {
        let (from_format, to_format) = (
            Format {
                rate: from,
                channels: 1,
            },
            Format {
                rate: to,
                channels: 1,
            },
        );
        let input: Vec<f32> = (0..from * 2)
            .map(|frame| {
                (f64::from(frame) * frequency * std::f64::consts::TAU / f64::from(from)).sin()
                    as f32
            })
            .collect();
        let mut converter = Converter::new(from_format, to_format).unwrap();
        let mut out = Vec::new();
        converter.push(&input, &mut out);
        converter.finish(&mut out);

        // The tone's size in the output, wherever its phase: a fit of a
        // sine and a cosine, away from the edges.
        let landed = if frequency <= f64::from(to) / 2.0 {
            frequency
        } else {
            f64::from(to) - frequency
        };
        let step = landed * std::f64::consts::TAU / f64::from(to);
        let (mut ss, mut cc, mut sc, mut ys, mut yc) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let edge = to as usize / 2;
        for (frame, sample) in out.iter().enumerate().take(out.len() - edge).skip(edge) {
            let (sine, cosine) = (frame as f64 * step).sin_cos();
            let sample = f64::from(*sample);
            ss += sine * sine;
            cc += cosine * cosine;
            sc += sine * cosine;
            ys += sample * sine;
            yc += sample * cosine;
        }
        let determinant = ss * cc - sc * sc;
        let (a, b) = (
            (ys * cc - yc * sc) / determinant,
            (yc * ss - ys * sc) / determinant,
        );
        20.0 * (a * a + b * b).sqrt().max(1e-12).log10()
    }

    #[test]
    fn resampling_leaves_the_audible_band_untouched() {
        for (from, to) in [
            (44_100, 48_000),
            (48_000, 44_100),
            (88_200, 48_000),
            (96_000, 48_000),
            (176_400, 48_000),
            (192_000, 48_000),
            (44_100, 96_000),
        ] {
            for frequency in [1_000.0, 10_000.0, 20_000.0] {
                let level = tone_level(from, to, frequency);
                assert!(
                    level.abs() < 0.01,
                    "{from} to {to}, {frequency} Hz: {level} dB"
                );
            }
        }
    }

    #[test]
    fn resampling_removes_what_would_alias() {
        for (from, to, frequency) in [
            (48_000, 44_100, 22_500.0),
            (96_000, 48_000, 24_500.0),
            (96_000, 48_000, 40_000.0),
            (192_000, 48_000, 25_000.0),
            (192_000, 44_100, 30_000.0),
        ] {
            let level = tone_level(from, to, frequency);
            assert!(level < -120.0, "{from} to {to}, {frequency} Hz: {level} dB");
        }
    }
}

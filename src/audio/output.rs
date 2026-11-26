//! The system's default output (CPAL): a stream whose callback drains the
//! ring the engine feeds, at the engine's volume.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, Sample, SampleFormat, SizedSample};

use super::Format;

/// What the ring holds, in seconds: the room decoding has to fall behind.
const RING_SECONDS: usize = 2;
/// The time the volume takes from silence to full, so that a pause, a
/// resume, a seek or a volume change does not click.
const RAMP_SECONDS: f32 = 0.012;
/// The frames the callback converts at a time.
const BLOCK_FRAMES: usize = 1024;

pub(super) struct Output {
    stream: cpal::Stream,
    shared: Arc<Shared>,
    format: Format,
}

/// What the engine and the callback both touch, without waiting.
struct Shared {
    playing: AtomicBool,
    /// The volume, as the bits of an `f32` factor.
    gain: AtomicU32,
    /// The stream broke and has to be opened again.
    failed: AtomicBool,
    /// The feed that replaces the callback's, set aside until it runs.
    incoming: Mutex<Option<Source>>,
    has_incoming: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            gain: AtomicU32::new(1.0_f32.to_bits()),
            failed: AtomicBool::new(false),
            incoming: Mutex::new(None),
            has_incoming: AtomicBool::new(false),
        }
    }

    /// Sets `source` aside for the callback to take over.
    fn replace_source(&self, source: Source) {
        // The callback only ever tries this lock, so it is never held long.
        *self
            .incoming
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(source);
        self.has_incoming.store(true, Ordering::Release);
    }
}

/// The callback's end of a feed.
pub(super) struct Source {
    samples: rtrb::Consumer<f32>,
    played_frames: Arc<AtomicU64>,
}

#[cfg(test)]
impl Source {
    /// Takes what is queued, as the callback would, into `out`.
    pub(super) fn take(&mut self, out: &mut Vec<f32>, channels: usize) {
        let ready = self.samples.slots() / channels * channels;
        let start = out.len();
        out.resize(start + ready, 0.0);
        self.samples
            .pop_entire_slice(&mut out[start..])
            .expect("what is ready can be taken");
        self.played_frames
            .fetch_add((ready / channels) as u64, Ordering::Relaxed);
    }
}

/// The engine's end of a feed: the ring it fills, and how much of it the
/// output has played.
pub(super) struct Feed {
    samples: rtrb::Producer<f32>,
    played_frames: Arc<AtomicU64>,
}

impl Feed {
    /// A feed of frames in `format`, and the end the output takes it from.
    pub(super) fn new(format: Format) -> (Self, Source) {
        let capacity = format.rate as usize * format.channels * RING_SECONDS;
        let (producer, consumer) = rtrb::RingBuffer::new(capacity);
        let played_frames = Arc::new(AtomicU64::new(0));
        let source = Source {
            samples: consumer,
            played_frames: played_frames.clone(),
        };
        let feed = Self {
            samples: producer,
            played_frames,
        };
        (feed, source)
    }

    /// Queues what fits of `samples`, and tells how many that was.
    pub(super) fn push(&mut self, samples: &[f32]) -> usize {
        let (_, rest) = self.samples.push_partial_slice(samples);
        samples.len() - rest.len()
    }

    /// The frames the output took from this feed.
    pub(super) fn played_frames(&self) -> u64 {
        self.played_frames.load(Ordering::Relaxed)
    }

    /// Everything queued was taken.
    pub(super) fn is_drained(&self) -> bool {
        self.samples.slots() == self.samples.buffer().capacity()
    }
}

impl Output {
    /// Opens the default output device with the configuration it prefers.
    /// The stream waits, silent, until [`Output::set_playing`].
    pub(super) fn open() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| "no audio output device".to_owned())?;
        let supported = device
            .default_output_config()
            .map_err(|error| format!("audio output: {error}"))?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let format = Format {
            rate: config.sample_rate,
            channels: usize::from(config.channels),
        };
        if format.rate == 0 || format.channels == 0 {
            return Err("audio output has no channels".to_owned());
        }

        let shared = Arc::new(Shared::new());
        let stream = match sample_format {
            SampleFormat::F32 => build::<f32>(&device, config, format, &shared),
            SampleFormat::F64 => build::<f64>(&device, config, format, &shared),
            SampleFormat::I8 => build::<i8>(&device, config, format, &shared),
            SampleFormat::I16 => build::<i16>(&device, config, format, &shared),
            SampleFormat::I24 => build::<cpal::I24>(&device, config, format, &shared),
            SampleFormat::I32 => build::<i32>(&device, config, format, &shared),
            SampleFormat::I64 => build::<i64>(&device, config, format, &shared),
            SampleFormat::U8 => build::<u8>(&device, config, format, &shared),
            SampleFormat::U16 => build::<u16>(&device, config, format, &shared),
            SampleFormat::U32 => build::<u32>(&device, config, format, &shared),
            SampleFormat::U64 => build::<u64>(&device, config, format, &shared),
            other => return Err(format!("audio output: unsupported sample format {other}")),
        }
        .map_err(|error| format!("audio output: {error}"))?;
        // Some hosts start a stream as it is built.
        let _ = stream.pause();

        Ok(Self {
            stream,
            shared,
            format,
        })
    }

    /// The layout of the frames a feed takes.
    pub(super) fn format(&self) -> Format {
        self.format
    }

    /// The stream broke (its device went away): open another.
    pub(super) fn failed(&self) -> bool {
        self.shared.failed.load(Ordering::Relaxed)
    }

    /// Starts a new feed, which the output takes over from the one before
    /// once that has faded out; what was still queued of it is dropped.
    pub(super) fn begin(&self) -> Feed {
        let (feed, source) = Feed::new(self.format);
        self.shared.replace_source(source);
        feed
    }

    /// Fades the feed in or out. Playing wakes the stream; [`Output::rest`]
    /// lets it sleep once the fade out is over.
    pub(super) fn set_playing(&self, playing: bool) {
        self.shared.playing.store(playing, Ordering::Relaxed);
        if playing && let Err(error) = self.stream.play() {
            eprintln!("Audio output failed to start: {error}");
            self.shared.failed.store(true, Ordering::Relaxed);
        }
    }

    /// Stops the stream's callbacks, so that a silent player neither wakes
    /// the processor nor keeps the system awake.
    pub(super) fn rest(&self) {
        if !self.shared.playing.load(Ordering::Relaxed) {
            let _ = self.stream.pause();
        }
    }

    pub(super) fn set_gain(&self, gain: f32) {
        self.shared.gain.store(gain.to_bits(), Ordering::Relaxed);
    }
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    format: Format,
    shared: &Arc<Shared>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let mut renderer = Renderer::new(shared.clone(), format);
    let shared = shared.clone();
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| renderer.render(data),
        move |error: cpal::Error| match error.kind() {
            // The stream goes on: it moved to the new default device, or
            // it glitched.
            ErrorKind::DeviceChanged | ErrorKind::Xrun | ErrorKind::RealtimeDenied => {}
            _ => {
                eprintln!("Audio output failed: {error}");
                shared.failed.store(true, Ordering::Relaxed);
            }
        },
        None,
    )
}

/// The callback's state. Nothing here waits or allocates.
struct Renderer {
    shared: Arc<Shared>,
    channels: usize,
    source: Option<Source>,
    /// The feed that takes over once `source` has faded out.
    next: Option<Source>,
    /// The volume now, on its way to the one asked for.
    gain: f32,
    /// How far the volume moves in a frame.
    ramp_step: f32,
    block: Vec<f32>,
}

impl Renderer {
    fn new(shared: Arc<Shared>, format: Format) -> Self {
        Self {
            shared,
            channels: format.channels,
            source: None,
            next: None,
            gain: 0.0,
            ramp_step: 1.0 / (RAMP_SECONDS * format.rate as f32),
            block: vec![0.0; BLOCK_FRAMES * format.channels],
        }
    }

    fn render<T>(&mut self, data: &mut [T])
    where
        T: Sample + FromSample<f32>,
    {
        if self.shared.has_incoming.swap(false, Ordering::Acquire) {
            match self.shared.incoming.try_lock() {
                Ok(mut incoming) => {
                    if let Some(source) = incoming.take() {
                        self.next = Some(source);
                    }
                }
                // The engine is setting one aside right now: next time.
                Err(_) => self.shared.has_incoming.store(true, Ordering::Release),
            }
        }
        let playing = self.shared.playing.load(Ordering::Relaxed);
        let volume = f32::from_bits(self.shared.gain.load(Ordering::Relaxed));

        let mut rest = data;
        while !rest.is_empty() {
            // A feed that is replaced fades out, then the new one fades
            // in: a seek or a skip cuts no wave in two.
            if self.next.is_some() && (self.source.is_none() || self.gain <= 0.0) {
                self.source = self.next.take();
                self.gain = 0.0;
            }
            let replaced = self.next.is_some();
            let target = if playing && !replaced { volume } else { 0.0 };

            // As far as the fade out goes, when one is under way.
            let frames = if replaced {
                (self.gain / self.ramp_step).ceil() as usize
            } else {
                BLOCK_FRAMES
            }
            .clamp(1, BLOCK_FRAMES);
            let length = (frames * self.channels).min(rest.len());
            let (out, tail) = std::mem::take(&mut rest).split_at_mut(length);
            rest = tail;

            let source = match &mut self.source {
                // A paused feed is still taken from until it has faded out.
                Some(source) if playing || self.gain > 0.0 => source,
                _ => {
                    out.fill(T::EQUILIBRIUM);
                    continue;
                }
            };
            let block = &mut self.block[..out.len()];
            // Whole frames only: the engine may be mid-frame.
            let ready = source.samples.slots() / self.channels * self.channels;
            let taken = ready.min(block.len() / self.channels * self.channels);
            if source
                .samples
                .pop_entire_slice(&mut block[..taken])
                .is_err()
            {
                block.fill(0.0);
            } else {
                block[taken..].fill(0.0);
                source
                    .played_frames
                    .fetch_add((taken / self.channels) as u64, Ordering::Relaxed);
            }

            for (frame, out) in block
                .chunks(self.channels)
                .zip(out.chunks_mut(self.channels))
            {
                self.gain = if self.gain < target {
                    (self.gain + self.ramp_step).min(target)
                } else {
                    (self.gain - self.ramp_step).max(target)
                };
                for (sample, out) in frame.iter().zip(out) {
                    *out = T::from_sample(sample * self.gain);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORMAT: Format = Format {
        rate: 48_000,
        channels: 2,
    };

    /// A renderer, playing at full volume, and what it shares.
    fn renderer() -> (Renderer, Arc<Shared>) {
        let shared = Arc::new(Shared::new());
        shared.playing.store(true, Ordering::Relaxed);
        (Renderer::new(shared.clone(), FORMAT), shared)
    }

    /// A feed of one second of `level`, set aside for the renderer.
    fn feed(shared: &Shared, level: f32) -> Feed {
        let (mut feed, source) = Feed::new(FORMAT);
        let second = vec![level; FORMAT.rate as usize * FORMAT.channels];
        assert_eq!(feed.push(&second), second.len());
        shared.replace_source(source);
        feed
    }

    /// The largest step between frames of `samples`' left channel.
    fn largest_step(samples: &[f32]) -> f32 {
        samples
            .chunks(FORMAT.channels)
            .map(|frame| frame[0])
            .fold((samples[0], 0.0_f32), |(previous, largest), sample| {
                (sample, largest.max((sample - previous).abs()))
            })
            .1
    }

    #[test]
    fn a_feed_fades_in_and_plays_untouched_at_full_volume() {
        let (mut renderer, shared) = renderer();
        let feed = feed(&shared, 0.5);
        let mut out = vec![0.0_f32; 4_800 * FORMAT.channels];

        renderer.render(&mut out);

        assert_eq!(feed.played_frames(), 4_800);
        assert!(largest_step(&out) <= 0.5 * renderer.ramp_step + 1e-6);
        // Past the fade, the samples are the feed's own.
        assert!(
            out[1_200 * FORMAT.channels..]
                .iter()
                .all(|&sample| sample == 0.5)
        );
    }

    #[test]
    fn a_replaced_feed_fades_out_before_the_new_one_fades_in() {
        let (mut renderer, shared) = renderer();
        let old = feed(&shared, 1.0);
        let mut out = vec![0.0_f32; 2_400 * FORMAT.channels];
        renderer.render(&mut out);
        assert_eq!(out.last(), Some(&1.0));

        let new = feed(&shared, -1.0);
        let played = old.played_frames();
        let mut swapped = vec![0.0_f32; 2_400 * FORMAT.channels];
        renderer.render(&mut swapped);

        // No step is larger than the fade's: nothing clicks.
        out.extend_from_slice(&swapped);
        assert!(largest_step(&out) <= renderer.ramp_step + 1e-6);
        assert_eq!(swapped.last(), Some(&-1.0));
        // The old feed gave only its fade out; the new one the rest.
        let faded = (old.played_frames() - played) as usize;
        let ramp = (RAMP_SECONDS * FORMAT.rate as f32).ceil() as usize;
        assert!(faded.abs_diff(ramp) <= 1, "{faded} vs {ramp}");
        assert_eq!(new.played_frames() as usize, 2_400 - faded);
    }

    #[test]
    fn a_paused_feed_fades_out_and_holds_its_place() {
        let (mut renderer, shared) = renderer();
        let feed = feed(&shared, 1.0);
        let mut out = vec![0.0_f32; 2_400 * FORMAT.channels];
        renderer.render(&mut out);

        shared.playing.store(false, Ordering::Relaxed);
        renderer.render(&mut out);
        assert!(largest_step(&out) <= renderer.ramp_step + 1e-6);
        assert_eq!(out.last(), Some(&0.0));
        let held = feed.played_frames();
        renderer.render(&mut out);
        assert_eq!(feed.played_frames(), held);
        assert!(out.iter().all(|&sample| sample == 0.0));
    }
}

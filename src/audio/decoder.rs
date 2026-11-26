//! Reads an audio file as interleaved `f32` frames (Symphonia).

use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::common::Tier;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase, Timestamp};

use super::Format;

/// Packets in a row that may fail to decode before the file is given up.
const MAX_DECODE_ERRORS: u32 = 64;
/// The rate Opus always decodes at.
const OPUS_RATE: u32 = 48_000;

/// Symphonia's own codecs, and Opus, which it does not decode yet.
fn codecs() -> &'static CodecRegistry {
    static CODECS: OnceLock<CodecRegistry> = OnceLock::new();
    CODECS.get_or_init(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        registry.register_audio_decoder_at_tier::<symphonia_opus::OpusDecoder>(Tier::Fallback);
        registry
    })
}

pub(super) struct Decoder {
    path: PathBuf,
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    duration: Option<Duration>,
    /// What the encoder put before the first frame that the decoder
    /// leaves in (Opus; the other codecs drop theirs).
    lead_in: Duration,
    /// Timestamps count frames, so a packet's trims do too.
    counts_frames: bool,
    /// What is still to be dropped before the next frames: the lead-in, or
    /// what a seek landed before its target.
    skip: Duration,
    /// The reader gave its last packet, or failed.
    ended: bool,
    samples: Vec<f32>,
}

impl Decoder {
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| error.to_string())?;
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
            hint.with_extension(extension);
        }
        let stream = MediaSourceStream::new(Box::new(file), Default::default());
        let reader = symphonia::default::get_probe()
            .probe(
                &hint,
                stream,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|error| format!("unsupported file: {error}"))?;

        let track = reader
            .default_track(TrackType::Audio)
            .ok_or_else(|| "no audio track".to_owned())?;
        let parameters = track
            .codec_params
            .as_ref()
            .and_then(|parameters| parameters.audio())
            .ok_or_else(|| "no audio codec parameters".to_owned())?;
        let decoder = codecs()
            .make_audio_decoder(parameters, &AudioDecoderOptions::default())
            .map_err(|error| format!("unsupported codec: {error}"))?;

        let opus = parameters.codec == CODEC_ID_OPUS;
        let rate = if opus {
            Some(OPUS_RATE)
        } else {
            parameters.sample_rate
        };
        let lead_in = track
            .delay
            .filter(|_| opus)
            .map_or(Duration::ZERO, |frames| {
                Duration::from_secs_f64(f64::from(frames) / f64::from(OPUS_RATE))
            });
        let time_base = track
            .time_base
            .or_else(|| rate.and_then(TimeBase::try_from_recip));
        let counts_frames =
            time_base.is_some() && time_base == rate.and_then(TimeBase::try_from_recip);
        let duration = track
            .duration
            .zip(time_base)
            .and_then(|(duration, time_base)| time_base.calc_duration(duration))
            .and_then(to_duration)
            .or_else(|| {
                let frames = track.num_frames?;
                let rate = parameters.sample_rate.filter(|&rate| rate > 0)?;
                Some(Duration::from_secs_f64(frames as f64 / f64::from(rate)))
            })
            .map(|duration| duration.saturating_sub(lead_in));

        Ok(Self {
            path: path.to_path_buf(),
            ended: false,
            track_id: track.id,
            time_base,
            duration,
            lead_in,
            counts_frames,
            skip: lead_in,
            samples: Vec::new(),
            decoder,
            reader,
        })
    }

    /// The file's length, when its container tells it.
    pub(super) fn duration(&self) -> Option<Duration> {
        self.duration
    }

    /// The next frames, interleaved, or `None` at the end. A packet that
    /// fails to decode is skipped.
    pub(super) fn next(&mut self) -> Result<Option<(Format, &[f32])>, String> {
        let mut errors = 0;
        loop {
            let packet = match self.reader.next_packet() {
                Ok(Some(packet)) => packet,
                Err(Error::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Ok(None) => {
                    self.ended = true;
                    return Ok(None);
                }
                Err(Error::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => {
                    self.ended = true;
                    return Ok(None);
                }
                Err(error) => {
                    self.ended = true;
                    return Err(error.to_string());
                }
            };
            if packet.track_id != self.track_id {
                continue;
            }

            let buffer = match self.decoder.decode(&packet) {
                Ok(buffer) => buffer,
                Err(Error::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(Error::DecodeError(_) | Error::IoError(_)) if errors < MAX_DECODE_ERRORS => {
                    errors += 1;
                    continue;
                }
                Err(error) => return Err(error.to_string()),
            };
            let format = Format {
                rate: buffer.spec().rate(),
                channels: buffer.num_planes(),
            };
            if buffer.frames() == 0 || format.rate == 0 || format.channels == 0 {
                continue;
            }
            self.samples.clear();
            buffer.copy_to_vec_interleaved(&mut self.samples);

            // The encoder's lead-in and padding, where the decoder left
            // them in: without them one file runs into the next gaplessly.
            let (start, end) = (packet.trim_start.get(), packet.trim_end.get());
            let frames = (self.samples.len() / format.channels) as u64;
            if self.counts_frames && start + end > 0 && frames == packet.block_dur().get() {
                self.samples
                    .truncate((frames - end) as usize * format.channels);
                self.samples.drain(..start as usize * format.channels);
                if self.samples.is_empty() {
                    continue;
                }
            }

            if !self.skip.is_zero() {
                let frames = self.samples.len() / format.channels;
                let wanted = (self.skip.as_secs_f64() * f64::from(format.rate)).round() as usize;
                let dropped = wanted.min(frames);
                self.skip = if dropped == wanted {
                    Duration::ZERO
                } else {
                    self.skip.saturating_sub(Duration::from_secs_f64(
                        dropped as f64 / f64::from(format.rate),
                    ))
                };
                self.samples.drain(..dropped * format.channels);
                if self.samples.is_empty() {
                    continue;
                }
            }
            return Ok(Some((format, &self.samples)));
        }
    }

    /// Goes back to the first frame.
    pub(super) fn restart(&mut self) -> Result<(), String> {
        *self = Self::open(&self.path)?;
        Ok(())
    }

    /// Moves to `position`, and tells where the next frames are from.
    pub(super) fn seek(&mut self, position: Duration) -> Result<Duration, String> {
        // Not every reader goes back once it has read its end.
        if self.ended {
            self.restart()?;
        }
        let time = i64::try_from(position.as_secs())
            .ok()
            .and_then(|seconds| Time::try_new(seconds, position.subsec_nanos()))
            .ok_or_else(|| "position out of range".to_owned())?;
        let seeked = self
            .reader
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|error| error.to_string())?;
        self.decoder.reset();

        // The reader lands on the packet holding the position; the frames
        // of it before the position are dropped as they are decoded.
        let (landed, wanted) = (
            self.time_of(seeked.actual_ts),
            self.time_of(seeked.required_ts),
        );
        match (landed, wanted) {
            (Some(landed), Some(wanted)) => {
                self.skip = wanted.saturating_sub(landed);
                if landed.is_zero() {
                    self.skip += self.lead_in;
                }
                Ok(landed.max(wanted))
            }
            _ => {
                self.skip = Duration::ZERO;
                Ok(landed.unwrap_or(position))
            }
        }
    }

    fn time_of(&self, timestamp: Timestamp) -> Option<Duration> {
        self.time_base?.calc_time(timestamp).and_then(to_duration)
    }
}

/// A time from the file's start; none before it.
fn to_duration(time: Time) -> Option<Duration> {
    u64::try_from(time.as_nanos())
        .ok()
        .map(Duration::from_nanos)
}

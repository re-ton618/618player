use std::path::{Path, PathBuf};

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};

use super::track::{Metadata, MetadataState, ReplayGain, TrackUpdate, fallback_title};

/// What R128 gains (Opus) are measured against sits this far under
/// ReplayGain's reference level, in dB.
const R128_TO_REPLAY_GAIN_DB: f64 = 5.0;

#[derive(Debug, Clone)]
pub(super) struct MetadataJob {
    pub(super) id: i64,
    pub(super) path: PathBuf,
}

pub(super) fn extract(job: &MetadataJob) -> TrackUpdate {
    match read(&job.path) {
        Ok(metadata) => TrackUpdate {
            id: job.id,
            metadata,
            metadata_state: MetadataState::Ready,
            metadata_error: None,
        },
        Err(error) => TrackUpdate {
            id: job.id,
            metadata: Metadata {
                title: fallback_title(&job.path),
                ..Metadata::default()
            },
            metadata_state: MetadataState::Failed,
            metadata_error: Some(error.to_string()),
        },
    }
}

fn read(path: &Path) -> lofty::error::Result<Metadata> {
    let options = ParseOptions::new().read_cover_art(false);
    let tagged_file = Probe::open(path)?.options(options).read()?;
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());
    let properties = tagged_file.properties();
    let text = |key: ItemKey| tag.and_then(|tag| tag.get_string(&key)).and_then(clean);

    Ok(Metadata {
        title: tag
            .and_then(Accessor::title)
            .as_deref()
            .and_then(clean)
            .unwrap_or_else(|| fallback_title(path)),
        artist: tag.and_then(Accessor::artist).as_deref().and_then(clean),
        album: tag.and_then(Accessor::album).as_deref().and_then(clean),
        album_artist: text(ItemKey::AlbumArtist),
        genre: text(ItemKey::Genre),
        composer: text(ItemKey::Composer),
        year: tag.and_then(Accessor::year),
        duration_ms: Some(properties.duration().as_millis().min(u64::MAX as u128) as u64),
        track_number: tag.and_then(Accessor::track),
        track_total: tag.and_then(Accessor::track_total),
        disc_number: tag.and_then(Accessor::disk),
        disc_total: tag.and_then(Accessor::disk_total),
        compilation: text(ItemKey::FlagCompilation).is_some_and(|flag| flag != "0"),
        release_id: text(ItemKey::MusicBrainzReleaseId),
        sample_rate: properties.sample_rate(),
        bit_depth: properties.bit_depth(),
        channels: properties.channels(),
        codec: codec(tagged_file.file_type(), properties.bit_depth()).map(str::to_owned),
        bitrate_kbps: properties.audio_bitrate(),
        replay_gain: tag.map(replay_gain).unwrap_or_default(),
    })
}

/// What the stream is coded as, as a listener names it. MP4 holds either
/// AAC or ALAC, and only the lossless one tells a bit depth.
fn codec(file_type: FileType, bit_depth: Option<u8>) -> Option<&'static str> {
    Some(match file_type {
        FileType::Flac => "FLAC",
        FileType::Mpeg => "MP3",
        FileType::Mp4 if bit_depth.is_some() => "ALAC",
        FileType::Mp4 | FileType::Aac => "AAC",
        FileType::Opus => "Opus",
        FileType::Vorbis => "Vorbis",
        FileType::Speex => "Speex",
        FileType::Wav | FileType::Aiff => "PCM",
        FileType::Ape => "Monkey's Audio",
        FileType::WavPack => "WavPack",
        FileType::Mpc => "Musepack",
        _ => return None,
    })
}

fn replay_gain(tag: &Tag) -> ReplayGain {
    let number = |key: ItemKey| tag.get_string(&key).and_then(decibels);
    // Opus files carry R128 gains instead: 1/256 dB against a quieter
    // reference.
    let r128 = |name: &str| {
        tag.get_string(&ItemKey::Unknown(name.to_owned()))
            .and_then(|value| value.trim().parse::<f64>().ok())
            .map(|value| value / 256.0 + R128_TO_REPLAY_GAIN_DB)
    };
    ReplayGain {
        track_gain_db: number(ItemKey::ReplayGainTrackGain).or_else(|| r128("R128_TRACK_GAIN")),
        track_peak: number(ItemKey::ReplayGainTrackPeak),
        album_gain_db: number(ItemKey::ReplayGainAlbumGain).or_else(|| r128("R128_ALBUM_GAIN")),
        album_peak: number(ItemKey::ReplayGainAlbumPeak),
    }
}

/// The number of a ReplayGain tag: "-6.54 dB", "+1.2", "0.988553".
fn decibels(value: &str) -> Option<f64> {
    let value = value.trim();
    let number = value
        .strip_suffix("dB")
        .or_else(|| value.strip_suffix("db"))
        .or_else(|| value.strip_suffix("DB"))
        .unwrap_or(value)
        .trim();
    number
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

fn clean(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
pub(super) mod tests {
    use std::path::Path;

    use lofty::config::WriteOptions;
    use lofty::tag::{ItemKey, Tag, TagExt, TagType};

    use super::*;

    /// Writes a mono 16-bit WAV file of `frames` silent frames.
    pub(in crate::library) fn write_wav(path: &Path, rate: u32, frames: u32) {
        let size = frames * 2;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((36 + size).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16_u32.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(rate.to_le_bytes());
        bytes.extend((rate * 2).to_le_bytes());
        bytes.extend(2_u16.to_le_bytes());
        bytes.extend(16_u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(size.to_le_bytes());
        bytes.resize(bytes.len() + size as usize, 0);
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn replay_gain_numbers_parse_with_and_without_their_unit() {
        assert_eq!(decibels("-6.54 dB"), Some(-6.54));
        assert_eq!(decibels("+1.20dB"), Some(1.2));
        assert_eq!(decibels(" 0.988553 "), Some(0.988553));
        assert_eq!(decibels("loud"), None);
        assert_eq!(decibels("NaN"), None);
    }

    #[test]
    fn mp4_is_named_by_what_it_holds() {
        assert_eq!(codec(FileType::Mp4, Some(24)), Some("ALAC"));
        assert_eq!(codec(FileType::Mp4, None), Some("AAC"));
        assert_eq!(codec(FileType::Flac, Some(16)), Some("FLAC"));
    }

    #[test]
    fn tags_and_stream_properties_are_read_from_a_file() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("song.wav");
        write_wav(&path, 48_000, 24_000);
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title("Song".to_owned());
        tag.set_artist("Soloist".to_owned());
        tag.set_album("Album".to_owned());
        tag.set_genre("Chamber".to_owned());
        tag.set_track(3);
        tag.set_track_total(9);
        tag.set_disk(2);
        tag.set_disk_total(2);
        for (key, value) in [
            (ItemKey::AlbumArtist, "Ensemble"),
            (ItemKey::Composer, "Composer"),
            (ItemKey::FlagCompilation, "1"),
            (ItemKey::ReplayGainTrackGain, "-7.25 dB"),
            (ItemKey::ReplayGainTrackPeak, "0.95"),
            (ItemKey::ReplayGainAlbumGain, "-8.00 dB"),
            (ItemKey::ReplayGainAlbumPeak, "1.0"),
        ] {
            tag.insert_text(key, value.to_owned());
        }
        tag.save_to_path(&path, WriteOptions::default()).unwrap();

        let update = extract(&MetadataJob { id: 7, path });

        assert_eq!(update.metadata_state, MetadataState::Ready);
        let metadata = update.metadata;
        assert_eq!(metadata.title, "Song");
        assert_eq!(metadata.album_artist.as_deref(), Some("Ensemble"));
        assert_eq!(metadata.genre.as_deref(), Some("Chamber"));
        assert_eq!(metadata.composer.as_deref(), Some("Composer"));
        assert_eq!(
            (metadata.track_number, metadata.track_total),
            (Some(3), Some(9))
        );
        assert_eq!(
            (metadata.disc_number, metadata.disc_total),
            (Some(2), Some(2))
        );
        assert!(metadata.compilation);
        assert_eq!(metadata.duration_ms, Some(500));
        assert_eq!(metadata.sample_rate, Some(48_000));
        assert_eq!(metadata.bit_depth, Some(16));
        assert_eq!(metadata.channels, Some(1));
        assert_eq!(metadata.codec.as_deref(), Some("PCM"));
        assert_eq!(metadata.bitrate_kbps, Some(768));
        assert_eq!(
            metadata.replay_gain,
            ReplayGain {
                track_gain_db: Some(-7.25),
                track_peak: Some(0.95),
                album_gain_db: Some(-8.0),
                album_peak: Some(1.0),
            }
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_falls_back_to_its_name() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("Broken Song.flac");
        std::fs::write(&path, b"not audio").unwrap();

        let update = extract(&MetadataJob { id: 1, path });

        assert_eq!(update.metadata_state, MetadataState::Failed);
        assert_eq!(update.metadata.title, "Broken Song");
        assert!(update.metadata_error.is_some());
    }
}

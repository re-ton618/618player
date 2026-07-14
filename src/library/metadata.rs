use std::path::PathBuf;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;

use super::track::{MetadataState, TrackUpdate, fallback_title};

#[derive(Debug, Clone)]
pub(super) struct MetadataJob {
    pub(super) id: i64,
    pub(super) path: PathBuf,
}

pub(super) fn extract(job: &MetadataJob) -> TrackUpdate {
    match extract_inner(job) {
        Ok(update) => update,
        Err(error) => TrackUpdate {
            id: job.id,
            title: fallback_title(&job.path),
            artist: None,
            album: None,
            year: None,
            duration_ms: None,
            track_number: None,
            disc_number: None,
            metadata_state: MetadataState::Failed,
            metadata_error: Some(error.to_string()),
        },
    }
}

fn extract_inner(job: &MetadataJob) -> lofty::error::Result<TrackUpdate> {
    let options = ParseOptions::new().read_cover_art(false);
    let tagged_file = Probe::open(&job.path)?.options(options).read()?;
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    Ok(TrackUpdate {
        id: job.id,
        title: tag
            .and_then(Accessor::title)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| fallback_title(&job.path)),
        artist: tag.and_then(Accessor::artist).and_then(clean),
        album: tag.and_then(Accessor::album).and_then(clean),
        year: tag.and_then(Accessor::year),
        duration_ms: Some(
            tagged_file
                .properties()
                .duration()
                .as_millis()
                .min(u64::MAX as u128) as u64,
        ),
        track_number: tag.and_then(Accessor::track),
        disc_number: tag.and_then(Accessor::disk),
        metadata_state: MetadataState::Ready,
        metadata_error: None,
    })
}

fn clean(value: std::borrow::Cow<'_, str>) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

mod artwork;
mod cache;
mod filter;
mod metadata;
mod scanner;
mod track;

#[cfg(test)]
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) use artwork::load_embedded_data_uri;
pub(crate) use filter::{SortColumn, SortDirection, matching_track_indices};
pub(crate) use track::{Track, TrackUpdate};

use cache::Cache;
use metadata::MetadataJob;

const METADATA_BATCH_SIZE: usize = 64;
const MAX_METADATA_WORKERS: usize = 4;

#[derive(Debug, Clone)]
pub(crate) enum LibraryEvent {
    Cached(Vec<Track>),
    Reconciled(Vec<Track>),
    MetadataBatch(Vec<TrackUpdate>),
    Complete,
    Failed(String),
}

/// Indexes the music directory, reporting each stage through `emit`; stops
/// early once `emit` returns false (nobody listens any more). Blocking: run
/// it on its own thread.
pub(crate) fn index(mut emit: impl FnMut(LibraryEvent) -> bool) {
    let mut cache = match Cache::open() {
        Ok(cache) => cache,
        Err(error) => {
            emit(LibraryEvent::Failed(error.to_string()));
            return;
        }
    };
    let cached = match cache.load_tracks() {
        Ok(tracks) => tracks,
        Err(error) => {
            emit(LibraryEvent::Failed(error.to_string()));
            return;
        }
    };
    if !emit(LibraryEvent::Cached(cached.clone())) {
        return;
    }

    let discovered = match scanner::scan_music_directory() {
        Ok(discovered) => discovered,
        Err(error) => {
            emit(LibraryEvent::Failed(error));
            return;
        }
    };
    let prepared = match cache.prepare_scan(discovered, &cached) {
        Ok(prepared) => prepared,
        Err(error) => {
            emit(LibraryEvent::Failed(error.to_string()));
            return;
        }
    };
    if !emit(LibraryEvent::Reconciled(prepared.tracks)) {
        return;
    }

    for jobs in prepared.metadata_jobs.chunks(METADATA_BATCH_SIZE) {
        let updates = extract_batch(jobs);
        if let Err(error) = cache.store_updates(&updates) {
            emit(LibraryEvent::Failed(error.to_string()));
            return;
        }
        if !emit(LibraryEvent::MetadataBatch(updates)) {
            return;
        }
    }

    emit(LibraryEvent::Complete);
}

fn extract_batch(jobs: &[MetadataJob]) -> Vec<TrackUpdate> {
    if jobs.len() <= 1 {
        return jobs.iter().map(metadata::extract).collect();
    }

    let worker_count = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(MAX_METADATA_WORKERS)
        .min(jobs.len());
    let next_job = AtomicUsize::new(0);
    let updates = Mutex::new(Vec::with_capacity(jobs.len()));

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let index = next_job.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(index) else {
                        break;
                    };
                    updates
                        .lock()
                        .expect("metadata update lock poisoned")
                        .push(metadata::extract(job));
                }
            });
        }
    });

    let mut updates = updates.into_inner().expect("metadata update lock poisoned");
    updates.sort_unstable_by_key(|update| update.id);
    updates
}

#[cfg(test)]
pub(crate) fn test_track(id: i64, path: PathBuf, album: Option<&str>) -> Track {
    Track::from_cache(
        id,
        path.clone(),
        path,
        1,
        1,
        None,
        format!("Track {id}"),
        None,
        album.map(str::to_owned),
        None,
        None,
        None,
        None,
        track::MetadataState::Ready,
        None,
    )
}

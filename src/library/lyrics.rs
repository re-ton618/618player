//! A track's lyrics: from an `.lrc` file of the same name beside it, else
//! from its tags. Timed lines (LRC) are kept with their times, so the
//! line being sung can be followed; plain text is kept as its lines.

use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::ItemKey;

/// The words of a track, a line each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Lyrics {
    pub(crate) lines: Vec<Line>,
    /// Every line has its time: the current one can be told.
    pub(crate) synced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    /// When it starts, in milliseconds; only for synced lyrics.
    pub(crate) at_ms: Option<u64>,
    pub(crate) text: String,
}

impl Lyrics {
    /// The line sung at `position_ms`: the last that has started.
    pub(crate) fn line_at(&self, position_ms: u64) -> Option<usize> {
        if !self.synced {
            return None;
        }
        let started = self
            .lines
            .partition_point(|line| line.at_ms.is_some_and(|at| at <= position_ms));
        started.checked_sub(1)
    }
}

/// The lyrics of the file at `path`, or none when it has none.
pub(crate) fn load(path: &Path) -> Result<Option<Lyrics>, String> {
    if let Some(lyrics) = sidecar(path) {
        return Ok(Some(lyrics));
    }
    let tagged = Probe::open(path)
        .and_then(|probe| {
            probe
                .options(
                    ParseOptions::new()
                        .read_cover_art(false)
                        .read_properties(false),
                )
                .read()
        })
        .map_err(|error| error.to_string())?;
    let keys = [
        ItemKey::Lyrics,
        ItemKey::Unknown("UNSYNCEDLYRICS".to_owned()),
        ItemKey::Unknown("UNSYNCED LYRICS".to_owned()),
    ];
    Ok(tagged
        .tags()
        .iter()
        .flat_map(|tag| keys.iter().filter_map(|key| tag.get_string(key)))
        .find_map(parse))
}

/// An `.lrc` beside the file, its name the file's own.
fn sidecar(path: &Path) -> Option<Lyrics> {
    ["lrc", "LRC"].into_iter().find_map(|extension| {
        let bytes = std::fs::read(path.with_extension(extension)).ok()?;
        parse(&String::from_utf8_lossy(&bytes))
    })
}

/// LRC when any line is timed, else plain lines; none when it is blank.
pub(crate) fn parse(source: &str) -> Option<Lyrics> {
    let source = source.trim_start_matches('\u{feff}');
    let mut offset_ms: i64 = 0;
    let mut timed = Vec::new();
    let mut plain = Vec::new();
    for raw in source.lines() {
        let line = raw.trim();
        let (times, rest) = timestamps(line);
        if times.is_empty() {
            if let Some(value) = tag_value(line, "offset") {
                offset_ms = value.trim().parse().unwrap_or(0);
            } else if !is_id_tag(line) {
                plain.push(line.to_owned());
            }
            continue;
        }
        let text = strip_word_times(rest).trim().to_owned();
        timed.extend(times.into_iter().map(|at| (at, text.clone())));
    }

    if !timed.is_empty() {
        // A positive offset shows the lines sooner.
        timed.sort_by_key(|(at, _)| *at);
        let lines = timed
            .into_iter()
            .map(|(at, text)| Line {
                at_ms: Some(u64::try_from((at as i64 - offset_ms).max(0)).unwrap_or(0)),
                text,
            })
            .collect();
        return Some(Lyrics {
            lines,
            synced: true,
        });
    }

    let first = plain.iter().position(|line| !line.is_empty())?;
    let last = plain.iter().rposition(|line| !line.is_empty())?;
    Some(Lyrics {
        lines: plain[first..=last]
            .iter()
            .map(|text| Line {
                at_ms: None,
                text: text.clone(),
            })
            .collect(),
        synced: false,
    })
}

/// The `[mm:ss.xx]` stamps that lead `line`, and what follows them.
fn timestamps(line: &str) -> (Vec<u64>, &str) {
    let mut times = Vec::new();
    let mut rest = line;
    while let Some(inner) = rest.strip_prefix('[') {
        let Some(end) = inner.find(']') else { break };
        let Some(at) = time(&inner[..end]) else { break };
        times.push(at);
        rest = inner[end + 1..].trim_start();
    }
    (times, rest)
}

/// `mm:ss`, `mm:ss.x`, `mm:ss.xx`, `mm:ss.xxx` or `mm:ss:xx`, in ms.
fn time(stamp: &str) -> Option<u64> {
    let (minutes, rest) = stamp.split_once(':')?;
    let (seconds, fraction) = match rest.split_once(['.', ':']) {
        Some((seconds, fraction)) => (seconds, fraction),
        None => (rest, ""),
    };
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if !digits(minutes) || !digits(seconds) || !(fraction.is_empty() || digits(fraction)) {
        return None;
    }
    let minutes: u64 = minutes.parse().ok()?;
    let seconds: u64 = seconds.parse().ok()?;
    let fraction_ms = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<u64>().ok()? * 100,
        2 => fraction.parse::<u64>().ok()? * 10,
        _ => fraction[..3].parse::<u64>().ok()?,
    };
    Some((minutes * 60 + seconds) * 1_000 + fraction_ms)
}

/// The value of an `[name:value]` line.
fn tag_value<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let (key, value) = inner.split_once(':')?;
    key.trim().eq_ignore_ascii_case(name).then_some(value)
}

/// `[ar:…]`, `[ti:…]` and the like: about the lyrics, not of them.
fn is_id_tag(line: &str) -> bool {
    line.strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .and_then(|inner| inner.split_once(':'))
        .is_some_and(|(key, _)| !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphabetic()))
}

/// Enhanced LRC times each word, `<mm:ss.xx>`: only the words are kept.
fn strip_word_times(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else { break };
        kept.push_str(&rest[..start]);
        let inner = &rest[start + 1..start + end];
        if time(inner).is_none() {
            kept.push_str(&rest[start..=start + end]);
        }
        rest = &rest[start + end + 1..];
    }
    kept.push_str(rest);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lyrics: &Lyrics) -> Vec<(Option<u64>, &str)> {
        lyrics
            .lines
            .iter()
            .map(|line| (line.at_ms, line.text.as_str()))
            .collect()
    }

    #[test]
    fn timed_lines_are_sorted_repeated_and_offset() {
        let lyrics = parse(
            "\u{feff}[ar:Someone]\n[ti:Song]\n[offset:500]\n\
             [00:12.50][01:02.00]Chorus\n[00:05.1]First\n[00:30:25] <00:30.25>Word <00:31.00>by word\n[00:40.00]",
        )
        .unwrap();
        assert!(lyrics.synced);
        assert_eq!(
            texts(&lyrics),
            [
                (Some(4_600), "First"),
                (Some(12_000), "Chorus"),
                (Some(29_750), "Word by word"),
                (Some(39_500), ""),
                (Some(61_500), "Chorus"),
            ]
        );
    }

    #[test]
    fn plain_text_keeps_its_lines_and_drops_blank_edges() {
        let lyrics = parse("\n\nOne\n\nTwo\n\n").unwrap();
        assert!(!lyrics.synced);
        assert_eq!(texts(&lyrics), [(None, "One"), (None, ""), (None, "Two")]);
        assert!(parse("  \n\n").is_none());
        assert!(parse("[ar:Only tags]").is_none());
    }

    #[test]
    fn the_current_line_is_the_last_started() {
        let lyrics = parse("[00:01.00]a\n[00:02.00]b\n[00:03.00]c").unwrap();
        assert_eq!(lyrics.line_at(0), None);
        assert_eq!(lyrics.line_at(1_000), Some(0));
        assert_eq!(lyrics.line_at(2_500), Some(1));
        assert_eq!(lyrics.line_at(99_000), Some(2));
        assert_eq!(parse("plain").unwrap().line_at(5_000), None);
    }

    #[test]
    fn a_sidecar_lrc_is_read_before_the_tags() {
        let dir = tempfile::tempdir().unwrap();
        let track = dir.path().join("song.flac");
        std::fs::write(&track, b"not audio").unwrap();
        std::fs::write(dir.path().join("song.lrc"), "[00:01.00]Hello").unwrap();
        let lyrics = load(&track).unwrap().unwrap();
        assert_eq!(texts(&lyrics), [(Some(1_000), "Hello")]);
    }
}

use std::path::Path;

use nsh_base::theme;
use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;

use crate::app::store::store;
use crate::library::{MetadataState, Track};
use crate::theme::{self as tokens, CHROME_BORDER_WIDTH};
use crate::ui::{caption, hint, strip, warning_strip};

/// The card on the left: the artwork as a square across its width, its
/// right line outside.
const ARTWORK_SIZE: f32 = 240.0;
/// The block under the artwork: inset 12 across, 10 down, its lines 4
/// apart (inside the one block).
const IDENTITY_PADDING_X: f32 = 12.0;
const IDENTITY_PADDING_Y: f32 = 10.0;
const IDENTITY_GAP: f32 = 4.0;
/// The name that leads: semibold 13pt.
const TITLE_TEXT_SIZE: f32 = 13.0;
const BYLINE_TEXT_SIZE: f32 = 12.0;
/// The format line under it: a 9pt tag, semibold and spaced.
const TAG_TEXT_SIZE: f32 = 9.0;
const TAG_LETTER_SPACING: f32 = 0.8;
const PLACEHOLDER_TEXT_SIZE: f32 = 10.0;
/// A section title: 28 and its bottom line, as the library's column
/// titles are.
const HEADER_HEIGHT: f32 = 29.0;
const HEADER_TEXT_SIZE: f32 = 10.0;
const HEADER_LETTER_SPACING: f32 = 0.8;
/// A line of a section: 28 and its bottom line.
const FIELD_HEIGHT: f32 = 29.0;
const FIELD_TEXT_SIZE: f32 = 11.0;
/// The label column: "Album artist", the longest name, is 12 characters
/// at 11pt (79), and its inset.
const LABEL_WIDTH: f32 = 100.0;
const HORIZONTAL_PADDING: f32 = 10.0;

/// What the tab shows of the current track, worked out once per change.
#[derive(Debug, Clone, PartialEq)]
struct Details {
    title: SharedString,
    artist: Option<SharedString>,
    /// The album and its year.
    album: Option<SharedString>,
    /// Codec, rate, depth and channels, as a tag.
    format: SharedString,
    /// What is wrong with what is known of it.
    notice: Option<SharedString>,
    sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq)]
struct Section {
    title: &'static str,
    fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq)]
struct Field {
    name: &'static str,
    value: SharedString,
    /// It may run longer than its cell: a tooltip holds the whole of it.
    long: bool,
}

impl Details {
    fn of(track: &Track) -> Self {
        let named = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let artist = named(&track.artist).or_else(|| named(&track.album_artist));
        let album = named(&track.album);
        let album_line = match (&album, track.year) {
            (Some(album), Some(year)) => Some(format!("{album} \u{b7} {year}")),
            (Some(album), None) => Some(album.clone()),
            (None, Some(year)) => Some(year.to_string()),
            (None, None) => None,
        };
        let channels = track.channels.map(channels);
        let format = [
            track.codec.clone(),
            track.sample_rate.map(sample_rate),
            track.bit_depth.map(|bits| format!("{bits}-bit")),
            channels.clone(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
        .to_uppercase();
        let gain = |db: Option<f64>| db.map(|db| format!("{db:+.2} dB"));
        let peak = |peak: Option<f64>| peak.map(|peak| format!("{peak:.6}"));
        let gains = &track.replay_gain;

        let sections = [
            Section::of(
                "TRACK",
                [
                    ("Title", Some(track.title.clone()), true),
                    ("Artist", named(&track.artist), true),
                    ("Album", album, true),
                    ("Album artist", named(&track.album_artist), true),
                    ("Composer", named(&track.composer), true),
                    ("Genre", named(&track.genre), true),
                    ("Year", track.year.map(|year| year.to_string()), false),
                    (
                        "Track",
                        of_total(track.track_number, track.track_total),
                        false,
                    ),
                    ("Disc", of_total(track.disc_number, track.disc_total), false),
                    ("Length", Some(track.formatted_duration()), false),
                    (
                        "Compilation",
                        track.compilation.then(|| "Yes".to_owned()),
                        false,
                    ),
                    ("Release ID", named(&track.release_id), true),
                ],
            ),
            Section::of(
                "AUDIO",
                [
                    ("Codec", track.codec.clone(), false),
                    ("Sample rate", track.sample_rate.map(sample_rate), false),
                    (
                        "Bit depth",
                        track.bit_depth.map(|bits| format!("{bits}-bit")),
                        false,
                    ),
                    ("Channels", channels, false),
                    (
                        "Bitrate",
                        track.bitrate_kbps.map(|kbps| format!("{kbps} kbps")),
                        false,
                    ),
                ],
            ),
            Section::of(
                "REPLAYGAIN",
                [
                    ("Track gain", gain(gains.track_gain_db), false),
                    ("Track peak", peak(gains.track_peak), false),
                    ("Album gain", gain(gains.album_gain_db), false),
                    ("Album peak", peak(gains.album_peak), false),
                ],
            ),
            Section::of(
                "FILE",
                [
                    ("Name", file_name(&track.path), true),
                    (
                        "Folder",
                        track
                            .path
                            .parent()
                            .map(|folder| folder.display().to_string()),
                        true,
                    ),
                    ("Size", Some(file_size(track.size)), false),
                    ("Modified", modified(track.modified_ns), false),
                    ("Tags", Some(tags(track.metadata_state).to_owned()), false),
                ],
            ),
        ]
        .into_iter()
        .filter(|section| !section.fields.is_empty())
        .collect();

        Self {
            title: track.title.clone().into(),
            artist: artist.map(SharedString::from),
            album: album_line.map(SharedString::from),
            format: format.into(),
            notice: notice(track).map(SharedString::from),
            sections,
        }
    }
}

impl Section {
    /// The section of the fields that have a value.
    fn of<const N: usize>(
        title: &'static str,
        fields: [(&'static str, Option<String>, bool); N],
    ) -> Self {
        let fields = fields
            .into_iter()
            .filter_map(|(name, value, long)| {
                let value = value.filter(|value| !value.is_empty())?;
                Some(Field {
                    name,
                    value: value.into(),
                    long,
                })
            })
            .collect();
        Self { title, fields }
    }
}

/// "3 of 12", or "3" when the total is not known.
fn of_total(number: Option<u32>, total: Option<u32>) -> Option<String> {
    let number = number?;
    Some(match total {
        Some(total) => format!("{number} of {total}"),
        None => number.to_string(),
    })
}

/// Hertz as kilohertz, with only the decimals it needs: "44.1 kHz".
fn sample_rate(hertz: u32) -> String {
    let kilohertz = format!("{:.3}", f64::from(hertz) / 1_000.0);
    let kilohertz = kilohertz.trim_end_matches('0').trim_end_matches('.');
    format!("{kilohertz} kHz")
}

fn channels(count: u8) -> String {
    match count {
        1 => "Mono".to_owned(),
        2 => "Stereo".to_owned(),
        count => format!("{count} channels"),
    }
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// Bytes in the largest binary unit under 1024 of them, to a tenth.
fn file_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1_024 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64 / 1_024.0;
    let mut unit = 0;
    while value >= 1_024.0 && unit + 1 < UNITS.len() {
        value /= 1_024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// When the file was last changed, in UTC: "2023-11-14 22:13 UTC".
fn modified(nanoseconds: i64) -> Option<String> {
    if nanoseconds <= 0 {
        return None;
    }
    let seconds = nanoseconds.div_euclid(1_000_000_000);
    let (days, of_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    Some(format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        of_day / 3_600,
        of_day / 60 % 60
    ))
}

/// The proleptic Gregorian date `days` after 1970-01-01 (Howard
/// Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * of_year + 2) / 153;
    let day = (of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn tags(state: MetadataState) -> &'static str {
    match state {
        MetadataState::Pending => "Being read",
        MetadataState::Ready => "Read",
        MetadataState::Fallback => "None; titled after its file",
        MetadataState::Failed => "Unreadable",
    }
}

fn notice(track: &Track) -> Option<String> {
    match track.metadata_state {
        MetadataState::Pending => Some("Its tags are still being read.".to_owned()),
        MetadataState::Failed => Some(match &track.metadata_error {
            Some(error) => format!("Its tags could not be read: {error}"),
            None => "Its tags could not be read.".to_owned(),
        }),
        MetadataState::Ready | MetadataState::Fallback => None,
    }
}

/// The Song Information tab: a card of the current track's artwork over
/// its name on the left; beside it what its tags, its stream and its file
/// say of it, a section each.
pub(super) fn song_information() -> impl IntoElement {
    let store = store();
    let details = store.select(|model| model.current_track().map(Details::of));
    let artwork = store.select(|model| {
        model
            .current_track()
            .and(model.playback.artwork_image())
            .cloned()
            .unwrap_or_default()
    });

    view()
        .flex_1()
        .min_h(px(0.0))
        .child(dynamic(move || match details.get() {
            Some(details) => body(details, artwork).into_element(),
            None => {
                strip("Nothing is playing. Press a track in the library to see its details here.")
                    .border_b_1()
                    .border_color(move || theme().border)
                    .into_element()
            }
        }))
}

/// The card on the left; on the right, what is wrong, then the sections.
fn body(details: Details, artwork: Memo<SharedString>) -> View {
    let border = move || theme().border;
    let sections = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .when_some(details.notice.clone(), |side, said| {
            side.child(warning_strip(said).border_b_1().border_color(border))
        })
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .w_full()
                    .selectable(true)
                    .children(details.sections.iter().map(section)),
            ),
        );
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_row()
        .child(card(&details, artwork))
        .child(sections)
}

/// The artwork, then under it the track's name, who made it, its album
/// and its format: a column with its right line, each block its bottom
/// line.
fn card(details: &Details, artwork: Memo<SharedString>) -> View {
    let border = move || theme().border;
    let shown = computed(move || artwork.with(|uri| !uri.is_empty()));
    let art = view()
        .w(px(ARTWORK_SIZE))
        .h(px(ARTWORK_SIZE + CHROME_BORDER_WIDTH))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .border_b_1()
        .border_color(border)
        .child(
            when(shown, move || {
                image(move || ImageSrc::from(artwork.get()))
                    .w(px(ARTWORK_SIZE))
                    .h(px(ARTWORK_SIZE))
                    .object_fit(ResizeMode::Cover)
                    .label("Album artwork")
                    .into_element()
            })
            .otherwise(move || {
                text("No artwork")
                    .text_size(px(PLACEHOLDER_TEXT_SIZE))
                    .text_color(tokens::text_faint)
                    .into_element()
            }),
        );

    let artist = details
        .artist
        .clone()
        .unwrap_or_else(|| SharedString::from("Unknown artist"));
    let format = details.format.clone();
    let lines = view()
        .w_full()
        .flex_shrink_0()
        .px(px(IDENTITY_PADDING_X))
        .py(px(IDENTITY_PADDING_Y))
        .gap(px(IDENTITY_GAP))
        .border_b_1()
        .border_color(border)
        .child(cut(
            details.title.clone(),
            TITLE_TEXT_SIZE,
            FontWeight::SEMIBOLD,
            || theme().foreground,
        ))
        .child(cut(artist, BYLINE_TEXT_SIZE, FontWeight::NORMAL, || {
            theme().muted_foreground
        }))
        .when_some(details.album.clone(), |lines, album| {
            lines.child(cut(album, BYLINE_TEXT_SIZE, FontWeight::NORMAL, || {
                theme().muted_foreground
            }))
        })
        .when_some((!format.is_empty()).then_some(format), |lines, format| {
            lines.child(
                caption(format, TAG_TEXT_SIZE)
                    .letter_spacing(AbsoluteLength::from(px(TAG_LETTER_SPACING)))
                    .text_color(move || theme().muted_foreground),
            )
        });

    view()
        .w(px(ARTWORK_SIZE + CHROME_BORDER_WIDTH))
        .h_full()
        .flex_shrink_0()
        .overflow_hidden()
        .border_r_1()
        .border_color(border)
        .child(art)
        .child(lines)
}

/// One line of text that truncates, with the whole of it in a tooltip.
fn cut(value: SharedString, size: f32, weight: FontWeight, color: fn() -> AdaptiveColor) -> View {
    view()
        .w_full()
        .min_w(px(0.0))
        .tooltip(hint(value.clone()))
        .child(
            text(value)
                .text_size(px(size))
                .font_weight(weight)
                .number_of_lines(1)
                .text_color(color),
        )
}

/// A section's title, then its fields as lines with bottom edges.
fn section(section: &Section) -> View {
    view()
        .w_full()
        .flex_shrink_0()
        .child(
            view()
                .h(px(HEADER_HEIGHT))
                .flex_row()
                .items_center()
                .px(px(HORIZONTAL_PADDING))
                .bg(move || theme().card)
                .border_b_1()
                .border_color(move || theme().border)
                .child(
                    caption(section.title, HEADER_TEXT_SIZE)
                        .letter_spacing(AbsoluteLength::from(px(HEADER_LETTER_SPACING)))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .children(section.fields.iter().map(field))
}

/// A line of a section: its name in the label column, its value in the
/// cell after it, which owns the seam between them.
fn field(field: &Field) -> View {
    let border = move || theme().border;
    let value = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .justify_center()
        .px(px(HORIZONTAL_PADDING))
        .border_l_1()
        .border_color(border)
        .child(
            text(field.value.clone())
                .text_size(px(FIELD_TEXT_SIZE))
                .number_of_lines(1)
                .tabular_nums()
                .text_color(move || theme().foreground),
        );
    let value = if field.long {
        value.tooltip(hint(field.value.clone()))
    } else {
        value
    };

    view()
        .h(px(FIELD_HEIGHT))
        .w_full()
        .flex_row()
        .border_b_1()
        .border_color(border)
        .role(AccessibilityRole::Text)
        .label(format!("{}: {}", field.name, field.value))
        .child(
            view()
                .w(px(LABEL_WIDTH))
                .h_full()
                .flex_shrink_0()
                .justify_center()
                .px(px(HORIZONTAL_PADDING))
                .child(
                    text(field.name)
                        .text_size(px(FIELD_TEXT_SIZE))
                        .number_of_lines(1)
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(value)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::library::test_track;

    fn values(details: &Details, title: &str) -> Vec<(&'static str, String)> {
        details
            .sections
            .iter()
            .find(|section| section.title == title)
            .map(|section| {
                section
                    .fields
                    .iter()
                    .map(|field| (field.name, field.value.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn shows_what_is_known_and_leaves_out_what_is_not() {
        let mut track = test_track(1, PathBuf::from("/music/album/01.flac"), Some("Album"));
        track.artist = Some("Artist".to_owned());
        track.year = Some(1999);
        track.track_number = Some(3);
        track.track_total = Some(12);
        track.disc_number = Some(1);
        track.codec = Some("FLAC".to_owned());
        track.sample_rate = Some(44_100);
        track.bit_depth = Some(16);
        track.channels = Some(2);
        track.replay_gain.track_gain_db = Some(-6.4268);

        let details = Details::of(&track);
        assert_eq!(details.artist.as_deref(), Some("Artist"));
        assert_eq!(details.album.as_deref(), Some("Album \u{b7} 1999"));
        assert_eq!(
            &*details.format,
            "FLAC \u{b7} 44.1 KHZ \u{b7} 16-BIT \u{b7} STEREO"
        );

        let track_fields = values(&details, "TRACK");
        assert!(track_fields.contains(&("Track", "3 of 12".to_owned())));
        assert!(track_fields.contains(&("Disc", "1".to_owned())));
        assert!(!track_fields.iter().any(|(name, _)| *name == "Composer"));
        assert_eq!(
            values(&details, "REPLAYGAIN"),
            vec![("Track gain", "-6.43 dB".to_owned())]
        );
        assert!(values(&details, "FILE").contains(&("Name", "01.flac".to_owned())));
    }

    #[test]
    fn a_section_with_nothing_in_it_is_left_out() {
        let track = test_track(1, PathBuf::from("/music/01.flac"), None);
        let details = Details::of(&track);
        assert!(details
            .sections
            .iter()
            .all(|section| section.title != "REPLAYGAIN"));
        assert!(details.album.is_none());
    }

    #[test]
    fn figures_read_as_people_write_them() {
        assert_eq!(sample_rate(44_100), "44.1 kHz");
        assert_eq!(sample_rate(48_000), "48 kHz");
        assert_eq!(sample_rate(22_050), "22.05 kHz");
        assert_eq!(channels(6), "6 channels");
        assert_eq!(file_size(512), "512 bytes");
        assert_eq!(file_size(8_812_345), "8.4 MB");
        assert_eq!(modified(1), Some("1970-01-01 00:00 UTC".to_owned()));
        assert_eq!(
            modified(1_700_000_000 * 1_000_000_000),
            Some("2023-11-14 22:13 UTC".to_owned())
        );
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }
}

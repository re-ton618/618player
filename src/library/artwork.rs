use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::TaggedFileExt;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::Tag;

/// The embedded cover of `path` as a `data:` URI an image view can show.
pub(crate) fn load_embedded_data_uri(path: &Path) -> lofty::error::Result<Option<String>> {
    Ok(embedded(path)?.map(|picture| {
        let mime = picture
            .mime_type()
            .map_or("application/octet-stream", MimeType::as_str);
        data_uri(mime, picture.data())
    }))
}

/// The embedded cover of `path`, its bytes as the file holds them (the
/// system's media controls read them).
#[cfg(target_os = "windows")]
pub(crate) fn load_embedded_picture(path: &Path) -> lofty::error::Result<Option<Vec<u8>>> {
    Ok(embedded(path)?.map(Picture::into_data))
}

fn embedded(path: &Path) -> lofty::error::Result<Option<Picture>> {
    let tagged_file = Probe::open(path)?
        .options(
            ParseOptions::new()
                .read_cover_art(true)
                .read_properties(false),
        )
        .read()?;
    Ok(preferred_picture(tagged_file.tags())
        .filter(|picture| !picture.data().is_empty())
        .cloned())
}

fn data_uri(mime: &str, bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut uri = String::with_capacity(mime.len() + 13 + bytes.len().div_ceil(3) * 4);
    uri.push_str("data:");
    uri.push_str(mime);
    uri.push_str(";base64,");
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &byte)| n | (u32::from(byte) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                uri.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                uri.push('=');
            }
        }
    }
    uri
}

fn preferred_picture(tags: &[Tag]) -> Option<&Picture> {
    tags.iter()
        .find_map(|tag| tag.get_picture_type(PictureType::CoverFront))
        .or_else(|| tags.iter().find_map(|tag| tag.pictures().first()))
}

#[cfg(test)]
mod tests {
    use lofty::tag::TagType;

    use super::*;

    #[test]
    fn later_front_cover_wins_over_earlier_other_picture() {
        let mut earlier = Tag::new(TagType::VorbisComments);
        earlier.push_picture(Picture::new_unchecked(
            PictureType::Other,
            Some(MimeType::Png),
            None,
            vec![1],
        ));
        let mut later = Tag::new(TagType::VorbisComments);
        later.push_picture(Picture::new_unchecked(
            PictureType::CoverFront,
            Some(MimeType::Jpeg),
            None,
            vec![2],
        ));

        let tags = [earlier, later];
        assert_eq!(preferred_picture(&tags).map(Picture::data), Some(&[2][..]));
    }

    #[test]
    fn first_picture_is_used_without_a_front_cover() {
        let mut first = Tag::new(TagType::VorbisComments);
        first.push_picture(Picture::new_unchecked(
            PictureType::Other,
            Some(MimeType::Png),
            None,
            vec![1],
        ));
        let mut second = Tag::new(TagType::VorbisComments);
        second.push_picture(Picture::new_unchecked(
            PictureType::Other,
            Some(MimeType::Jpeg),
            None,
            vec![2],
        ));

        let tags = [first, second];
        assert_eq!(preferred_picture(&tags).map(Picture::data), Some(&[1][..]));
    }

    #[test]
    fn data_uris_are_padded_base64() {
        assert_eq!(data_uri("image/png", b""), "data:image/png;base64,");
        assert_eq!(data_uri("image/png", b"f"), "data:image/png;base64,Zg==");
        assert_eq!(data_uri("image/png", b"fo"), "data:image/png;base64,Zm8=");
        assert_eq!(
            data_uri("image/jpeg", b"foobar"),
            "data:image/jpeg;base64,Zm9vYmFy"
        );
    }

    #[test]
    fn empty_tags_have_no_artwork() {
        assert!(preferred_picture(&[]).is_none());
        assert!(preferred_picture(&[Tag::new(TagType::VorbisComments)]).is_none());
    }
}

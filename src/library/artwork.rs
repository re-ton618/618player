use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::TaggedFileExt;
use lofty::picture::{Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::Tag;

pub(crate) fn load_embedded(path: &Path) -> lofty::error::Result<Option<Vec<u8>>> {
    let tagged_file = Probe::open(path)?
        .options(
            ParseOptions::new()
                .read_cover_art(true)
                .read_properties(false),
        )
        .read()?;

    Ok(preferred_picture(tagged_file.tags())
        .map(Picture::data)
        .filter(|data| !data.is_empty())
        .map(<[u8]>::to_vec))
}

fn preferred_picture(tags: &[Tag]) -> Option<&Picture> {
    tags.iter()
        .find_map(|tag| tag.get_picture_type(PictureType::CoverFront))
        .or_else(|| tags.iter().find_map(|tag| tag.pictures().first()))
}

#[cfg(test)]
mod tests {
    use lofty::picture::MimeType;
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
    fn empty_tags_have_no_artwork() {
        assert!(preferred_picture(&[]).is_none());
        assert!(preferred_picture(&[Tag::new(TagType::VorbisComments)]).is_none());
    }
}

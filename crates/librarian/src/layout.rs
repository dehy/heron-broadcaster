// SPDX-License-Identifier: AGPL-3.0-or-later

//! Where a file goes in the library, from its tags.
//!
//! A layout is a path template, `{album_artist}/{album}/{disc-}{track:02}
//! {title}.{ext}` by default. Fields:
//!
//! | Field | Value |
//! |---|---|
//! | `{album_artist}` | album artist, else artist; `Compilations` for various-artists albums |
//! | `{artist}` | artist, else album artist |
//! | `{album}`, `{title}`, `{year}`, `{ext}` | `title` defaults to the original file name |
//! | `{track}`, `{disc}` | numbers; `{track:02}` pads to two digits |
//! | `{disc-}` | `2-` on multi-disc albums, nothing otherwise |
//!
//! Missing artists and albums give `Unknown Artist` and `Unknown Album`.
//! Every path component is made safe on every file system.

use heron_library::Tags;

const UNKNOWN_ARTIST: &str = "Unknown Artist";
const UNKNOWN_ALBUM: &str = "Unknown Album";
const COMPILATIONS: &str = "Compilations";
const VARIOUS_ARTISTS: &str = "Various Artists";

/// Longest path component, in bytes, on common file systems.
const MAX_COMPONENT_BYTES: usize = 255;

/// Replaced in names: not allowed on Windows, SMB or FAT file systems.
const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Names Windows reserves, whatever the extension.
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LayoutError {
    #[error("unknown field {{{0}}} in the layout")]
    UnknownField(String),
    #[error("unclosed {{ in the layout")]
    Unclosed,
    #[error("the layout is empty")]
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Segment {
    Text(String),
    Field { field: Field, width: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    AlbumArtist,
    Artist,
    Album,
    Title,
    Track,
    Disc,
    DiscDash,
    Year,
    Ext,
}

/// A parsed layout template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// One list of segments per path component.
    components: Vec<Vec<Segment>>,
}

/// What a file is named after: its tags, and its original name.
pub struct FileFacts<'a> {
    pub tags: &'a Tags,
    /// File name without the extension, used when there is no title.
    pub original_stem: &'a str,
    /// Without the dot. Lowercased in the result.
    pub extension: &'a str,
}

impl Layout {
    pub fn parse(template: &str) -> Result<Self, LayoutError> {
        let mut template = template.trim().trim_matches('/').to_owned();
        if template.is_empty() {
            return Err(LayoutError::Empty);
        }
        if !template.contains("{ext}") {
            template.push_str(".{ext}");
        }
        let components = template
            .split('/')
            .filter(|component| !component.is_empty())
            .map(parse_component)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { components })
    }

    /// The station's layout, or the default one if it is invalid.
    pub fn from_settings(settings: &heron_core::LibrarySettings) -> Self {
        match Self::parse(&settings.layout) {
            Ok(layout) => layout,
            Err(error) => {
                tracing::error!(%error, layout = settings.layout, "invalid layout, using the default");
                match Self::parse(heron_core::LibrarySettings::DEFAULT_LAYOUT) {
                    Ok(layout) => layout,
                    Err(_) => unreachable!("the default layout parses"),
                }
            }
        }
    }

    /// The path of a file in the library, with `/` separators. Collisions
    /// with existing files are resolved by the caller, see [`with_suffix`].
    pub fn render(&self, facts: &FileFacts<'_>) -> String {
        let last = self.components.len() - 1;
        self.components
            .iter()
            .enumerate()
            .map(|(index, segments)| {
                let raw: String = segments
                    .iter()
                    .map(|segment| match segment {
                        Segment::Text(text) => text.clone(),
                        Segment::Field { field, width } => value(*field, *width, facts),
                    })
                    .collect();
                if index == last {
                    file_name(&raw, 1)
                } else {
                    safe_component(&raw)
                }
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}

fn parse_component(component: &str) -> Result<Vec<Segment>, LayoutError> {
    let mut segments = Vec::new();
    let mut rest = component;
    while let Some(start) = rest.find('{') {
        if start > 0 {
            segments.push(Segment::Text(rest[..start].to_owned()));
        }
        let end = rest[start..].find('}').ok_or(LayoutError::Unclosed)? + start;
        let spec = &rest[start + 1..end];
        if spec.contains('{') {
            return Err(LayoutError::Unclosed);
        }
        let (name, width) = match spec.split_once(':') {
            Some((name, width)) => (name, width.parse().unwrap_or(0)),
            None => (spec, 0),
        };
        let field = match name {
            "album_artist" => Field::AlbumArtist,
            "artist" => Field::Artist,
            "album" => Field::Album,
            "title" => Field::Title,
            "track" => Field::Track,
            "disc" => Field::Disc,
            "disc-" => Field::DiscDash,
            "year" => Field::Year,
            "ext" => Field::Ext,
            other => return Err(LayoutError::UnknownField(other.to_owned())),
        };
        segments.push(Segment::Field { field, width });
        rest = &rest[end + 1..];
    }
    if !rest.is_empty() {
        segments.push(Segment::Text(rest.to_owned()));
    }
    Ok(segments)
}

/// Whether the tags describe a various-artists compilation.
pub fn is_compilation(tags: &Tags) -> bool {
    tags.compilation
        || tags
            .album_artist
            .as_deref()
            .is_some_and(|artist| artist.eq_ignore_ascii_case(VARIOUS_ARTISTS))
}

fn value(field: Field, width: usize, facts: &FileFacts<'_>) -> String {
    let tags = facts.tags;
    let number =
        |number: Option<u32>| number.map_or_else(String::new, |number| format!("{number:0width$}"));
    match field {
        Field::AlbumArtist if is_compilation(tags) => COMPILATIONS.to_owned(),
        Field::AlbumArtist => tags
            .album_artist
            .clone()
            .or_else(|| tags.artist.clone())
            .unwrap_or_else(|| UNKNOWN_ARTIST.to_owned()),
        Field::Artist => tags
            .artist
            .clone()
            .or_else(|| tags.album_artist.clone())
            .unwrap_or_else(|| UNKNOWN_ARTIST.to_owned()),
        Field::Album => tags
            .album
            .clone()
            .unwrap_or_else(|| UNKNOWN_ALBUM.to_owned()),
        Field::Title => tags
            .title
            .clone()
            .unwrap_or_else(|| facts.original_stem.to_owned()),
        Field::Track => number(tags.track),
        Field::Disc => number(tags.disc),
        Field::DiscDash => {
            let several = match tags.disc_total {
                Some(total) => total > 1,
                None => tags.disc.is_some_and(|disc| disc > 1),
            };
            match tags.disc {
                Some(disc) if several => format!("{disc:0width$}-"),
                _ => String::new(),
            }
        }
        Field::Year => tags.year().map(|year| year.to_string()).unwrap_or_default(),
        Field::Ext => facts.extension.to_ascii_lowercase(),
    }
    // Field values never create directories: `AC/DC` stays one name.
    .replace('/', "_")
}

/// Makes `raw` a safe directory name.
pub fn safe_component(raw: &str) -> String {
    let mut name = clean(raw);
    truncate(&mut name, MAX_COMPONENT_BYTES);
    finish(name)
}

/// Makes `raw` (`stem.ext`) a safe file name. A `suffix` above 1 is added
/// as ` (2)` before the extension, to avoid a collision.
fn file_name(raw: &str, suffix: u32) -> String {
    let (stem, extension) = match raw.rsplit_once('.') {
        Some((stem, extension)) if !extension.is_empty() && !extension.contains(' ') => {
            (stem, Some(clean(extension)))
        }
        _ => (raw, None),
    };
    let suffix = if suffix > 1 {
        format!(" ({suffix})")
    } else {
        String::new()
    };
    let extension = extension.map(|extension| format!(".{extension}"));
    let tail_len = suffix.len() + extension.as_ref().map_or(0, String::len);

    let mut stem = clean(stem);
    truncate(&mut stem, MAX_COMPONENT_BYTES.saturating_sub(tail_len));
    let mut stem = finish(stem);
    if is_reserved(&stem) {
        stem.push('_');
    }
    stem.push_str(&suffix);
    if let Some(extension) = extension {
        stem.push_str(&extension);
    }
    stem
}

/// `path` with ` (n)` added to the file name.
pub fn with_suffix(path: &str, suffix: u32) -> String {
    match path.rsplit_once('/') {
        Some((dirs, name)) => format!("{dirs}/{}", file_name(name, suffix)),
        None => file_name(path, suffix),
    }
}

fn clean(raw: &str) -> String {
    let replaced: String = raw
        .chars()
        .map(|c| {
            if FORBIDDEN.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    // Collapse runs of whitespace left by missing fields.
    replaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(name: &mut String, max_bytes: usize) {
    if name.len() > max_bytes {
        let mut end = max_bytes;
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        name.truncate(end);
    }
}

/// Trailing dots and spaces are dropped (Windows strips them), a leading dot
/// would hide the file from the scanner, and empty names get a placeholder.
fn finish(name: String) -> String {
    let mut name = name.trim_end_matches(['.', ' ']).trim_start().to_owned();
    if name.starts_with('.') {
        name.replace_range(..1, "_");
    }
    if name.is_empty() {
        name.push('_');
    }
    if is_reserved(&name) {
        name.push('_');
    }
    name
}

fn is_reserved(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or(name);
    RESERVED
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(base.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(layout: &str, tags: &Tags) -> String {
        Layout::parse(layout).unwrap().render(&FileFacts {
            tags,
            original_stem: "track05",
            extension: "MP3",
        })
    }

    fn default_layout(tags: &Tags) -> String {
        render(heron_core::LibrarySettings::DEFAULT_LAYOUT, tags)
    }

    fn album_track() -> Tags {
        Tags {
            title: Some("So What".into()),
            artist: Some("Miles Davis".into()),
            album: Some("Kind of Blue".into()),
            track: Some(1),
            disc: Some(1),
            disc_total: Some(1),
            date: Some("1959".into()),
            ..Tags::default()
        }
    }

    #[test]
    fn files_by_album_artist_album_and_track() {
        assert_eq!(
            default_layout(&album_track()),
            "Miles Davis/Kind of Blue/01 So What.mp3"
        );
        assert_eq!(
            render("{artist}/{year} - {album}/{track} {title}", &album_track()),
            "Miles Davis/1959 - Kind of Blue/1 So What.mp3"
        );
    }

    #[test]
    fn multi_disc_albums_get_the_disc_number() {
        let tags = Tags {
            disc: Some(2),
            disc_total: Some(2),
            track: Some(7),
            ..album_track()
        };
        assert_eq!(
            default_layout(&tags),
            "Miles Davis/Kind of Blue/2-07 So What.mp3"
        );
        let unknown_total = Tags {
            disc_total: None,
            ..tags
        };
        assert_eq!(
            default_layout(&unknown_total),
            "Miles Davis/Kind of Blue/2-07 So What.mp3"
        );
    }

    #[test]
    fn compilations_and_missing_tags() {
        let compilation = Tags {
            album_artist: Some("Various Artists".into()),
            album: Some("Now 42".into()),
            ..album_track()
        };
        assert_eq!(
            default_layout(&compilation),
            "Compilations/Now 42/01 So What.mp3"
        );
        assert_eq!(
            default_layout(&Tags::default()),
            "Unknown Artist/Unknown Album/track05.mp3"
        );
    }

    #[test]
    fn names_are_safe_everywhere() {
        let tags = Tags {
            artist: Some("AC/DC".into()),
            album: Some("...Baby One More Time".into()),
            title: Some("What? <Really> \"yes\": a|b*c\\d  ".into()),
            track: None,
            ..album_track()
        };
        assert_eq!(
            default_layout(&tags),
            "AC_DC/_..Baby One More Time/What_ _Really_ _yes__ a_b_c_d.mp3"
        );
        let dots = Tags {
            album: Some("Vol. 2...".into()),
            title: Some("CON".into()),
            ..album_track()
        };
        assert_eq!(default_layout(&dots), "Miles Davis/Vol. 2/01 CON.mp3");
        let reserved = Tags {
            album: Some("nul".into()),
            ..album_track()
        };
        assert_eq!(default_layout(&reserved), "Miles Davis/nul_/01 So What.mp3");
    }

    #[test]
    fn components_fit_in_255_bytes() {
        let tags = Tags {
            album: Some("é".repeat(300)),
            title: Some("ü".repeat(300)),
            ..album_track()
        };
        let path = default_layout(&tags);
        for component in path.split('/') {
            assert!(component.len() <= 255, "{} bytes", component.len());
        }
        assert!(path.ends_with(".mp3"));
        let suffixed = with_suffix(&path, 12);
        let name = suffixed.rsplit('/').next().unwrap();
        assert!(name.len() <= 255 && name.ends_with("ü (12).mp3"), "{name}");
    }

    #[test]
    fn collisions_get_a_number() {
        assert_eq!(with_suffix("A/B/01 X.mp3", 2), "A/B/01 X (2).mp3");
        assert_eq!(with_suffix("x.flac", 3), "x (3).flac");
    }

    #[test]
    fn bad_layouts_are_rejected() {
        assert_eq!(
            Layout::parse("{genre}/{title}"),
            Err(LayoutError::UnknownField("genre".into()))
        );
        assert_eq!(Layout::parse("{album"), Err(LayoutError::Unclosed));
        assert_eq!(Layout::parse("//"), Err(LayoutError::Empty));
    }
}

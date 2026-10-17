// SPDX-License-Identifier: AGPL-3.0-or-later

//! Names as rotation compares them.
//!
//! - Artist names are matched case-insensitively, ignoring accents and
//!   extra spaces: [`normalize`].
//! - An artist tag is split into several names only on "featuring"
//!   (`feat.`, `ft.`, `featuring`): `Simon & Garfunkel` stays one name.
//!   Featured artists written in the title (`Song (feat. X)`) count too.
//! - Titles and albums are compared without their bracketed parts, so
//!   `Song (Remastered 2011)` repeats `Song`.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Lowercase, without accents, with typographic quotes and dashes made
/// plain and spaces collapsed. Empty when nothing is left.
pub fn normalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut space = false;
    for c in name.nfkd().filter(|c| !is_combining_mark(*c)) {
        let c = match c {
            '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' | '`' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201F}' | '\u{2033}' => '"',
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            c => c,
        };
        if c.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// Ways of writing "featuring", lowercase, with the spaces around them.
const FEATURING: [&str; 5] = [" featuring ", " feat. ", " feat ", " ft. ", " ft "];

/// Byte range of the first "featuring" marker in `text`, ignoring case.
fn find_featuring(text: &str) -> Option<(usize, usize)> {
    // ASCII lowercasing keeps byte offsets; an opening bracket counts as
    // a space (`A (feat. B)`).
    let lower: String = text
        .chars()
        .map(|c| match c {
            '(' | '[' => ' ',
            c => c.to_ascii_lowercase(),
        })
        .collect();
    FEATURING
        .iter()
        .filter_map(|marker| lower.find(marker).map(|at| (at, at + marker.len())))
        .min()
}

fn push_name(names: &mut Vec<String>, name: &str) {
    let name = name
        .trim()
        .trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']'))
        .trim();
    if !normalize(name).is_empty()
        && !names
            .iter()
            .any(|known| normalize(known) == normalize(name))
    {
        names.push(name.to_owned());
    }
}

/// The artist names of a track, as written, main artist first: the artist
/// tag split on "featuring", then the artists featured in the title.
pub fn artist_names(artist: Option<&str>, title: Option<&str>) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(artist) = artist {
        let mut rest = artist;
        while let Some((start, end)) = find_featuring(rest) {
            push_name(&mut names, &rest[..start]);
            rest = &rest[end..];
        }
        push_name(&mut names, rest);
    }
    if let Some(title) = title {
        for (open, close) in [('(', ')'), ('[', ']')] {
            let mut rest = title;
            while let Some(at) = rest.find(open) {
                let inside = &rest[at + 1..];
                let end = inside.find(close).unwrap_or(inside.len());
                let group = format!(" {}", &inside[..end]);
                if let Some((0, marker_end)) = find_featuring(&group) {
                    push_name(&mut names, &group[marker_end..]);
                }
                rest = &inside[end..];
            }
        }
    }
    names
}

/// Key under which titles (or albums) count as the same: normalized,
/// without bracketed parts. `None` for an empty title.
pub fn title_key(title: Option<&str>) -> Option<String> {
    let title = title?;
    let mut bare = String::with_capacity(title.len());
    let mut depth = 0_u32;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            c if depth == 0 => bare.push(c),
            _ => {}
        }
    }
    let key = normalize(&bare);
    let key = if key.is_empty() {
        normalize(title)
    } else {
        key
    };
    (!key.is_empty()).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_accents_and_spaces() {
        assert_eq!(normalize("  Beyoncé   Knowles "), "beyonce knowles");
        assert_eq!(normalize("Guns N’ Roses"), "guns n' roses");
        assert_eq!(normalize("MØ"), "mø");
        assert_eq!(normalize("Sigur Rós"), normalize("SIGUR ROS"));
        assert_eq!(normalize("   "), "");
    }

    #[test]
    fn splits_on_featuring_only() {
        assert_eq!(
            artist_names(Some("Neal Morse feat. Mike Portnoy"), None),
            ["Neal Morse", "Mike Portnoy"]
        );
        assert_eq!(
            artist_names(Some("A Ft. B featuring C"), None),
            ["A", "B", "C"]
        );
        assert_eq!(
            artist_names(Some("Simon & Garfunkel"), None),
            ["Simon & Garfunkel"]
        );
        assert_eq!(
            artist_names(Some("Earth, Wind & Fire"), None),
            ["Earth, Wind & Fire"]
        );
        // "Feat" inside a word is not a marker.
        assert_eq!(artist_names(Some("Defeat Theft"), None), ["Defeat Theft"]);
        assert_eq!(artist_names(Some("A (feat. B)"), None), ["A", "B"]);
    }

    #[test]
    fn finds_artists_featured_in_the_title() {
        assert_eq!(
            artist_names(
                Some("Daft Punk"),
                Some("Get Lucky (feat. Pharrell Williams)")
            ),
            ["Daft Punk", "Pharrell Williams"]
        );
        assert_eq!(
            artist_names(Some("A feat. B"), Some("Song [ft. b] (Remix)")),
            ["A", "B"]
        );
        assert_eq!(
            artist_names(None, Some("Song (Live)")),
            Vec::<String>::new()
        );
    }

    #[test]
    fn titles_compare_without_brackets() {
        assert_eq!(
            title_key(Some("Wish You Were Here (Remastered 2011)")),
            title_key(Some("wish you were here"))
        );
        assert_eq!(title_key(Some("(Untitled)")), Some("(untitled)".to_owned()));
        assert_eq!(title_key(Some(" ")), None);
        assert_eq!(title_key(None), None);
    }
}

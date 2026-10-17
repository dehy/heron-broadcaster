// SPDX-License-Identifier: AGPL-3.0-or-later

//! Picking the release of each file from its AcoustID matches.
//!
//! A recording is usually on several releases (the album, compilations,
//! singles, reissues). Files dropped together are siblings: the files of one
//! album should agree on the release. So the release matching the most
//! files of a group is picked first, then the next one for the files left,
//! and so on. A pick is applied only when it is unambiguous:
//!
//! - one release covers more files than any other; or
//! - among the tied releases, exactly one is titled like the files' album
//!   tag; or
//! - the tied releases are editions of the same release group that put every
//!   file at the same disc and track number (the tags written are then the
//!   same, whichever is picked: the earliest one is).
//!
//! Anything else needs review.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

/// One place a matched recording appears on a release.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Candidate {
    pub score: f64,
    pub recording_id: String,
    pub release_id: String,
    pub release_title: Option<String>,
    pub release_group_id: Option<String>,
    pub release_group_title: Option<String>,
    /// `YYYY-MM-DD`, `YYYY-MM` or `YYYY`; sorts by date.
    pub date: Option<String>,
    pub country: Option<String>,
    pub disc: Option<u32>,
    pub track: Option<u32>,
}

/// The confident matches of one file.
#[derive(Clone, Debug, Default)]
pub struct FileMatches {
    pub album_tag: Option<String>,
    pub candidates: Vec<Candidate>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    Release {
        release_id: String,
        recording_id: String,
        score: f64,
        disc: Option<u32>,
        track: Option<u32>,
    },
    Review(String),
}

pub fn choose(files: &[FileMatches]) -> Vec<Choice> {
    let mut choices: Vec<Option<Choice>> = vec![None; files.len()];
    let mut remaining: HashSet<usize> = HashSet::new();
    for (index, file) in files.iter().enumerate() {
        if file.candidates.is_empty() {
            choices[index] = Some(Choice::Review("no confident AcoustID match".into()));
        } else {
            remaining.insert(index);
        }
    }

    while !remaining.is_empty() {
        let mut coverage: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut sorted: Vec<usize> = remaining.iter().copied().collect();
        sorted.sort_unstable();
        for &index in &sorted {
            let releases: HashSet<&str> = files[index]
                .candidates
                .iter()
                .map(|candidate| candidate.release_id.as_str())
                .collect();
            for release in releases {
                coverage.entry(release).or_default().push(index);
            }
        }
        let Some(best) = coverage.values().map(Vec::len).max() else {
            break;
        };
        let mut top: Vec<&str> = coverage
            .iter()
            .filter(|(_, covered)| covered.len() == best)
            .map(|(release, _)| *release)
            .collect();
        top.sort_unstable();

        if top.len() > 1 {
            let narrowed = by_album_tag(files, &coverage, &top);
            if !narrowed.is_empty() {
                top = narrowed;
            }
        }
        let chosen = if top.len() == 1 {
            Some(top[0])
        } else {
            equivalent_editions(files, &coverage, &top)
        };

        match chosen {
            Some(release) => {
                for &index in &coverage[release] {
                    choices[index] = Some(place(&files[index], release));
                    remaining.remove(&index);
                }
            }
            None => {
                let mut covered: Vec<usize> = top
                    .iter()
                    .flat_map(|release| coverage[release].iter().copied())
                    .collect();
                covered.sort_unstable();
                covered.dedup();
                for index in covered {
                    choices[index] = Some(Choice::Review(format!(
                        "matches {} releases, none clearly better",
                        top.len()
                    )));
                    remaining.remove(&index);
                }
            }
        }
    }
    choices
        .into_iter()
        .map(|choice| choice.unwrap_or_else(|| Choice::Review("not matched".into())))
        .collect()
}

/// The file's place on the chosen release.
fn place(file: &FileMatches, release: &str) -> Choice {
    let on_release: Vec<&Candidate> = file
        .candidates
        .iter()
        .filter(|candidate| candidate.release_id == release)
        .collect();
    let positions: HashSet<(Option<u32>, Option<u32>, &str)> = on_release
        .iter()
        .map(|c| (c.disc, c.track, c.recording_id.as_str()))
        .collect();
    if positions.len() > 1 {
        return Choice::Review("matches several tracks of the release".into());
    }
    let Some(best) = on_release.iter().max_by(|a, b| a.score.total_cmp(&b.score)) else {
        return Choice::Review("not matched".into());
    };
    Choice::Release {
        release_id: release.to_owned(),
        recording_id: best.recording_id.clone(),
        score: best.score,
        disc: best.disc,
        track: best.track,
    }
}

/// The tied releases titled like most of the files' album tags.
fn by_album_tag<'a>(
    files: &[FileMatches],
    coverage: &HashMap<&str, Vec<usize>>,
    top: &[&'a str],
) -> Vec<&'a str> {
    let scores: Vec<(&str, usize)> = top
        .iter()
        .map(|&release| {
            let agreeing = coverage[release]
                .iter()
                .filter(|&&index| {
                    let Some(tag) = files[index].album_tag.as_deref().map(normalize) else {
                        return false;
                    };
                    files[index].candidates.iter().any(|candidate| {
                        candidate.release_id == release
                            && [&candidate.release_title, &candidate.release_group_title]
                                .iter()
                                .any(|title| title.as_deref().map(normalize).as_ref() == Some(&tag))
                    })
                })
                .count();
            (release, agreeing)
        })
        .collect();
    let best = scores
        .iter()
        .map(|(_, agreeing)| *agreeing)
        .max()
        .unwrap_or(0);
    if best == 0 {
        return Vec::new();
    }
    scores
        .into_iter()
        .filter(|(_, agreeing)| *agreeing == best)
        .map(|(release, _)| release)
        .collect()
}

/// The earliest of the tied releases when they are editions of one release
/// group placing every file at the same disc and track.
fn equivalent_editions<'a>(
    files: &[FileMatches],
    coverage: &HashMap<&str, Vec<usize>>,
    top: &[&'a str],
) -> Option<&'a str> {
    let first = top.first()?;
    let group = |release: &str| -> Option<String> {
        files
            .iter()
            .flat_map(|file| &file.candidates)
            .find(|candidate| candidate.release_id == release)
            .and_then(|candidate| candidate.release_group_id.clone())
    };
    let group_of_first = group(first)?;
    if top
        .iter()
        .any(|release| group(release).as_ref() != Some(&group_of_first))
    {
        return None;
    }
    let covered = &coverage[first];
    if top.iter().any(|release| &coverage[release] != covered) {
        return None;
    }
    for &index in covered {
        let positions: HashSet<(Option<u32>, Option<u32>)> = files[index]
            .candidates
            .iter()
            .filter(|candidate| top.contains(&candidate.release_id.as_str()))
            .map(|candidate| (candidate.disc, candidate.track))
            .collect();
        if positions.len() != 1 {
            return None;
        }
    }
    let date = |release: &str| -> String {
        files
            .iter()
            .flat_map(|file| &file.candidates)
            .find(|candidate| candidate.release_id == release)
            .and_then(|candidate| candidate.date.clone())
            // Releases without a date sort last.
            .unwrap_or_else(|| "9999".into())
    };
    top.iter()
        .copied()
        .min_by(|a, b| date(a).cmp(&date(b)).then_with(|| a.cmp(b)))
}

/// Lowercase letters and digits only, to compare titles loosely.
fn normalize(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(release: &str, group: &str, title: &str, track: u32) -> Candidate {
        Candidate {
            score: 0.97,
            recording_id: format!("rec-{track}"),
            release_id: release.into(),
            release_title: Some(title.into()),
            release_group_id: Some(group.into()),
            release_group_title: Some(title.into()),
            date: Some("1999-01-01".into()),
            country: None,
            disc: Some(1),
            track: Some(track),
        }
    }

    fn file(album: Option<&str>, candidates: Vec<Candidate>) -> FileMatches {
        FileMatches {
            album_tag: album.map(Into::into),
            candidates,
        }
    }

    fn release_of(choice: &Choice) -> &str {
        match choice {
            Choice::Release { release_id, .. } => release_id,
            Choice::Review(reason) => panic!("needs review: {reason}"),
        }
    }

    #[test]
    fn siblings_pick_their_album_over_a_compilation() {
        // Track 1 is also on a compilation; the album has both tracks.
        let files = vec![
            file(
                None,
                vec![
                    candidate("album", "g1", "Album", 1),
                    candidate("best-of", "g2", "Best Of", 7),
                ],
            ),
            file(None, vec![candidate("album", "g1", "Album", 2)]),
        ];
        let choices = choose(&files);
        assert_eq!(release_of(&choices[0]), "album");
        assert_eq!(release_of(&choices[1]), "album");
        assert!(matches!(choices[0], Choice::Release { track: Some(1), .. }));
    }

    #[test]
    fn a_lone_file_on_several_releases_needs_review_unless_its_album_tag_says() {
        let candidates = vec![
            candidate("album", "g1", "Album", 3),
            candidate("single", "g2", "The Single", 1),
        ];
        let lone = choose(&[file(None, candidates.clone())]);
        assert!(matches!(lone[0], Choice::Review(_)), "{lone:?}");
        let tagged = choose(&[file(Some("the single"), candidates)]);
        assert_eq!(release_of(&tagged[0]), "single");
    }

    #[test]
    fn editions_of_one_release_group_are_equivalent() {
        let mut reissue = candidate("reissue", "g1", "Album", 3);
        reissue.date = Some("2011-05-01".into());
        let original = candidate("original", "g1", "Album", 3);
        let choices = choose(&[file(None, vec![reissue, original])]);
        assert_eq!(release_of(&choices[0]), "original", "the earliest");

        // Different track numbers: not equivalent.
        let a = candidate("a", "g1", "Album", 3);
        let b = candidate("b", "g1", "Album", 4);
        assert!(matches!(
            choose(&[file(None, vec![a, b])])[0],
            Choice::Review(_)
        ));
    }

    #[test]
    fn files_without_matches_need_review_and_others_still_resolve() {
        let files = vec![
            file(None, vec![]),
            file(None, vec![candidate("album", "g1", "Album", 1)]),
        ];
        let choices = choose(&files);
        assert!(matches!(choices[0], Choice::Review(_)));
        assert_eq!(release_of(&choices[1]), "album");
    }

    #[test]
    fn a_recording_twice_on_the_release_needs_review() {
        let mut bonus = candidate("album", "g1", "Album", 12);
        bonus.recording_id = "rec-1".into();
        let choices = choose(&[file(
            None,
            vec![candidate("album", "g1", "Album", 1), bonus],
        )]);
        assert!(matches!(choices[0], Choice::Review(_)), "{choices:?}");
    }
}

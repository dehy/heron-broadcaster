// SPDX-License-Identifier: AGPL-3.0-or-later

//! Turns each minute's listener list into sessions: continues the sessions
//! of listeners still connected, closes those of listeners gone, opens new
//! ones, and splits sessions at midnight.

use std::collections::HashSet;

use jiff::civil::Date;

/// A listener who reconnects this soon after they were last seen continues
/// their session: they were gone at most about a minute, given that the
/// list is read once a minute.
pub const RECONNECT_MS: i64 = 120_000;
/// Slack on the connection start, which the server gives to the second.
const START_SLACK_MS: i64 = 2_000;

pub type ListenerKey = [u8; 32];

/// The listener key: the address and player, keyed with the day's key.
pub fn listener_key(day_key: &[u8; 32], address: &str, user_agent: &str) -> ListenerKey {
    let mut hasher = blake3::Hasher::new_keyed(day_key);
    hasher.update(address.as_bytes());
    hasher.update(b"\n");
    hasher.update(user_agent.as_bytes());
    *hasher.finalize().as_bytes()
}

/// A stored session that may go on: open, or closed a moment ago.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Known {
    pub id: u64,
    pub key: ListenerKey,
    pub user_agent: String,
    pub client_id: u64,
    pub day: Date,
    pub started_ms: i64,
    pub last_seen_ms: i64,
    pub open: bool,
}

/// A listener of this minute's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub client_id: u64,
    /// With today's key.
    pub key: ListenerKey,
    pub user_agent: String,
    /// When the server says the connection started.
    pub connected_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSession {
    pub key: ListenerKey,
    pub user_agent: String,
    pub client_id: u64,
    pub started_ms: i64,
}

/// What to write.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Seen now (reopened if they were closed), with their connection's ID.
    pub continued: Vec<(u64, u64)>,
    /// Gone: they end when they were last seen.
    pub closed: Vec<u64>,
    /// Sessions of an earlier day still connected: they end at midnight.
    pub split: Vec<u64>,
    pub opened: Vec<NewSession>,
}

/// Matches this minute's listeners with the known sessions of one output.
pub fn reconcile(
    known: &[Known],
    seen: &[Seen],
    now_ms: i64,
    today: Date,
    today_start_ms: i64,
) -> Changes {
    let mut changes = Changes::default();
    let mut taken = HashSet::new();
    let mut unmatched = Vec::new();
    // The same connection: its ID, player and start fit. After a server
    // restart IDs start again, but the new connections started after the
    // session was last seen.
    for listener in seen {
        let same = known.iter().find(|session| {
            session.open
                && !taken.contains(&session.id)
                && session.client_id == listener.client_id
                && session.user_agent == listener.user_agent
                && listener.connected_ms <= session.last_seen_ms + START_SLACK_MS
        });
        match same {
            Some(session) => {
                taken.insert(session.id);
                if session.day < today {
                    changes.split.push(session.id);
                    changes.opened.push(NewSession {
                        key: listener.key,
                        user_agent: listener.user_agent.clone(),
                        client_id: listener.client_id,
                        started_ms: today_start_ms,
                    });
                } else {
                    changes.continued.push((session.id, listener.client_id));
                }
            }
            None => unmatched.push(listener),
        }
    }
    // A reconnection: the same listener today, back soon after the end of
    // a connection that is gone.
    let gone: HashSet<u64> = known
        .iter()
        .filter(|session| session.open && !taken.contains(&session.id))
        .map(|session| session.id)
        .collect();
    for listener in unmatched {
        let earlier = known
            .iter()
            .filter(|session| {
                (gone.contains(&session.id) || !session.open)
                    && !taken.contains(&session.id)
                    && session.day == today
                    && session.key == listener.key
                    && listener.connected_ms - session.last_seen_ms <= RECONNECT_MS
            })
            .max_by_key(|session| session.last_seen_ms);
        match earlier {
            Some(session) => {
                taken.insert(session.id);
                changes.continued.push((session.id, listener.client_id));
            }
            None => changes.opened.push(NewSession {
                key: listener.key,
                user_agent: listener.user_agent.clone(),
                client_id: listener.client_id,
                started_ms: listener.connected_ms.max(today_start_ms).min(now_ms),
            }),
        }
    }
    changes.closed = known
        .iter()
        .filter(|session| session.open && !taken.contains(&session.id))
        .map(|session| session.id)
        .collect();
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;
    const DAY: Date = jiff::civil::date(2026, 10, 25);

    fn key(n: u8) -> ListenerKey {
        [n; 32]
    }

    fn known(id: u64, n: u8, client_id: u64, started_ms: i64, last_seen_ms: i64) -> Known {
        Known {
            id,
            key: key(n),
            user_agent: format!("player {n}"),
            client_id,
            day: DAY,
            started_ms,
            last_seen_ms,
            open: true,
        }
    }

    fn seen(n: u8, client_id: u64, connected_ms: i64) -> Seen {
        Seen {
            client_id,
            key: key(n),
            user_agent: format!("player {n}"),
            connected_ms,
        }
    }

    #[test]
    fn keys_depend_on_the_day_key() {
        let one = listener_key(&[1; 32], "192.0.2.1", "VLC");
        assert_eq!(one, listener_key(&[1; 32], "192.0.2.1", "VLC"));
        assert_ne!(one, listener_key(&[2; 32], "192.0.2.1", "VLC"));
        assert_ne!(one, listener_key(&[1; 32], "192.0.2.1", "mpv"));
        assert_ne!(one, listener_key(&[1; 32], "192.0.2.2", "VLC"));
    }

    #[test]
    fn opens_continues_and_closes() {
        let now = 10 * MIN;
        let sessions = [known(1, 1, 7, 0, 9 * MIN), known(2, 2, 8, 0, 9 * MIN)];
        let changes = reconcile(
            &sessions,
            &[seen(1, 7, 0), seen(3, 9, now - 30_000)],
            now,
            DAY,
            0,
        );
        assert_eq!(changes.continued, [(1, 7)]);
        assert_eq!(changes.closed, [2]);
        assert!(changes.split.is_empty());
        assert_eq!(
            changes.opened,
            [NewSession {
                key: key(3),
                user_agent: "player 3".to_owned(),
                client_id: 9,
                started_ms: now - 30_000,
            }]
        );
    }

    #[test]
    fn a_quick_reconnection_continues_the_session() {
        let now = 10 * MIN;
        // Gone and back between two lists: a new connection, same listener.
        let open = [known(1, 1, 7, 0, 9 * MIN)];
        let changes = reconcile(&open, &[seen(1, 8, now - 20_000)], now, DAY, 0);
        assert_eq!(changes.continued, [(1, 8)]);
        assert!(changes.closed.is_empty() && changes.opened.is_empty());
        // Closed at the previous list, back within a minute or so.
        let mut closed = known(1, 1, 7, 0, 8 * MIN);
        closed.open = false;
        let changes = reconcile(
            std::slice::from_ref(&closed),
            &[seen(1, 8, 9 * MIN + 30_000)],
            now,
            DAY,
            0,
        );
        assert_eq!(changes.continued, [(1, 8)]);
        // Back much later: a new session.
        closed.last_seen_ms = 2 * MIN;
        let changes = reconcile(&[closed], &[seen(1, 8, 9 * MIN)], now, DAY, 0);
        assert!(changes.continued.is_empty());
        assert_eq!(changes.opened.len(), 1);
    }

    #[test]
    fn a_restarted_server_gives_new_sessions() {
        let now = 10 * MIN;
        // Same ID and player, but the connection started after the session
        // was last seen, long ago: a new listener on a restarted server.
        let open = [known(1, 1, 1, 0, 3 * MIN)];
        let other = Seen {
            key: key(9),
            ..seen(1, 1, 9 * MIN)
        };
        let changes = reconcile(&open, &[other], now, DAY, 0);
        assert_eq!(changes.closed, [1]);
        assert_eq!(changes.opened.len(), 1);
    }

    #[test]
    fn splits_at_midnight() {
        let midnight = 24 * 60 * MIN;
        let now = midnight + 30_000;
        let mut yesterday = known(1, 1, 7, midnight - 5 * MIN, midnight - 30_000);
        yesterday.day = DAY.yesterday().unwrap();
        // Today's key differs: the connection is matched by its ID.
        let today = Seen {
            key: key(5),
            ..seen(1, 7, midnight - 5 * MIN)
        };
        let changes = reconcile(&[yesterday], &[today], now, DAY, midnight);
        assert_eq!(changes.split, [1]);
        assert_eq!(changes.opened[0].started_ms, midnight);
        assert_eq!(changes.opened[0].key, key(5));
        assert!(changes.closed.is_empty() && changes.continued.is_empty());
        // A listener connected since yesterday, first seen today.
        let changes = reconcile(&[], &[seen(2, 3, midnight - 60 * MIN)], now, DAY, midnight);
        assert_eq!(changes.opened[0].started_ms, midnight);
    }
}

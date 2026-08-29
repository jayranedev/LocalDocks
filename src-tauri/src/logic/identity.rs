//! Reading a process identity back apart.
//!
//! `{pid}-{startedAt}` is easy to build and slightly awkward to parse, because
//! the timestamp contains hyphens of its own:
//!
//! ```text
//! 8420-2026-08-28T09:00:00.000Z
//!     ^ the only separator that matters
//! ```
//!
//! So the split is at the *first* hyphen, not the last and not all of them.
//! Getting that wrong yields a PID of 8420 and a start time of
//! `2026`, which would then fail to match and silently refuse every action.
//!
//! Pure and syscall-free: every destructive command begins by parsing an
//! identity it was handed by the frontend, and that parse must be impossible
//! to trick.

use crate::models::ProcessId;

/// A process identity, taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedIdentity {
    pub pid: u32,
    /// The creation time exactly as the snapshot rendered it. Compared as a
    /// string against a freshly formatted reading, never parsed into a date —
    /// both sides come from `time::to_iso8601`, so equality is exact and there
    /// is no timezone or precision question to get wrong.
    pub started_at: String,
}

/// Parse `{pid}-{startedAt}`, rejecting anything that is not exactly that.
///
/// Returns `None` rather than a partial result: a malformed identity is a
/// caller bug or a tampered value, and the only safe response is to do nothing.
pub fn parse(id: &ProcessId) -> Option<ParsedIdentity> {
    let (pid, started_at) = id.split_once('-')?;

    // `str::parse::<u32>` accepts a leading `+`, which would make `+8420-...`
    // a second spelling of the same identity. Identities are generated, never
    // typed, so exactly one spelling is correct.
    if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let pid: u32 = pid.parse().ok()?;

    // PID 0 is the Idle process: not a real process, never openable, and never
    // something a command should act on.
    if pid == 0 || started_at.is_empty() {
        return None;
    }

    Some(ParsedIdentity {
        pid,
        started_at: started_at.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::make_process_id;

    #[test]
    fn a_generated_identity_round_trips() {
        let id = make_process_id(8420, "2026-08-28T09:00:00.000Z");
        let parsed = parse(&id).expect("a generated identity must parse");

        assert_eq!(parsed.pid, 8420);
        assert_eq!(parsed.started_at, "2026-08-28T09:00:00.000Z");
    }

    #[test]
    fn the_split_is_at_the_first_hyphen_not_the_last() {
        // The timestamp's own hyphens must survive. Splitting at the last one
        // would give a start time of "28T09:00:00.000Z".
        let parsed = parse(&"8420-2026-08-28T09:00:00.000Z".to_string()).unwrap();
        assert_eq!(parsed.started_at, "2026-08-28T09:00:00.000Z");
        assert!(parsed.started_at.starts_with("2026-08-28"));
    }

    #[test]
    fn every_pid_width_parses() {
        for pid in [1u32, 4, 8420, 65535, 4_294_967_295] {
            let id = make_process_id(pid, "2026-08-28T09:00:00.000Z");
            assert_eq!(parse(&id).unwrap().pid, pid);
        }
    }

    #[test]
    fn a_malformed_identity_is_rejected_rather_than_guessed() {
        for bad in [
            "",                             // empty
            "8420",                         // no separator
            "-2026-08-28T09:00:00.000Z",    // no pid
            "8420-",                        // no timestamp
            "abc-2026-08-28T09:00:00.000Z", // pid not a number
            "84 20-2026-08-28T09:00:00.000Z",
            "0-2026-08-28T09:00:00.000Z", // the Idle process
        ] {
            assert!(
                parse(&bad.to_string()).is_none(),
                "{bad:?} should not parse"
            );
        }
    }

    #[test]
    fn a_pid_with_a_sign_or_padding_is_not_a_second_spelling() {
        // `str::parse::<u32>` would accept "+8420". Identities are generated,
        // so exactly one spelling is correct and anything else is tampering.
        for bad in [
            "+8420-2026-08-28T09:00:00.000Z",
            " 8420-2026-08-28T09:00:00.000Z",
        ] {
            assert!(
                parse(&bad.to_string()).is_none(),
                "{bad:?} should not parse"
            );
        }
    }

    #[test]
    fn a_pid_too_large_for_the_type_is_rejected() {
        assert!(parse(&"4294967296-2026-08-28T09:00:00.000Z".to_string()).is_none());
        assert!(parse(&"99999999999999999999-2026-08-28T09:00:00.000Z".to_string()).is_none());
    }

    #[test]
    fn the_timestamp_is_carried_verbatim_and_not_interpreted() {
        // Whatever the snapshot rendered is what gets compared later. This
        // function is not a date parser and must not become one.
        let parsed = parse(&"8420-not-a-timestamp".to_string()).unwrap();
        assert_eq!(parsed.started_at, "not-a-timestamp");
    }
}

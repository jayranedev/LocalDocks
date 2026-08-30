//! Release-channel policy.
//!
//! Pure and syscall-free, like everything else in `logic/`. Given the version
//! that is installed and the version a release feed is advertising, this
//! decides whether the user is offered an update — and nothing else. No
//! network, no filesystem, no plugin.
//!
//! It exists as its own module because the policy is the part that can be
//! wrong in a way nobody notices until an update ships. The updater plugin
//! does its own semver comparison before it will install anything; this runs
//! first, on our side, so the two rules that matter here are ours and are
//! tested:
//!
//!   1. **Never downgrade.** A feed advertising an older version is ignored,
//!      not obeyed. This is not hypothetical: `latest.json` is a static asset,
//!      and a botched release, a cached CDN copy or a hand-edited file can all
//!      advertise a version behind the one already installed.
//!   2. **Never offer a prerelease on the stable channel.** GitHub's
//!      `/releases/latest` already excludes prereleases, so this is the second
//!      of two independent guards rather than the only one. A release marked
//!      stable by mistake would slip past GitHub; it does not slip past this.
//!
//! There is deliberately no channel *system*. There is one channel, and the
//! rule for it is "stable releases only". A prerelease channel would need a
//! second feed, a setting, and a story for moving between them — none of which
//! buys anything until there are prereleases worth distributing.

use semver::Version;

/// What to do with a version a release feed advertised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Newer, stable, parseable. Offer it.
    Offer,
    /// Nothing to do. The user is current, or ahead.
    Ignore(Reason),
}

/// Why an advertised version was not offered.
///
/// Carried rather than discarded so a log line can say which rule applied.
/// "No update" and "the feed is broken" look identical to a user and must not
/// look identical to whoever is reading the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Same version. The overwhelmingly common case.
    AlreadyCurrent,
    /// The feed is behind the installed build. Never act on this.
    WouldDowngrade,
    /// A prerelease — `0.9.1-beta`, `0.9.1-rc.1` — on the stable channel.
    Prerelease,
    /// The installed version is not semver. Should be impossible: it comes
    /// from `Cargo.toml` via Tauri. If it ever happens, do nothing.
    UnreadableCurrent,
    /// The feed said something that is not a version.
    UnreadableCandidate,
}

impl Reason {
    /// A short phrase for a log line. Not user-facing copy.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyCurrent => "already current",
            Self::WouldDowngrade => "candidate is older than the installed version",
            Self::Prerelease => "candidate is a prerelease and this is the stable channel",
            Self::UnreadableCurrent => "the installed version is not valid semver",
            Self::UnreadableCandidate => "the feed advertised something that is not a version",
        }
    }
}

/// Decide whether `candidate` should be offered to someone running `current`.
///
/// Build metadata (`+build.7`) is ignored, which is what semver says it means:
/// two versions differing only in build metadata are the same release.
pub fn decide(current: &str, candidate: &str) -> Decision {
    let Ok(current) = Version::parse(current.trim()) else {
        return Decision::Ignore(Reason::UnreadableCurrent);
    };
    let Ok(candidate) = Version::parse(candidate.trim()) else {
        return Decision::Ignore(Reason::UnreadableCandidate);
    };

    // Checked before the ordering comparison, deliberately. `0.9.1-rc.1` is
    // greater than `0.9.0` by semver, so ordering alone would offer it.
    if !candidate.pre.is_empty() {
        return Decision::Ignore(Reason::Prerelease);
    }

    // Compare on the release triple only, so `0.9.1+build.7` is not treated as
    // newer than `0.9.1`.
    let current = (current.major, current.minor, current.patch, current.pre);
    let candidate = (
        candidate.major,
        candidate.minor,
        candidate.patch,
        candidate.pre,
    );

    match candidate.cmp(&current) {
        std::cmp::Ordering::Greater => Decision::Offer,
        std::cmp::Ordering::Equal => Decision::Ignore(Reason::AlreadyCurrent),
        std::cmp::Ordering::Less => Decision::Ignore(Reason::WouldDowngrade),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ignored(current: &str, candidate: &str) -> Reason {
        match decide(current, candidate) {
            Decision::Ignore(reason) => reason,
            Decision::Offer => panic!("{candidate} should not have been offered to {current}"),
        }
    }

    // --- the three cases the release brief names by number -----------------

    #[test]
    fn a_newer_patch_is_offered() {
        assert_eq!(decide("0.9.0", "0.9.1"), Decision::Offer);
    }

    #[test]
    fn the_same_version_offers_nothing() {
        assert_eq!(ignored("0.9.1", "0.9.1"), Reason::AlreadyCurrent);
    }

    #[test]
    fn an_older_version_is_never_a_downgrade() {
        assert_eq!(ignored("0.9.2", "0.9.1"), Reason::WouldDowngrade);
    }

    // --- prereleases -------------------------------------------------------

    #[test]
    fn a_prerelease_is_never_offered_on_the_stable_channel() {
        for candidate in ["0.9.1-beta", "0.9.1-rc.1", "1.0.0-alpha.2", "2.0.0-0"] {
            assert_eq!(
                ignored("0.9.0", candidate),
                Reason::Prerelease,
                "{candidate} reached the stable channel",
            );
        }
    }

    #[test]
    fn a_prerelease_that_is_newer_by_ordering_is_still_refused() {
        // The trap: semver puts 0.9.1-rc.1 above 0.9.0, so an ordering-only
        // check offers it. This is why the prerelease guard runs first.
        assert!(
            semver::Version::parse("0.9.1-rc.1").unwrap()
                > semver::Version::parse("0.9.0").unwrap()
        );
        assert_eq!(ignored("0.9.0", "0.9.1-rc.1"), Reason::Prerelease);
    }

    #[test]
    fn a_stable_release_reaches_someone_running_its_own_prerelease() {
        // Running 0.9.1-rc.1 and 0.9.1 ships: that is an upgrade, and someone
        // on a release candidate is exactly who should get it.
        assert_eq!(decide("0.9.1-rc.1", "0.9.1"), Decision::Offer);
    }

    // --- malformed feeds ---------------------------------------------------

    #[test]
    fn a_feed_that_is_not_a_version_changes_nothing() {
        for candidate in ["", "latest", "v0.9.1", "0.9", "0.9.1.4", "<html>", "  "] {
            assert_eq!(
                ignored("0.9.0", candidate),
                Reason::UnreadableCandidate,
                "{candidate:?} was parsed as a version",
            );
        }
    }

    #[test]
    fn an_unreadable_installed_version_offers_nothing() {
        // Belt and braces: this cannot happen, because the installed version
        // comes from Cargo.toml through Tauri. If it ever does, the safe
        // behaviour is to do nothing rather than to guess.
        assert_eq!(ignored("not-a-version", "0.9.1"), Reason::UnreadableCurrent);
    }

    #[test]
    fn surrounding_whitespace_is_tolerated() {
        assert_eq!(decide("0.9.0", " 0.9.1\n"), Decision::Offer);
    }

    // --- ordering that is easy to get wrong --------------------------------

    #[test]
    fn build_metadata_alone_is_not_a_new_release() {
        assert_eq!(ignored("0.9.1", "0.9.1+build.7"), Reason::AlreadyCurrent);
    }

    #[test]
    fn versions_compare_numerically_not_as_text() {
        // "0.9.10" sorts before "0.9.9" as a string. It must not here.
        assert_eq!(decide("0.9.9", "0.9.10"), Decision::Offer);
        assert_eq!(ignored("0.9.10", "0.9.9"), Reason::WouldDowngrade);
        assert_eq!(decide("0.9.0", "0.10.0"), Decision::Offer);
        assert_eq!(ignored("0.10.0", "0.9.0"), Reason::WouldDowngrade);
    }

    #[test]
    fn a_major_release_is_offered_like_any_other() {
        assert_eq!(decide("0.9.0", "1.0.0"), Decision::Offer);
    }

    #[test]
    fn every_ignore_reason_can_explain_itself() {
        for reason in [
            Reason::AlreadyCurrent,
            Reason::WouldDowngrade,
            Reason::Prerelease,
            Reason::UnreadableCurrent,
            Reason::UnreadableCandidate,
        ] {
            assert!(!reason.as_str().is_empty());
        }
    }
}

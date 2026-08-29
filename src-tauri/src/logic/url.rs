//! Deciding whether a URL may be handed to the operating system.
//!
//! `open_external` ends in a call that asks Windows to launch whatever handler
//! is registered for a scheme. That is a powerful thing to point at a string,
//! so the string is checked here first, in a pure function with tests, rather
//! than at the call site.
//!
//! The rule is an allowlist of two schemes and a set of structural refusals.
//! An allowlist is the only defensible shape: a blocklist of `javascript:`,
//! `file:`, `shell:` and friends is a guess about what exists, and Windows lets
//! any installed application register a new scheme at any time.
//!
//! In practice the only URLs the app ever produces are
//! `http://localhost:{port}` from `localUrl()` in the frontend. This validates
//! anyway, because "the caller is trusted" is exactly the assumption that turns
//! a UI bug into a code-execution bug.

/// The only schemes that may be opened.
const ALLOWED: [&str; 2] = ["http://", "https://"];

/// A generous ceiling. Real URLs are far shorter; this exists so a pathological
/// string cannot be passed to the OS at all.
const MAX_LENGTH: usize = 2048;

/// Why a URL was refused. Carried into the error so the message names the
/// actual problem instead of "invalid URL".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlRejection {
    Empty,
    TooLong,
    /// Not `http://` or `https://`.
    UnsupportedScheme,
    /// Contains a control character, whitespace, or a NUL.
    IllegalCharacter,
    /// `http://` with nothing after it.
    MissingHost,
}

impl UrlRejection {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Empty => "the URL is empty",
            Self::TooLong => "the URL is longer than 2048 characters",
            Self::UnsupportedScheme => "only http:// and https:// URLs can be opened",
            Self::IllegalCharacter => "the URL contains whitespace or a control character",
            Self::MissingHost => "the URL has no host",
        }
    }
}

/// Check a URL, returning it unchanged if it may be opened.
///
/// Deliberately a validator and not a parser. Nothing here needs to understand
/// the URL — it needs to be certain the string is an ordinary web URL before
/// the OS is allowed to interpret it. Rewriting or normalising it would mean
/// opening something the caller did not ask for.
pub fn validate(url: &str) -> Result<&str, UrlRejection> {
    if url.is_empty() {
        return Err(UrlRejection::Empty);
    }
    if url.len() > MAX_LENGTH {
        return Err(UrlRejection::TooLong);
    }

    // Checked before the scheme so that `java\nscript:` or an embedded NUL is
    // refused on its own terms rather than sliding through some later check.
    // A NUL matters especially: the string becomes a NUL-terminated wide
    // string on the way to Win32, so anything after one would be invisible
    // here and absent there.
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(UrlRejection::IllegalCharacter);
    }

    // Scheme comparison is ASCII-case-insensitive because schemes are, but the
    // rest of the URL is passed through byte-for-byte.
    let scheme = ALLOWED
        .iter()
        .find(|s| url.len() >= s.len() && url[..s.len()].eq_ignore_ascii_case(s))
        .ok_or(UrlRejection::UnsupportedScheme)?;

    if url.len() == scheme.len() {
        return Err(UrlRejection::MissingHost);
    }

    // `http:///path` has an empty authority. Windows would treat it as a local
    // path, which is the kind of surprise this function exists to prevent.
    if url[scheme.len()..].starts_with('/') {
        return Err(UrlRejection::MissingHost);
    }

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_urls_the_app_actually_produces_are_allowed() {
        // `localUrl()` in the frontend builds exactly this shape.
        for url in [
            "http://localhost:5173",
            "http://localhost:80",
            "http://127.0.0.1:8000",
            "http://[::1]:3000",
        ] {
            assert_eq!(validate(url), Ok(url), "{url} should be allowed");
        }
    }

    #[test]
    fn https_is_allowed() {
        assert!(validate("https://localhost:5173").is_ok());
        assert!(validate("https://example.com/a/b?c=d#e").is_ok());
    }

    #[test]
    fn the_scheme_check_is_case_insensitive_but_changes_nothing_else() {
        assert!(validate("HTTP://localhost:5173").is_ok());
        assert!(validate("HtTpS://localhost:5173").is_ok());
        // The string is returned verbatim: no normalising, no rewriting.
        assert_eq!(
            validate("HTTP://LocalHost:5173/Path"),
            Ok("HTTP://LocalHost:5173/Path")
        );
    }

    #[test]
    fn every_other_scheme_is_refused() {
        for url in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "file:///C:/Windows/System32/cmd.exe",
            "file://server/share",
            "shell:startup",
            "ms-settings:privacy",
            "vbscript:msgbox",
            "data:text/html,<script>alert(1)</script>",
            "ftp://example.com",
            "mailto:someone@example.com",
            "steam://run/570",
            "\\\\server\\share",
            "C:\\Windows\\System32\\cmd.exe",
            "cmd.exe",
            "//example.com",
            "localhost:5173",
        ] {
            assert_eq!(
                validate(url),
                Err(UrlRejection::UnsupportedScheme),
                "{url} must be refused"
            );
        }
    }

    #[test]
    fn a_scheme_hidden_behind_a_control_character_is_refused() {
        // The reason control characters are checked before the scheme.
        for url in [
            "java\nscript:alert(1)",
            "java\tscript:alert(1)",
            " http://localhost:5173",
            "http://localhost:5173\n",
            "http://local host:5173",
        ] {
            assert_eq!(
                validate(url),
                Err(UrlRejection::IllegalCharacter),
                "{url:?} must be refused"
            );
        }
    }

    #[test]
    fn an_embedded_nul_is_refused_because_win32_would_not_see_past_it() {
        // Everything after a NUL would vanish on the way to Windows, so the
        // string checked here would not be the string opened.
        assert_eq!(
            validate("http://localhost:5173\0javascript:alert(1)"),
            Err(UrlRejection::IllegalCharacter)
        );
        assert_eq!(
            validate("http://good.example\0"),
            Err(UrlRejection::IllegalCharacter)
        );
    }

    #[test]
    fn a_url_with_no_host_is_refused() {
        for url in ["http://", "https://", "http:///etc/passwd", "https:///"] {
            assert_eq!(
                validate(url),
                Err(UrlRejection::MissingHost),
                "{url} must be refused"
            );
        }
    }

    #[test]
    fn empty_and_oversized_input_is_refused() {
        assert_eq!(validate(""), Err(UrlRejection::Empty));

        let long = format!("http://localhost/{}", "a".repeat(MAX_LENGTH));
        assert_eq!(validate(&long), Err(UrlRejection::TooLong));

        // Just under the ceiling is still fine.
        let ok = format!("http://localhost/{}", "a".repeat(MAX_LENGTH - 20));
        assert!(validate(&ok).is_ok());
    }

    #[test]
    fn a_prefix_that_merely_looks_like_the_scheme_is_refused() {
        for url in [
            "http:/localhost",
            "http:localhost",
            "httpx://localhost",
            "xhttp://localhost",
        ] {
            assert!(validate(url).is_err(), "{url} must be refused");
        }
    }

    #[test]
    fn every_rejection_can_explain_itself() {
        for r in [
            UrlRejection::Empty,
            UrlRejection::TooLong,
            UrlRejection::UnsupportedScheme,
            UrlRejection::IllegalCharacter,
            UrlRejection::MissingHost,
        ] {
            assert!(!r.reason().is_empty());
        }
    }
}

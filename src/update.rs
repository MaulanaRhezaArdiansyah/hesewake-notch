//! Update check: asks GitHub for the latest release (at startup and every 24 hours, or from the menu) and tells the
//! person once per version when a newer one is out. Nothing is downloaded or installed.

use serde_json::Value;

/// The latest published release (drafts and pre-releases are never returned by this endpoint).
pub(crate) const LATEST_URL: &str = "https://api.github.com/repos/dirgaastya/hesewake-notch/releases/latest";
/// Only release pages of this repository are opened.
const RELEASES_PREFIX: &str = "https://github.com/dirgaastya/hesewake-notch/releases/";
/// Largest API answer read.
pub(crate) const MAX_BYTES: usize = 512 * 1024;

/// A release newer than the running version.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Release {
    /// Without the leading `v`, e.g. `0.9.0`.
    pub(crate) version: String,
    /// The release page.
    pub(crate) url: String,
}

/// `v1.2.3`, `1.2.3`, `1.2.3-alpha.4` → (1, 2, 3, pre-release?). None if it isn't a version.
pub(crate) fn parse_version(s: &str) -> Option<(u64, u64, u64, bool)> {
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    let s = s.split('+').next().unwrap_or("");
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, !p.is_empty()),
        None => (s, false),
    };
    let mut parts = core.split('.');
    let mut num = || parts.next().filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())).and_then(|p| p.parse::<u64>().ok());
    let (a, b, c) = (num()?, num()?, num()?);
    if parts.next().is_some() {
        return None;
    }
    Some((a, b, c, pre))
}

/// Is `latest` newer than `current`? A pre-release `latest` never is.
pub(crate) fn is_newer(latest: &str, current: &str) -> bool {
    let (Some(l), Some(c)) = (parse_version(latest), parse_version(current)) else { return false };
    if l.3 {
        return false;
    }
    let (lv, cv) = ((l.0, l.1, l.2), (c.0, c.1, c.2));
    lv > cv || (lv == cv && c.3)
}

/// The release in the API's answer, if it is a proper, published release of this repository.
pub(crate) fn parse_release(json: &str) -> Option<Release> {
    let v: Value = serde_json::from_str(json).ok()?;
    let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    if flag("draft") || flag("prerelease") {
        return None;
    }
    let tag = v.get("tag_name").and_then(Value::as_str)?;
    let url = v.get("html_url").and_then(Value::as_str).filter(|u| u.starts_with(RELEASES_PREFIX))?;
    let (_, _, _, pre) = parse_version(tag)?;
    if pre {
        return None;
    }
    Some(Release { version: tag.trim_start_matches(['v', 'V']).to_string(), url: url.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_like_semver() {
        assert_eq!(parse_version("v0.8.0"), Some((0, 8, 0, false)));
        assert_eq!(parse_version("1.2.3-alpha.4"), Some((1, 2, 3, true)));
        assert_eq!(parse_version("V2.0.1+build5"), Some((2, 0, 1, false)));
        for bad in ["", "v", "1.2", "1.2.x", "a.b.c", "1.2.3.4", "-1.2.3"] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
        assert!(is_newer("v0.8.1", "0.8.0"));
        assert!(is_newer("0.9.0", "0.8.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("v0.8.0", "0.8.0"), "same version");
        assert!(!is_newer("0.7.9", "0.8.0"), "older");
        assert!(!is_newer("0.9.0-beta.1", "0.8.0"), "pre-releases are ignored");
        assert!(is_newer("0.8.0", "0.8.0-alpha.1"), "the release after its own pre-release");
        assert!(!is_newer("garbage", "0.8.0"));
        assert!(!is_newer("0.9.0", "garbage"), "can't tell: say nothing");
        assert!(!is_newer("0.10.0", "0.10.0"));
        assert!(is_newer("0.10.0", "0.9.0"), "numbers, not text");
    }

    #[test]
    fn releases_are_read_and_checked() {
        let json = r#"{"tag_name":"v0.9.0","html_url":"https://github.com/dirgaastya/hesewake-notch/releases/tag/v0.9.0","draft":false,"prerelease":false}"#;
        assert_eq!(parse_release(json), Some(Release { version: "0.9.0".into(), url: "https://github.com/dirgaastya/hesewake-notch/releases/tag/v0.9.0".into() }));
        let bad = [
            "",
            "{",
            r#"{"message":"Not Found","documentation_url":"https://docs.github.com"}"#,
            r#"{"tag_name":"v0.9.0","html_url":"https://evil.example/releases/tag/v0.9.0"}"#,
            r#"{"tag_name":"v0.9.0","html_url":"https://github.com/dirgaastya/hesewake-notch/releases.evil/x"}"#,
            r#"{"tag_name":"v0.9.0","html_url":"http://github.com/dirgaastya/hesewake-notch/releases/tag/v0.9.0"}"#,
            r#"{"tag_name":"nightly","html_url":"https://github.com/dirgaastya/hesewake-notch/releases/tag/nightly"}"#,
            r#"{"tag_name":"v0.9.0","html_url":"https://github.com/dirgaastya/hesewake-notch/releases/tag/v0.9.0","draft":true}"#,
            r#"{"tag_name":"v0.9.0","html_url":"https://github.com/dirgaastya/hesewake-notch/releases/tag/v0.9.0","prerelease":true}"#,
        ];
        for b in bad {
            assert_eq!(parse_release(b), None, "{b}");
        }
    }
}

/// Normalizes a raw URL string obtained from browser address bars.
///
/// Strips protocol schemes (`https://`, `http://`, `//`), leading `www.`,
/// trailing slashes where appropriate, and converts ASCII to lowercase.
pub fn normalize_url(raw: &str) -> String {
    let mut s = raw.trim();

    // Strip common protocol prefixes
    if let Some(rest) = s.strip_prefix("https://") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("http://") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("//") {
        s = rest;
    }

    // Strip leading "www."
    if let Some(rest) = s.strip_prefix("www.") {
        s = rest;
    }

    s.to_ascii_lowercase()
}

/// Checks if a normalized URL matches any of the configured target rules.
/// Returns the matched rule pattern if found.
pub fn find_matching_rule<'a>(normalized_url: &str, rules: &'a [String]) -> Option<&'a str> {
    for rule in rules {
        let rule_clean = rule.trim().to_ascii_lowercase();
        let rule_norm = rule_clean
            .strip_prefix("https://")
            .or_else(|| rule_clean.strip_prefix("http://"))
            .unwrap_or(&rule_clean)
            .strip_prefix("www.")
            .unwrap_or(&rule_clean);

        if normalized_url.contains(rule_norm) {
            return Some(rule.as_str());
        }
    }
    None
}

/// Extracts a unique slug identifier from the URL for a matched rule.
///
/// For instance, for `youtube.com/shorts/abc123xyz?feature=share` with rule
/// `youtube.com/shorts/`, extracts `abc123xyz`.
pub fn extract_slug(normalized_url: &str, matched_rule: &str) -> String {
    let rule_norm = matched_rule
        .trim()
        .to_ascii_lowercase()
        .replace("https://", "")
        .replace("http://", "")
        .replace("www.", "");

    if let Some(idx) = normalized_url.find(&rule_norm) {
        let after = &normalized_url[idx + rule_norm.len()..];
        // Strip query parameters and anchors
        let clean = after.split('?').next().unwrap_or(after);
        let clean = clean.split('#').next().unwrap_or(clean);
        let trimmed = clean.trim_matches('/');
        if !trimmed.is_empty() {
            // First segment of path after the rule
            let slug = trimmed.split('/').next().unwrap_or(trimmed);
            return slug.to_string();
        }
    }

    // Fallback if no specific slug path is present (e.g. base /reels/)
    rule_norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_url() {
        assert_eq!(
            normalize_url("https://www.youtube.com/shorts/dQw4w9WgXcQ"),
            "youtube.com/shorts/dqw4w9wgxcq"
        );
        assert_eq!(
            normalize_url("HTTP://INSTAGRAM.COM/reels/xyz/"),
            "instagram.com/reels/xyz/"
        );
        assert_eq!(
            normalize_url("   youtube.com/shorts/test   "),
            "youtube.com/shorts/test"
        );
    }

    #[test]
    fn test_matching_rule() {
        let rules = vec![
            "youtube.com/shorts/".to_string(),
            "instagram.com/reels".to_string(),
        ];

        let url1 = normalize_url("https://www.youtube.com/shorts/12345");
        assert_eq!(
            find_matching_rule(&url1, &rules),
            Some("youtube.com/shorts/")
        );

        let url2 = normalize_url("https://instagram.com/reels/abc987/");
        assert_eq!(
            find_matching_rule(&url2, &rules),
            Some("instagram.com/reels")
        );

        let safe_url = normalize_url("https://github.com/rust-lang/rust");
        assert_eq!(find_matching_rule(&safe_url, &rules), None);
    }

    #[test]
    fn test_extract_slug() {
        let url = "youtube.com/shorts/dqw4w9wgxcq?feature=share";
        let slug = extract_slug(url, "youtube.com/shorts/");
        assert_eq!(slug, "dqw4w9wgxcq");

        let url2 = "instagram.com/reels/c_9812739/?hl=en";
        let slug2 = extract_slug(url2, "instagram.com/reels");
        assert_eq!(slug2, "c_9812739");
    }
}

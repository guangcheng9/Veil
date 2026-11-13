//! Detect opaque base64 / data-URL payloads that must not be scanned for secrets.
//!
//! Multimodal APIs put image bytes in JSON string fields. Running phone/IP/token
//! rules over those blobs can rewrite substrings and produce invalid base64.

/// True for `data:<mime>;base64,<payload>` (any mime).
///
/// The whole value is opaque. The payload is intentionally not decoded here:
/// the caller only needs to decide whether it is safe to skip secret scanning.
pub fn is_data_url_base64(text: &str) -> bool {
    let t = text.trim();
    let Some(comma) = t.find(',') else {
        return false;
    };
    if comma == 0
        || !t
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
    {
        return false;
    }
    let header = &t[..comma];
    if !header
        .split(';')
        .any(|part| part.eq_ignore_ascii_case("base64"))
    {
        return false;
    }
    !t[comma + 1..].trim().is_empty()
}

fn is_base64_char(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'+'
            | b'/'
            | b'-'
            | b'_'
    )
}

/// Validate the shape of a standard or URL-safe base64 blob without allocating
/// a decoded buffer.
///
/// This rejects mixed padding, non-base64 characters and impossible lengths.
/// It deliberately accepts both padded and unpadded encodings because
/// multimodal APIs use both.
pub fn looks_like_base64_blob(text: &str) -> bool {
    let t = text.trim();
    if t.len() < 32 {
        return false;
    }

    let mut data_len = 0usize;
    let mut padding = 0usize;
    let mut saw_padding = false;
    let mut saw_standard = false;
    let mut saw_urlsafe = false;
    for byte in t.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if is_base64_char(byte) {
            if saw_padding {
                return false;
            }
            data_len += 1;
            match byte {
                b'+' | b'/' => saw_standard = true,
                b'-' | b'_' => saw_urlsafe = true,
                _ => {}
            }
        } else if byte == b'=' {
            saw_padding = true;
            padding += 1;
            if padding > 2 {
                return false;
            }
        } else {
            return false;
        }
    }

    if data_len < 32 || (saw_standard && saw_urlsafe) {
        return false;
    }
    let logical_len = data_len + padding;
    if padding > 0 && !logical_len.is_multiple_of(4) {
        return false;
    }
    if padding == 0 && logical_len % 4 == 1 {
        return false;
    }
    true
}

fn key_suggests_binary(key: &str) -> bool {
    [
        "data",
        "image",
        "b64_json",
        "inline_data",
        "file_data",
        "content_base64",
        "audio",
        "video",
    ]
    .iter()
    .any(|candidate| key.eq_ignore_ascii_case(candidate))
}

/// Whether outbound desensitization should leave this string untouched.
///
/// Explicit binary keys get a lower length threshold. Generic keys only skip
/// long base64-shaped values; ordinary prose and punctuation are still scanned.
pub fn should_skip_desensitize(key: Option<&str>, text: &str) -> bool {
    if is_data_url_base64(text) {
        return true;
    }
    if !looks_like_base64_blob(text) {
        return false;
    }
    if key.is_some_and(key_suggests_binary) {
        return true;
    }
    text.trim().len() >= 200
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_b64() -> String {
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
            .to_string()
    }

    #[test]
    fn detects_data_url_base64() {
        let url = format!("data:image/png;base64,{}", png_b64());
        assert!(is_data_url_base64(&url));
        assert!(is_data_url_base64("data:image/jpeg;BASE64,AAAA"));
        assert!(is_data_url_base64("DATA:image/png;base64,AAAA"));
        assert!(!is_data_url_base64("data:text/plain,hello"));
        assert!(!is_data_url_base64("https://example.com/a.png"));
        assert!(!is_data_url_base64("data:image/png;base64,"));
    }

    #[test]
    fn detects_standard_and_urlsafe_base64() {
        let standard = "A".repeat(198) + "==";
        let urlsafe = "A".repeat(40) + "-" + &"_".repeat(43);
        assert!(looks_like_base64_blob(&standard));
        assert!(looks_like_base64_blob(&urlsafe));
        assert!(looks_like_base64_blob(&format!(
            "{}\n{}",
            "A".repeat(64),
            "B".repeat(64)
        )));
    }

    #[test]
    fn rejects_non_base64_and_impossible_shapes() {
        assert!(!looks_like_base64_blob("hello world"));
        let invalid_char = "A".repeat(200) + "!";
        assert!(!looks_like_base64_blob(&invalid_char));
        let mixed_padding = "A".repeat(200) + "=" + "A";
        assert!(!looks_like_base64_blob(&mixed_padding));
        assert!(!looks_like_base64_blob(&"A".repeat(33)));
        let mixed_alphabet = "A".repeat(40) + "+" + &"_".repeat(43);
        assert!(!looks_like_base64_blob(&mixed_alphabet));
    }

    #[test]
    fn skips_binary_fields_but_not_prose() {
        let blob = "A".repeat(240);
        assert!(should_skip_desensitize(Some("data"), &blob));
        assert!(should_skip_desensitize(Some("b64_json"), &blob));
        assert!(!should_skip_desensitize(None, "hello world"));
        let prose = "The quick brown fox jumps over the lazy dog. ".repeat(6);
        assert!(!should_skip_desensitize(Some("content"), &prose));
    }

    #[test]
    fn skips_long_raw_base64_but_not_short_values() {
        let long_blob = "A".repeat(240);
        assert!(should_skip_desensitize(None, &long_blob));
        let short_blob = "A".repeat(40);
        assert!(!should_skip_desensitize(None, &short_blob));
    }

    #[test]
    fn keeps_short_non_binary_text_scanable() {
        let phone = "call 13800138000";
        assert!(!should_skip_desensitize(Some("content"), phone));
        assert!(!should_skip_desensitize(Some("data"), phone));
    }
}

//! Strings on their way to something the operating system draws.

/// Removes control and bidirectional formatting characters, so a title or address shows as what it
/// is, and bounds the length. Applied to native window titles and confirmations only: note text
/// itself is never changed.
pub fn plain(text: &str, max_chars: usize) -> String {
    text.chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(*c, '\u{200E}' | '\u{200F}' | '\u{061C}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        })
        .take(max_chars)
        .collect()
}

/// `xx` hex of bytes.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_what_could_disguise_text_and_keeps_every_script() {
        assert_eq!(plain("a\u{202E}b\u{2066}c\nd\u{2069}", 99), "abcd");
        assert_eq!(plain("مرحبا 你好 नमस्ते", 99), "مرحبا 你好 नमस्ते");
        assert_eq!(plain("abcdef", 3), "abc");
    }
}

//! Display-name validation, per the requirements:
//! 1–16 chars, non-empty after trimming surrounding whitespace, no control chars.

/// Why a name was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    /// Name is empty after trimming whitespace.
    Empty,
    /// Name exceeds the 16-character limit.
    TooLong,
    /// Name contains a control character.
    IllegalChar,
}

/// Maximum display-name length (in characters).
pub const MAX_NAME_LEN: usize = 16;

/// Validate and normalize a display name.
///
/// Trims surrounding whitespace, then rejects empty, over-long, and
/// control-character-containing names. The returned `String` is the trimmed
/// form. Check order: empty before length before legality, so a trimmable
/// name like `"\n"` reports [`NameError::Empty`].
pub fn sanitize_name(raw: &str) -> Result<String, NameError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(NameError::Empty);
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(NameError::TooLong);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(NameError::IllegalChar);
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(sanitize_name("  Bob  "), Ok("Bob".to_owned()));
        assert_eq!(sanitize_name("\tAlice\n"), Ok("Alice".to_owned()));
    }

    #[test]
    fn accepts_max_length_name() {
        assert_eq!(sanitize_name("1234567890123456"), Ok("1234567890123456".to_owned()));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(sanitize_name(""), Err(NameError::Empty));
        assert_eq!(sanitize_name("   "), Err(NameError::Empty));
        assert_eq!(sanitize_name("\n"), Err(NameError::Empty));
    }

    #[test]
    fn rejects_too_long() {
        assert_eq!(sanitize_name("12345678901234567"), Err(NameError::TooLong));
    }

    #[test]
    fn rejects_control_chars() {
        assert_eq!(sanitize_name("Bo\u{1}b"), Err(NameError::IllegalChar));
        assert_eq!(sanitize_name("a\tb"), Err(NameError::IllegalChar));
    }

    #[test]
    fn interior_whitespace_is_kept() {
        assert_eq!(sanitize_name("Lost Mage"), Ok("Lost Mage".to_owned()));
    }
}
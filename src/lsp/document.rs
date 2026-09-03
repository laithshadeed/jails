//! The buffer, addressed the way an editor addresses it.
//!
//! A `file:` URI in, a path out; a zero-based line and character in, the
//! text of that line and the word under the cursor out. Nothing here knows
//! what JDL is -- that is [`super::language`] -- and nothing here reads the
//! filesystem.

/// The filesystem path a `file:` URI names, with percent-escapes undone.
///
/// **Only `file:`.** A client editing over `untitled:` or a remote scheme is
/// editing something this server has no project for, and guessing a path
/// from one would answer about a file that is not there.
pub(super) fn path_of(uri: &str) -> Option<std::path::PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // `file:///home/...` on unix: the empty authority, then the path.
    let rest = rest.strip_prefix('/').map(|tail| format!("/{tail}"))?;
    Some(std::path::PathBuf::from(unescape(&rest)))
}

/// Percent-decoding, for the escapes a URI actually carries: a space is
/// `%20` in every client and a path with one is ordinary.
fn unescape(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && at + 2 < bytes.len()
            && let Some(byte) = std::str::from_utf8(&bytes[at + 1..at + 3])
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        {
            out.push(byte);
            at += 3;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The text of one zero-based line.
pub(super) fn line_at(text: &str, line: usize) -> Option<&str> {
    text.lines().nth(line)
}

/// Everything on the line before the cursor.
///
/// **Character offsets are counted in characters, not bytes.** A comment
/// with an em dash in it moves every byte offset after it and no editor's
/// column with it.
pub(super) fn before(line: &str, column: usize) -> &str {
    match line.char_indices().nth(column) {
        Some((at, _)) => &line[..at],
        None => line,
    }
}

/// The identifier the cursor is inside or immediately after.
///
/// Word characters are what JDL identifiers are made of, plus the `@` that
/// starts an attribute, so hovering `@pk` answers about the marker rather
/// than about `pk` with no idea what it is.
pub(super) fn word_at(line: &str, column: usize) -> &str {
    let characters: Vec<(usize, char)> = line.char_indices().collect();
    let is_word = |character: char| character.is_alphanumeric() || character == '_';
    let cursor = column.min(characters.len());
    let mut start = cursor;
    while start > 0 && is_word(characters[start - 1].1) {
        start -= 1;
    }
    // The `@` belongs to the word it introduces.
    if start > 0 && characters[start - 1].1 == '@' {
        start -= 1;
    }
    let mut end = cursor;
    while end < characters.len() && is_word(characters[end].1) {
        end += 1;
    }
    if start >= end {
        return "";
    }
    let from = characters[start].0;
    let to = characters.get(end).map_or(line.len(), |(byte, _)| *byte);
    &line[from..to]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_uri_becomes_a_path_and_anything_else_becomes_nothing() {
        assert_eq!(
            path_of("file:///home/me/my%20app"),
            Some(std::path::PathBuf::from("/home/me/my app"))
        );
        assert_eq!(path_of("untitled:Untitled-1"), None);
    }

    #[test]
    fn the_word_under_the_cursor_takes_its_attribute_sign_with_it() {
        let line = "  id: uuid @pk";
        assert_eq!(word_at(line, 13), "@pk");
        assert_eq!(word_at(line, 4), "id");
        assert_eq!(word_at(line, 9), "uuid");
    }

    #[test]
    fn a_column_is_characters_rather_than_bytes() {
        // Five characters, seven bytes: at column three a byte slice would
        // land inside the dash and panic, and at column four it would cut
        // the line one character short of where the cursor is.
        let line = "a — b";
        assert_eq!(before(line, 3), "a —");
        assert_eq!(before(line, 4), "a — ");
        assert_eq!(word_at(line, 5), "b");
    }
}

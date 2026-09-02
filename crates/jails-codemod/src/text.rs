//! Reading Java text without being fooled by what is inside it.
//!
//! One scanner, two public modes, and the only blanker in the workspace: a
//! second one would be a second answer to where a literal ends. It reads text
//! and returns text, and knows nothing about jails.

/// The source with every comment and string/char literal replaced by spaces,
/// preserving length so byte offsets into the result also index the original.
pub fn blanked(source: &str) -> String {
    masked(source, true)
}

/// One allocation and one scanner for both public masking modes.
///
/// Starting from a memcpy of the source is substantially cheaper than
/// filling a same-sized buffer with spaces and copying ordinary source code
/// one byte at a time. Generated Java is overwhelmingly ordinary code; only
/// the comparatively small comment/literal ranges need rewriting.
fn masked(source: &str, comments: bool) -> String {
    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                if comments {
                    blank_range(&mut out, start, i);
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                let start = i;
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
                if comments {
                    blank_range(&mut out, start, i);
                }
            }
            // Text blocks first: `"""` would otherwise read as an empty
            // string literal followed by an unterminated one.
            b'"' if bytes[i..].starts_with(br#"""""#) => {
                let start = i;
                i += 3;
                while i + 2 < bytes.len() && !bytes[i..].starts_with(br#"""""#) {
                    i += 1;
                }
                i = (i + 3).min(bytes.len());
                blank_range(&mut out, start, i);
            }
            quote @ (b'"' | b'\'') => {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
                blank_range(&mut out, start, i);
            }
            _ => i += 1,
        }
    }
    valid_mask(out)
}

/// The mirror image of [`blanked`], and the two exist for opposite reasons.
/// A scan for annotations must ignore comments; a scan for `TODO` markers
/// must read only comments, and must still not be fooled by the word
/// appearing inside a string literal. Length is preserved either way, so line
/// and byte offsets still index the original.
pub fn without_literals(source: &str) -> String {
    masked(source, false)
}

/// Blank a byte range to spaces, leaving newlines so line numbers survive a
/// multi-line text block.
fn blank_range(out: &mut [u8], start: usize, end: usize) {
    let end = end.min(out.len());
    for byte in &mut out[start..end] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

fn valid_mask(out: Vec<u8>) -> String {
    String::from_utf8(out).unwrap_or_else(|error| {
        // The source starts as valid UTF-8 and masking only introduces ASCII,
        // so this is defensive. Reuse the allocation even on malformed input.
        let mut bytes = error.into_bytes();
        for byte in &mut bytes {
            if !byte.is_ascii() {
                *byte = b' ';
            }
        }
        String::from_utf8(bytes).expect("replacing non-ASCII bytes makes valid UTF-8")
    })
}

//! The one question jails asks a `.class` file: which types does it name?
//!
//! This is the index behind `jails testd --affected`, and it is deliberately
//! the smallest reader that can answer that. **Not a class-file parser, and it
//! must not grow into one** -- the same rule `java.rs` carries. It reads the
//! constant pool and stops: no access flags, no fields, no methods, no code.
//!
//! Two entry kinds matter and the rest exist only to be stepped over.
//! `CONSTANT_Class` names a type directly. `CONSTANT_Utf8` carries the
//! descriptors -- `Lcom/example/Money;` inside a field type, a method
//! signature or an annotation -- and a type a class only mentions in a
//! signature is still a type whose change can break it.
//!
//! **`CONSTANT_Long` and `CONSTANT_Double` take two pool slots each.** That is
//! the trap in this format: a reader that advances by one after them reads
//! every later entry at the wrong index, and because the tags it then lands on
//! are usually valid it produces a plausible, wrong answer rather than an
//! error. `a_long_in_the_pool_does_not_shift_every_later_entry` pins it.

use std::collections::BTreeSet;

/// Every internal class name (`com/example/Money`) this class file names.
pub(crate) fn referenced_types(bytes: &[u8]) -> Option<BTreeSet<String>> {
    let mut reader = Reader::new(bytes);
    if reader.u32()? != 0xCAFE_BABE {
        return None;
    }
    reader.skip(4)?; // minor and major version
    let count = reader.u16()?;

    let mut utf8 = Vec::new();
    let mut class_indices = Vec::new();
    // The pool is 1-indexed and `count` is one past the last entry.
    let mut index = 1;
    while index < count {
        let tag = reader.u8()?;
        match tag {
            1 => {
                let length = reader.u16()? as usize;
                let text = reader.bytes(length)?;
                utf8.push((index, String::from_utf8_lossy(text).into_owned()));
            }
            7 | 8 | 16 | 19 | 20 => {
                let target = reader.u16()?;
                if tag == 7 {
                    class_indices.push(target);
                }
            }
            15 => reader.skip(3)?,
            3 | 4 | 9 | 10 | 11 | 12 | 17 | 18 => reader.skip(4)?,
            5 | 6 => {
                reader.skip(8)?;
                // The whole reason this function cannot be a simple loop.
                index += 1;
            }
            _ => return None,
        }
        index += 1;
    }

    let mut types = BTreeSet::new();
    for (slot, text) in &utf8 {
        if class_indices.contains(slot) {
            // A CONSTANT_Class name is the type itself. An array type is
            // spelled as a descriptor even here, so it falls through to the
            // descriptor scan below rather than being taken literally.
            if !text.starts_with('[') {
                types.insert(text.clone());
            }
        }
        collect_descriptors(text, &mut types);
    }
    Some(types)
}

/// Pull `Lcom/example/Money;` out of a descriptor or signature.
///
/// Runs over **every** Utf8 entry, not only the ones a `NameAndType` points
/// at, because generic signatures and annotation values are Utf8 entries
/// nothing else references. A string constant that happens to look like a
/// descriptor yields a name no class file will match, which costs nothing:
/// the index is only ever intersected with types the project actually has.
fn collect_descriptors(text: &str, types: &mut BTreeSet<String>) {
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(offset) = bytes[start..].iter().position(|byte| *byte == b'L') {
        let from = start + offset + 1;
        match bytes[from..].iter().position(|byte| *byte == b';') {
            Some(length) => {
                let name = &text[from..from + length];
                // `<` and `>` appear in generic signatures; a name carrying
                // one is a parameterised use, and the raw type before it has
                // already been recorded by the descriptor that opened it.
                if !name.is_empty() && !name.contains(['<', '>', '.', ' ']) {
                    types.insert(name.to_string());
                }
                start = from + length + 1;
            }
            None => break,
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        let slice = self.bytes.get(self.at..self.at + count)?;
        self.at += count;
        Some(slice)
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.bytes(count).map(|_| ())
    }

    fn u8(&mut self) -> Option<u8> {
        self.bytes(1).map(|slice| slice[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.bytes(2)
            .map(|slice| u16::from_be_bytes([slice[0], slice[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.bytes(4)
            .map(|slice| u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal pool, built by hand so the shape is visible.
    fn class_file(entries: Vec<Vec<u8>>) -> Vec<u8> {
        let mut bytes = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 69];
        // One past the last slot, and slot 0 does not exist.
        let mut slots = 1u16;
        for entry in &entries {
            slots += if matches!(entry[0], 5 | 6) { 2 } else { 1 };
        }
        bytes.extend_from_slice(&slots.to_be_bytes());
        for entry in entries {
            bytes.extend_from_slice(&entry);
        }
        bytes
    }

    fn utf8(text: &str) -> Vec<u8> {
        let mut entry = vec![1];
        entry.extend_from_slice(&(text.len() as u16).to_be_bytes());
        entry.extend_from_slice(text.as_bytes());
        entry
    }

    fn class_ref(slot: u16) -> Vec<u8> {
        let mut entry = vec![7];
        entry.extend_from_slice(&slot.to_be_bytes());
        entry
    }

    #[test]
    fn a_class_entry_names_its_type() {
        let bytes = class_file(vec![utf8("com/example/Money"), class_ref(1)]);
        let types = referenced_types(&bytes).unwrap();
        assert!(types.contains("com/example/Money"));
    }

    /// The trap this module exists to avoid. A `CONSTANT_Long` occupies two
    /// slots, so a reader that advances by one reads every later entry at the
    /// wrong index -- and lands on tags that are usually valid, so the result
    /// is plausible and wrong rather than an error.
    #[test]
    fn a_long_in_the_pool_does_not_shift_every_later_entry() {
        let mut long = vec![5];
        long.extend_from_slice(&7i64.to_be_bytes());
        // Slots: 1 = Utf8, 2 = Long (and 3, which does not exist), 4 = Class.
        let bytes = class_file(vec![utf8("com/example/Money"), long, class_ref(1)]);
        let types = referenced_types(&bytes).unwrap();
        assert!(
            types.contains("com/example/Money"),
            "the class entry after a long must still resolve: {types:?}"
        );
    }

    /// A type only ever mentioned in a signature is still one whose change can
    /// break this class, and it has no `CONSTANT_Class` entry at all.
    #[test]
    fn a_type_only_named_in_a_descriptor_is_still_found() {
        let bytes = class_file(vec![utf8("(Lcom/example/Money;)Lcom/example/Receipt;")]);
        let types = referenced_types(&bytes).unwrap();
        assert!(types.contains("com/example/Money"));
        assert!(types.contains("com/example/Receipt"));
    }

    #[test]
    fn something_that_is_not_a_class_file_is_refused_rather_than_guessed() {
        assert!(referenced_types(b"not a class file at all").is_none());
        assert!(referenced_types(&[0xCA, 0xFE, 0xBA, 0xBE]).is_none());
    }
}

#[cfg(test)]
mod real_class_files {
    use super::*;

    /// Synthetic pools prove the arithmetic; a real one proves the arithmetic
    /// was about the right format. `JAILS_CLASSFILE_PROBE` points at a
    /// directory of `.class` files -- every one must parse, and the types they
    /// name must include their own package's neighbours.
    #[test]
    fn every_class_file_in_the_probe_directory_parses() {
        let Some(dir) = std::env::var_os("JAILS_CLASSFILE_PROBE") else {
            return;
        };
        let mut seen = 0;
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(path) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "class") {
                    let bytes = std::fs::read(&path).unwrap();
                    assert!(
                        referenced_types(&bytes).is_some(),
                        "failed to read {}",
                        path.display()
                    );
                    seen += 1;
                }
            }
        }
        assert!(seen > 0, "the probe directory held no class files");
    }
}

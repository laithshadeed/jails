//! Content addressing: SHA-256, domain separation, and hex.
//!
//! An identity is `SHA256(ASCII-prefix || bytes)` and **the prefix is not
//! length-prefixed**. It is a constant per identity kind, so it cannot be
//! confused with a following length field, and keeping it raw is what lets a
//! second implementation reproduce the exact hex.
//!
//! Every value jails compares by identity -- a blob, a rendered tree, a plan
//! bundle, a classpath, a test epoch -- is one of these digests, so two
//! callers that hashed the same value differently would not produce a decode
//! error. They would produce a *different identity for the same value*, which
//! is why there is one owner rather than a helper per crate.

use crate::Result;

/// A digest, ID or any other fixed 32-byte value.
pub const DIGEST_BYTES: usize = 32;

/// `SHA256(prefix || encoded)`, the one shape every identity in this protocol
/// has. The ASCII prefix is deliberately **not** length-prefixed.
pub fn domain_hash(prefix: &str, encoded: &[u8]) -> [u8; DIGEST_BYTES] {
    let mut input = Vec::with_capacity(prefix.len() + encoded.len());
    input.extend_from_slice(prefix.as_bytes());
    input.extend_from_slice(encoded);
    sha256(&input)
}

/// Lowercase 64-hex. Presentation only -- never a wire form.
pub fn hex(digest: &[u8; DIGEST_BYTES]) -> String {
    let mut out = String::with_capacity(DIGEST_BYTES * 2);
    for byte in digest {
        out.push(char::from_digit((byte >> 4) as u32, 16).expect("a nibble is a hex digit"));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).expect("a nibble is a hex digit"));
    }
    out
}

/// Exactly 64 lowercase hex characters, back to bytes.
///
/// Uppercase rejects on purpose: parsing then rendering has to be
/// byte-identical, and accepting both spellings would break that.
pub fn unhex(text: &str) -> Result<[u8; DIGEST_BYTES]> {
    if text.len() != DIGEST_BYTES * 2 {
        return Err(format!(
            "expected {} lowercase hex characters, found {}",
            DIGEST_BYTES * 2,
            text.len()
        )
        .into());
    }
    let mut out = [0u8; DIGEST_BYTES];
    let bytes = text.as_bytes();
    for (index, slot) in out.iter_mut().enumerate() {
        let hi = nibble(bytes[index * 2])?;
        let lo = nibble(bytes[index * 2 + 1])?;
        *slot = (hi << 4) | lo;
    }
    Ok(out)
}

fn nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(format!("`{}` is not a lowercase hexadecimal digit", byte as char).into()),
    }
}

/// FIPS 180-4 SHA-256.
///
/// Hand-written for the same reason `config.rs` hand-parses TOML: jails'
/// dependency list is short on purpose, and this is a fixed, fully specified,
/// heavily vectored algorithm rather than a moving target. It is checked
/// against the standard's own test vectors below.
pub fn sha256(input: &[u8]) -> [u8; DIGEST_BYTES] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // **The input is read where it is, and only the tail is built.** Copying
    // it to append eight bytes of length costs a second allocation the size
    // of the thing being hashed, and the largest thing jails hashes is the
    // accepted projection serialised as JSON -- fourteen megabytes on a
    // hundred-entity project, hashed on every capture and every plan. The
    // padding is at most two blocks whatever the input is.
    let bit_length = (input.len() as u64).wrapping_mul(8);
    let (blocks, remainder) = input.as_chunks::<64>();
    let mut tail = [0u8; 128];
    tail[..remainder.len()].copy_from_slice(remainder);
    tail[remainder.len()] = 0x80;
    // 56 bytes of the last block are the message; the final eight are the
    // length. One block when the remainder leaves room for both, two when it
    // does not.
    let tail_blocks = match remainder.len() < 56 {
        true => 1,
        false => 2,
    };
    let length_at = tail_blocks * 64 - 8;
    tail[length_at..length_at + 8].copy_from_slice(&bit_length.to_be_bytes());

    // Both sizes are constants, so both are `as_chunks`. The inner one earns
    // it twice over: a `&[u8; 4]` is exactly what `from_be_bytes` wants, so
    // the `try_into().expect("four bytes")` -- a fallible conversion standing
    // in for a fact the padding above already guarantees -- disappears.
    for chunk in blocks
        .iter()
        .chain(tail[..tail_blocks * 64].as_chunks::<64>().0)
    {
        let mut w = [0u32; 64];
        for (index, word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            w[index] = u32::from_be_bytes(*word);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut out = [0u8; DIGEST_BYTES];
    for (index, word) in h.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FIPS 180-4's own vectors, plus the two-block and empty cases.
    ///
    /// This is hand-written cryptography, so it is checked against the
    /// standard rather than against itself. Every identity in the protocol is
    /// a SHA-256, so a subtly wrong implementation would not fail loudly -- it
    /// would produce stable, self-consistent, *wrong* identities that no other
    /// implementation could reproduce.
    #[test]
    fn sha256_matches_the_published_vectors() {
        for (input, expected) in [
            (
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            ),
        ] {
            assert_eq!(hex(&sha256(input.as_bytes())), expected, "input {input:?}");
        }
        // One million 'a's -- the vector that catches a wrong multi-block loop
        // or a bit-length that overflows.
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&sha256(&million)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    /// Parse then render is byte-identical, which is why uppercase refuses.
    #[test]
    fn hex_round_trips_and_refuses_anything_but_lowercase() {
        let digest = sha256(b"abc");
        let text = hex(&digest);
        assert_eq!(text.len(), 64);
        assert_eq!(unhex(&text).unwrap(), digest);
        assert_eq!(hex(&unhex(&text).unwrap()), text);

        assert!(
            unhex(&text.to_uppercase())
                .unwrap_err()
                .contains("lowercase")
        );
        assert!(unhex("abc").unwrap_err().contains("expected 64"));
        assert!(unhex(&"g".repeat(64)).unwrap_err().contains("hexadecimal"));
    }

    /// Two different prefixes over the same value give different identities --
    /// which is the entire reason each identity kind has its own.
    #[test]
    fn a_domain_prefix_is_concatenated_raw_and_keeps_two_kinds_apart() {
        let value = b"the same bytes";
        let mut expected = b"JAILS-EXAMPLE-1".to_vec();
        expected.extend_from_slice(value);
        assert_eq!(domain_hash("JAILS-EXAMPLE-1", value), sha256(&expected));
        assert_ne!(
            domain_hash("JAILS-EXAMPLE-1", value),
            domain_hash("JAILS-EXAMPLE-2", value)
        );
    }

    /// 100,000 bytes hashed by coreutils' `sha256sum`, compared byte for byte.
    /// The published vectors prove the algorithm; this pins agreement with an
    /// independent implementation on input nobody chose, at a length that is
    /// not a block multiple. The digest is pinned rather than the file kept,
    /// so the test stays hermetic.
    #[test]
    fn sha256_agreed_with_coreutils_on_random_bytes() {
        let bytes: Vec<u8> = (0..100_000u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect();
        assert_eq!(hex(&sha256(&bytes)), SEEDED_DIGEST);
    }

    /// coreutils' `sha256sum` over the bytes the test above builds.
    const SEEDED_DIGEST: &str = "da7d952c43183bf6d33a9110c955bb23227d7dc925819d3f579ce2e01e81b603";
}

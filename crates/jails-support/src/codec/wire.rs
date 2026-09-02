//! Rust's own types, on the wire.
//!
//! Each impl delegates to the [`Encoder`]/[`Decoder`] method that already
//! frames that shape, so the bytes are exactly what a hand-written codec
//! writes and `#[derive(Codec)]` can stand in for one without moving a byte.
//!
//! Without these the trait stops at the first `bool`: a record holding one
//! cannot be written generically, so its codec has to be spelled out by hand.

use super::{Codec, Decoder, Encoder};
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};

impl Codec for bool {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.bool(*self);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.bool()
    }
}

/// A `Box` is not a wire shape.
///
/// It exists to keep a large variant from widening the enum in memory, which
/// is a Rust concern and not a format one -- so the encoding is the inner
/// value's, unchanged. Without this a boxed payload has to be unboxed by hand
/// in the codec, which is the sort of detail a derive should absorb.
impl<T: Codec> Codec for Box<T> {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        (**self).encode(encoder)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        T::decode(decoder).map(Box::new)
    }
}

impl Codec for u32 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.u32(*self);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.u32()
    }
}

impl Codec for u64 {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.u64(*self);
        Ok(())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.u64()
    }
}

/// A `String` takes the ordinary-string cap, never the path one.
///
/// The two differ only in their length limit, and a path is always a validated
/// newtype with its own codec -- so there is no shape where a bare `String`
/// should have taken `MAX_PATH_BYTES`.
impl Codec for String {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.string(self)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.string()
    }
}

impl<T: Codec> Codec for Option<T> {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.maybe(self.as_ref())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.perhaps()
    }
}

impl<T: Codec> Codec for Vec<T> {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.seq(self.len(), self.iter())
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.seq()
    }
}

impl<T: Codec + Ord + std::fmt::Debug> Codec for BTreeSet<T> {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.set(self)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.set()
    }
}

impl<K: Codec + Ord + std::fmt::Debug, V: Codec> Codec for BTreeMap<K, V> {
    fn encode(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.map(self)
    }

    fn decode(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.map()
    }
}

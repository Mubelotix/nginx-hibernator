//! This is a copy of `heed::types::SerdeBincode` but using bincode2 instead of bincode.
//! More importantly, it enables big-endian encoding. This is necessary to have correct ordering of keys in the database.

use std::borrow::Cow;

use heed::{BoxedError, BytesDecode, BytesEncode};
use serde::{Deserialize, Serialize};

fn config() -> bincode2::Config {
    let mut config = bincode2::config();
    config.big_endian();
    config
}

/// Describes a type that is [`Serialize`]/[`Deserialize`] and uses `bincode` to do so.
///
/// It can borrow bytes from the original slice.
pub struct Bincoded<T>(std::marker::PhantomData<T>);

impl<'a, T: 'a> BytesEncode<'a> for Bincoded<T>
where
    T: Serialize,
{
    type EItem = T;

    fn bytes_encode(item: &'a Self::EItem) -> Result<Cow<'a, [u8]>, BoxedError> {
        config().serialize(item).map(Cow::Owned).map_err(Into::into)
    }
}

impl<'a, T: 'a> BytesDecode<'a> for Bincoded<T>
where
    T: Deserialize<'a>,
{
    type DItem = T;

    fn bytes_decode(bytes: &'a [u8]) -> Result<Self::DItem, BoxedError> {
        config().deserialize(bytes).map_err(Into::into)
    }
}

unsafe impl<T> Send for Bincoded<T> {}

unsafe impl<T> Sync for Bincoded<T> {}

//! Owned, self-referential wrapper around [`Moc3`].
use yoke::Yoke;

use crate::data::{Moc3, Moc3Error};

/// Owns the MOC3 bytes together with the zero-copy [`Moc3`] view parsed from
/// them.
///
/// Construct one with [`OwnedMoc3::parse`], then reach the parsed data through
/// [`moc3`](OwnedMoc3::moc3).
pub struct OwnedMoc3 {
    yoke: Yoke<Moc3<'static>, Box<[u8]>>,
}

impl OwnedMoc3 {
    /// Take ownership of `data` and parse it into a [`Moc3`] view.
    pub fn parse(data: impl Into<Box<[u8]>>) -> Result<Self, Moc3Error> {
        let yoke = Yoke::try_attach_to_cart(data.into(), |data| Moc3::new(data))?;
        Ok(OwnedMoc3 { yoke })
    }

    /// Get the parsed [`Moc3`] view.
    #[inline]
    pub fn moc3(&self) -> Moc3<'_> {
        *self.yoke.get()
    }

    /// Borrow the underlying bytes.
    #[inline]
    pub fn bytes(&self) -> &[u8] {
        self.yoke.backing_cart()
    }

    /// Consume the wrapper and return the owned bytes.
    pub fn into_bytes(self) -> Box<[u8]> {
        self.yoke.into_backing_cart()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn propagates_parse_errors() {
        assert!(OwnedMoc3::parse(Vec::new()).is_err());
    }
}

/// Errors that can occur while constructing an [`OnlyQueue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnlyqError {
    /// Requested/inferred capacity was zero - a queue must hold at least
    /// one element, since it is never resized.
    ZeroCapacity,
    /// `T` is a zero-sized type, which `OnlyQueue` does not support (yet).
    ZeroSizedType,
}

impl std::fmt::Display for OnlyqError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCapacity => f.write_str("cannot create a queue with zero capacity"),
            Self::ZeroSizedType => f.write_str("OnlyQueue does not support zero-sized types yet"),
        }
    }
}

impl std::error::Error for OnlyqError {}

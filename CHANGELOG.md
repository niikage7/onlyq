# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]


## [v0.2.0] - 2026-08-18

### Added
- `IntoIterator` implementation for `OnlyQueue<T>`, via a new `IntoIter<T>`
  type. Yields every stored element by value, in physical slot order
  (same order as `get_all`), without cloning.


## [v0.1.1] - 2026-08-17

### Added
- `is_empty()` accessor.

### Changed
- Internal: allocation, pointer arithmetic, and raw reads/writes are now
  handled by a private `RawQueue<T>` type instead of free functions in a
  `utils` module. No public API impact.


## [v0.1.0] - 2026-08-16

### Added
- `OnlyQueue<T>`: a fixed-capacity, heap-allocated cyclic queue that overwrites
  the oldest element once full instead of growing.
- Construction:
    - `new` - panics on zero capacity or zero-sized `T`
    - `try_new` - fallible version, returns `Result<_, OnlyqError>`
    - `new_unchecked` - `unsafe`, skips validation
    - `TryFrom<Vec<T>>` - takes ownership of an existing `Vec`'s allocation
    - `TryFrom<&[T]>` where `T: Clone` - clones elements into a fresh buffer
- Accessors: `len`, `cap`, `idx`
- Reading elements: `get`, `get_all`
- Writing: `push` - appends while capacity remains, otherwise evicts and
  returns the oldest element
- `clear` - drops all elements and resets the queue to empty
- Trait implementations: `Deref<Target = [T]>`, `Display`, `Debug`, `Drop`
- `OnlyqError`: error type for fallible construction (`ZeroCapacity`,
  `ZeroSizedType`), implementing `Display` and `std::error::Error`


[Unreleased]: https://github.com/niikage7/onlyq/compare/v0.2.0...HEAD
[v0.2.0]: https://github.com/niikage7/onlyq/compare/v0.1.1...v0.2.0
[v0.1.1]: https://github.com/niikage7/onlyq/compare/v0.1.0...v0.1.1
[v0.1.0]: https://github.com/niikage7/onlyq/releases/tag/v0.1.0
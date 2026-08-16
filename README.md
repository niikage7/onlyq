# onlyq
A cyclic data structure that allows the oldest value to be overwritten by the newest one.

[![Crates.io][crates-badge]][crates-url]
[![Docs.io][docs-badge]][docs-url]
[![MIT licensed][mit-badge]][mit-url]

[crates-badge]: https://img.shields.io/crates/v/onlyq.svg
[crates-url]: https://crates.io/crates/onlyq
[docs-badge]: https://img.shields.io/docsrs/onlyq
[docs-url]: https://docs.rs/onlyq
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/niikage7/onlyq/blob/main/LICENSE

## Promises

### 1. Items are stored in a _Cyclic_ buffer
All items are stored in a structure called a _Cyclic (or Ring) buffer_, which allows the oldest values to be replaced with the newest ones.

### 2. Buffer has a _Fixed_ capacity
The buffer will **NEVER** allocate more memory than it needs to store the given number of elements.
When the user tries to push an element beyond capacity, the new element will replace the oldest one, according to Promise #1 - so the buffer never needs to be reallocated.

### 3. Order of elements is _Unchangeable_
There is no way to remove a single item from the buffer or reorder its elements - the only way to remove anything is `clear()`, which empties the buffer entirely. 

Note: `pop` will not be implemented, since it would conflict with this promise (see the crate's design notes on eviction ordering).

## Example
```rust
use onlyq::OnlyQueue;

let mut queue = OnlyQueue::<i32>::new(3);
queue.push(1);
queue.push(2);
queue.push(3);

// The queue is now full; pushing another element overwrites the oldest one.
let evicted = queue.push(4);
assert_eq!(evicted, Some(1));
assert_eq!(queue.get_all(), &[4, 2, 3]);
```

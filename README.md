# onlyq
A cyclic data structure that allows the oldest value to be overwritten by the newest one.

## Promises

### 1. Items are stored in a _Cyclic_ buffer
All items are stored in a structure called a _Cyclic (or Ring) buffer_, which allows the oldest values to be replaced with the newest ones.

### 2. Buffer has a _Fixed_ capacity
The buffer will **NEVER** allocate more memory than it needs to store the given number of elements.
When the user tries to push an element beyond capacity, the new element will replace the oldest one, according to Promise #1 - so the buffer never needs to be reallocated.

### 3. Order of elements is _Unchangeable_
There is no way to remove a single item from the buffer or reorder its elements - the only way to remove anything is `clear()`, which empties the buffer entirely. 

Note: `pop` will not be implemented, since it would conflict with this promise (see the crate's design notes on eviction ordering).
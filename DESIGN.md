# Design

## Application-owned order types

`Order` deliberately exposes behavior rather than prescribing identifier, price, quantity, or side
types. Applications can retain richer domain types and arbitrary metadata on their orders.

The book only asks `is_buy()`. A library-owned side enum would force users to translate into a type
that cannot represent any additional application semantics.

## Book-owned fills

Submitting an order returns a slice of fills owned by the book. The slice remains valid until the
next mutable book operation. This keeps the submission API small and lets the book reuse its fill
buffer.

A full fill owns the maker removed from the book. A partial fill contains:

- a maker-order snapshot immediately before execution; and
- the executed quantity.

Returning the order preserves arbitrary application-specific data. A partially filled maker remains
in the book, so producing an owned snapshot requires `Order: Clone`. Completely filled makers are
moved directly into the fill buffer.

The executed quantity is a separate field rather than being written into a partial maker snapshot.
Consequently, the snapshot remains a genuine order and its quantity has one unambiguous meaning.

There is no separate submission status. Callers that need to know whether the incoming order remains
open can query the book by its identifier after processing the fills.

## Identifier uniqueness

Order identifiers are expected to be unique, but the book does not enforce this. Identifier
allocation and uniqueness belong to the surrounding exchange infrastructure. Implementations are
not required to maintain an ID index, although identifiers are hashable so optimized implementations
can do so.

## Quantity policy

The book does not define or validate zero, negative, or otherwise invalid quantities. Quantity policy
belongs to the surrounding exchange infrastructure, just like identifier uniqueness. Matching only
requires quantities to be ordered and subtractable.

Reducing an order requires a quantity lower than its current quantity. Cancellation remains a
distinct operation.

## Construction

`Default` and `FromIterator` are not requirements of the `OrderBook` trait. Construction is separate
from matching behavior, and collecting an iterator should not unexpectedly execute orders or panic
when they cross.

## Indexed price levels

`LevelBook` follows the conventional tree-of-levels and linked-orders design. Separate bid and ask
`BTreeMap`s map prices to level handles. Each level is a FIFO doubly linked list of order handles,
and an `FxHashMap` maps application order identifiers directly to those handles.

Orders and levels live in private generational vector arenas. Handles remain stable while occupied,
removed values can be moved out without cloning, and stale handles cannot refer to reused slots.
Cancellation unlinks an order immediately rather than leaving tombstones or periodically compacting
queues.

The identifier index uses a fast, non-cryptographic hasher intended for trusted exchange input.
`LevelBook` requires cloneable identifiers and prices because its indexes own those keys.

`FlatLevelBook` retains the same arenas, identifier index, and linked FIFO levels but replaces each
price tree with a sorted vector. Both sides are stored worst-to-best, so matching and removal at the
best level operate at the vector's end. This trades linear movement when inserting or removing a
non-best level for contiguous searches and lower fixed overhead on books with relatively few levels.

## Validation and measurement

`VecBook` is the differential reference for `LevelBook` and `FlatLevelBook`. Deterministic mixed
traces, property-based action sequences, real QuantCup replay, and internal arena/link invariants
check equivalent behavior.
Benchmarks generate or load traces before timing and exclude prepared-book setup and teardown.
Focused experiments cover operation scaling, arena reuse, large-order cloning, and identifier
hashers.

## Other order behavior

The matching engine accepts limit orders only. Market, immediate-or-cancel, post-only, and similar
policies belong in infrastructure around the book.

A midpoint is not part of the API because price is generic and a midpoint may not be well-defined.

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
allocation and uniqueness belong to the surrounding exchange infrastructure. This also avoids
requiring every implementation to maintain an ID index.

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

## Other order behavior

The matching engine accepts limit orders only. Market, immediate-or-cancel, post-only, and similar
policies belong in infrastructure around the book.

A midpoint is not part of the API because price is generic and a midpoint may not be well-defined.

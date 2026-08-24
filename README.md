# Lobster

A small in-memory, price-time-priority limit order book in Rust. It is intended for exchanges with
straightforward matching mechanics and order types.

## Example

```rust
use lobster::{Fill, OrderBook, SimpleOrder, VecBook};

let mut book = VecBook::new();

let _ = book.submit(SimpleOrder::sell(0, 2, 5));
let maker = SimpleOrder::sell(1, 3, 6);
let _ = book.submit(maker);
let _ = book.submit(SimpleOrder::sell(2, 4, 7));
book.cancel(&0);

let fills = book.submit(SimpleOrder::buy(3, 6, 6));

assert_eq!(fills, [Fill::Full(maker)]);
```

## Implementations

- `VecBook` is a compact reference implementation backed by sorted vectors.
- `LevelBook` stores price levels in balanced trees, orders in generational vector arenas, and an
  `FxHashMap` identifier index for direct cancellation, reduction, and lookup.
- `FlatLevelBook` keeps the same indexed FIFO structure but stores price levels in sorted contiguous
  vectors, targeting books with relatively few active levels.

## Design

- Generic over the application's order, identifier, quantity, and price types.
- Fills preserve the complete resting order, including application-specific data.
- Price-time priority with bids and asks exposed from best to worst.
- Limit orders only. Other order behaviors can be implemented by exchange infrastructure around the
  book.
- Order identifier uniqueness is a caller responsibility; indexed implementations assume it.

## Development

Enter the reproducible development shell with `nix develop` and run checks with `just check`.
Benchmark recipes compile with `-C target-cpu=native`; their binaries and results are specific to
the host CPU. See [`benches/README.md`](benches/README.md) for synthetic, QuantCup, focused
operation, memory-reuse, and hasher experiments.

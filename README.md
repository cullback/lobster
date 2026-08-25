# Lobster

A small in-memory price-time-priority limit order book for Rust, intended as a matching core for
exchanges with straightforward rules.

## Features

- `submit`: matches an order and returns its maker fills
- `cancel`: removes an open order by identifier
- `reduce`: lowers an order's open quantity without changing priority
- `best_bid`, `best_ask`, and iterators inspect resting orders
- Application-owned order, identifier, quantity, price, and side representation

## Exchange policy

- Immediate-or-cancel: call `submit`, then `cancel`
- Add-liquidity-only: compare the submitted price with `best_ask` or `best_bid`
- Stop orders: hold the order outside the book until its trigger fires

## Input contract

- Each open order has a unique identifier. An identifier may be reused after its previous order
  leaves the book.
- Quantities represent non-negative magnitudes. Zero is valid, including for fills and reductions.
  The caller validates this requirement.
- Subtracting a smaller quantity from a larger quantity produces a valid non-negative remainder.
- Prices have a lawful, stable total ordering. They do not need to be numeric or positive.
- An order's identifier, price, and side remain stable while it rests. `set_quantity` changes only
  its quantity.

## Implementations

- `VecBook` is the reference implementation. It stores each side in a sorted vector.
- `LevelBook` maps prices through balanced trees, stores orders and levels in generational vector
  arenas, and indexes identifiers for direct lookup, cancellation, and reduction.
- `FlatLevelBook` uses the same arena-backed FIFO levels and identifier index, but keeps prices in
  sorted vectors. It targets books with relatively few active price levels.

## Performance

On one thread of an AMD Ryzen 9 9950X, `FlatLevelBook` processes roughly 50 million
book actions per second across the synthetic workloads. Each workload replays 100,000
submissions, cancellations, and reductions; one submission may execute several resting orders.

| Workload     | `VecBook` | `LevelBook` | `FlatLevelBook` |
| ------------ | --------: | ----------: | --------------: |
| Mixed        |    0.6M/s |     37.3M/s |         49.0M/s |
| Cancel-heavy |    0.5M/s |     45.0M/s |         57.3M/s |
| Sweep-heavy  |    2.0M/s |     36.4M/s |         42.9M/s |
| Empirical    |    1.2M/s |     44.4M/s |         52.3M/s |

These are Criterion mean throughputs measured with Rust 1.95.0 and
`-C target-cpu=native`. Results depend on hardware, compiler, book shape, and action mix. See the
[benchmark notes](benches/README.md) for workload definitions and commands.

## When Lobster is not a fit

Choose another matching engine when order behavior must run atomically inside the matching loop and
cannot be expressed through limit submissions, cancellations, and reductions. This includes:

- pro-rata, size-priority, auction, or other non-price-time matching
- native iceberg refresh, pegged, discretionary, stop, or conditional-order state machines
- atomic multi-leg, spread, or cross-book matching

Rich order structs are supported because the book carries their fields unchanged. Matching behavior
remains limited to price-time-priority limit orders.

## Example

```rust
use lobster::{Fill, OrderBook, SimpleOrder, VecBook};

let mut book = VecBook::default();

let _ = book.submit(SimpleOrder::sell(0, 2, 5));
let maker = SimpleOrder::sell(1, 3, 6);
let _ = book.submit(maker);
let _ = book.submit(SimpleOrder::sell(2, 4, 7));
book.cancel(&0);

let fills = book.submit(SimpleOrder::buy(3, 6, 6));

assert_eq!(fills, [Fill::Full(maker)]);
```

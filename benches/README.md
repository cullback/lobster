# Benchmarks

All traces are generated or loaded before Criterion starts timing. Run the
benchmarks with host CPU optimizations through `just`.

- `just bench-synthetic` compares complete mixed workloads.
- `just bench-quantcup` replays the repository's external QuantCup trace.
- `just bench-operations` isolates lookup, cancellation, reduction, insertion,
  sweeping, large-order cloning, price-level count crossover, and integer hash lookup.

Trace replays compare `VecBook`, tree-indexed `LevelBook`, and contiguous-price-indexed
`FlatLevelBook`. The `price_levels` operation group varies the active level count from 8 to 8,192
and measures existing-level, new-best, and worst-case new-level insertion. The `level_lookup` group
measures cancellation at best and middle levels while varying active levels over the same range.

## Synthetic workloads

The structural workloads intentionally stress different implementation paths:

- `mixed` combines submissions, cancellations, reductions, and crossing orders.
- `cancel-heavy` keeps a deep book while emphasizing arbitrary cancellation.
- `sweep-heavy` emphasizes matching across several levels.

The `empirical` workload is inspired by published market microstructure results.
It uses heavy-tailed placement offsets around the current best quote, clustered
quiet/normal/volatile regimes, round-lot and heavy-tailed quantities, and
cancellation biased toward recent and near-quote orders.

The generator prints realized event percentages, crossing rates, fill counts,
resting depth, level counts, and distribution percentiles before measurement.
These statistics are part of benchmark review; the workload is not claimed to
be a calibrated model of a specific venue.

Useful empirical references:

- Mike and Farmer, [An empirical behavioral model of price
  formation](https://arxiv.org/abs/physics/0509194), motivates heavy-tailed
  placement and state-dependent cancellation.
- Muni Toke, [Stepping out of the limit order book](https://mpra.ub.uni-muenchen.de/70291/), studies short-lived and
  cancellation-dominated EBS FX orders.
- Bacry et al., [High-dimensional Hawkes processes for limit order
  books](https://ideas.repec.org/p/hal/journl/hal-01686122.html), motivates
  clustered and cross-exciting event regimes.

## Real traces

`quantcup` is a replay benchmark rather than a synthetic model. Future trace
calibration can use official [Nasdaq TotalView-ITCH samples](https://emi.nasdaq.com/ITCH/Nasdaq%20ITCH/) or free LOBSTER samples.
ITCH is a market-data protocol, so it is most directly useful for calibrating
add/cancel ratios, lifetimes, depth, and price placement rather than recovering
all original aggressive orders.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use lobster::{FlatLevelBook, LevelBook, Order, OrderBook, SimpleOrder, VecBook};
use rustc_hash::FxHashMap;

fn same_price_book<BookType>(orders: u32) -> BookType
where
    BookType: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = BookType::default();
    for order_id in 0..orders {
        let _ = book.submit(SimpleOrder::buy(order_id, 10, 100));
    }
    book
}

fn ask_book<BookType>(orders: u32) -> BookType
where
    BookType: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = BookType::default();
    for order_id in 0..orders {
        let _ = book.submit(SimpleOrder::sell(order_id, 1, 100 + order_id % 32));
    }
    book
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LargeOrder {
    id: u64,
    quantity: u64,
    price: u64,
    is_buy: bool,
    payload: [u8; 256],
}

impl LargeOrder {
    fn buy(id: u64, quantity: u64, price: u64) -> Self {
        Self {
            id,
            quantity,
            price,
            is_buy: true,
            payload: [7; 256],
        }
    }

    fn sell(id: u64, quantity: u64, price: u64) -> Self {
        Self {
            id,
            quantity,
            price,
            is_buy: false,
            payload: [7; 256],
        }
    }
}

impl Order for LargeOrder {
    type OrderId = u64;
    type Quantity = u64;
    type Price = u64;

    fn id(&self) -> &Self::OrderId {
        &self.id
    }

    fn quantity(&self) -> &Self::Quantity {
        &self.quantity
    }

    fn price(&self) -> &Self::Price {
        &self.price
    }

    fn is_buy(&self) -> bool {
        self.is_buy
    }

    fn set_quantity(&mut self, quantity: Self::Quantity) {
        self.quantity = quantity;
    }
}

#[derive(Default)]
struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        self.0 = hash;
    }

    fn write_u32(&mut self, value: u32) {
        self.0 = u64::from(value);
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }
}

type IdentityHashMap<Key, Value> = HashMap<Key, Value, BuildHasherDefault<IdentityHasher>>;

#[allow(clippy::too_many_lines)]
fn operation_benchmarks(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("operations");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));

    for orders in [1_000_u32, 10_000, 100_000] {
        let vecbook = same_price_book::<VecBook<SimpleOrder>>(orders);
        let levelbook = same_price_book::<LevelBook<SimpleOrder>>(orders);
        let flatbook = same_price_book::<FlatLevelBook<SimpleOrder>>(orders);
        let target = orders / 2;

        group.bench_function(BenchmarkId::new("cancel_middle_vec", orders), |bencher| {
            bencher.iter_batched_ref(
                || vecbook.clone(),
                |book| black_box(book.cancel(black_box(&target))),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(BenchmarkId::new("cancel_middle_level", orders), |bencher| {
            bencher.iter_batched_ref(
                || levelbook.clone(),
                |book| black_box(book.cancel(black_box(&target))),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(BenchmarkId::new("cancel_middle_flat", orders), |bencher| {
            bencher.iter_batched_ref(
                || flatbook.clone(),
                |book| black_box(book.cancel(black_box(&target))),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(BenchmarkId::new("reduce_middle_vec", orders), |bencher| {
            bencher.iter_batched_ref(
                || vecbook.clone(),
                |book| black_box(book.reduce(black_box(&target), black_box(5))),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(BenchmarkId::new("reduce_middle_level", orders), |bencher| {
            bencher.iter_batched_ref(
                || levelbook.clone(),
                |book| black_box(book.reduce(black_box(&target), black_box(5))),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(BenchmarkId::new("reduce_middle_flat", orders), |bencher| {
            bencher.iter_batched_ref(
                || flatbook.clone(),
                |book| black_box(book.reduce(black_box(&target), black_box(5))),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(BenchmarkId::new("get_middle_vec", orders), |bencher| {
            bencher.iter(|| black_box(vecbook.get(black_box(&target))));
        });
        group.bench_function(BenchmarkId::new("get_middle_level", orders), |bencher| {
            bencher.iter(|| black_box(levelbook.get(black_box(&target))));
        });
        group.bench_function(BenchmarkId::new("get_middle_flat", orders), |bencher| {
            bencher.iter(|| black_box(flatbook.get(black_box(&target))));
        });
    }

    let mut vecbook = same_price_book::<VecBook<SimpleOrder>>(20_000);
    let mut levelbook = same_price_book::<LevelBook<SimpleOrder>>(20_000);
    let mut flatbook = same_price_book::<FlatLevelBook<SimpleOrder>>(20_000);
    // Grow indexes once, then free the slot so insertion measures steady-state reuse rather than a
    // coincidental hash-table capacity boundary.
    let spare = SimpleOrder::buy(20_000, 10, 98);
    let _ = vecbook.submit(spare);
    let _ = levelbook.submit(spare);
    let _ = flatbook.submit(spare);
    let _ = vecbook.cancel(&spare.id());
    let _ = levelbook.cancel(&spare.id());
    let _ = flatbook.cancel(&spare.id());
    for (name, order) in [
        ("existing_level", SimpleOrder::buy(20_000, 10, 100)),
        ("new_level", SimpleOrder::buy(20_000, 10, 101)),
    ] {
        group.bench_function(BenchmarkId::new(name, "vecbook"), |bencher| {
            bencher.iter_batched_ref(
                || vecbook.clone(),
                |book| black_box(book.submit(black_box(order)).len()),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(BenchmarkId::new(name, "levelbook"), |bencher| {
            bencher.iter_batched_ref(
                || levelbook.clone(),
                |book| black_box(book.submit(black_box(order)).len()),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(BenchmarkId::new(name, "flatbook"), |bencher| {
            bencher.iter_batched_ref(
                || flatbook.clone(),
                |book| black_box(book.submit(black_box(order)).len()),
                BatchSize::SmallInput,
            );
        });
    }

    for sweep in [1_u32, 10, 100, 1_000] {
        let vecbook = ask_book::<VecBook<SimpleOrder>>(sweep);
        let levelbook = ask_book::<LevelBook<SimpleOrder>>(sweep);
        let flatbook = ask_book::<FlatLevelBook<SimpleOrder>>(sweep);
        let taker = SimpleOrder::buy(sweep, sweep, 1_000);
        group.throughput(Throughput::Elements(u64::from(sweep)));
        group.bench_function(BenchmarkId::new("sweep_vec", sweep), |bencher| {
            bencher.iter_batched_ref(
                || vecbook.clone(),
                |book| black_box(book.submit(black_box(taker)).len()),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(BenchmarkId::new("sweep_level", sweep), |bencher| {
            bencher.iter_batched_ref(
                || levelbook.clone(),
                |book| black_box(book.submit(black_box(taker)).len()),
                BatchSize::SmallInput,
            );
        });
        group.bench_function(BenchmarkId::new("sweep_flat", sweep), |bencher| {
            bencher.iter_batched_ref(
                || flatbook.clone(),
                |book| black_box(book.submit(black_box(taker)).len()),
                BatchSize::SmallInput,
            );
        });
    }

    let iteration_orders = 100_000;
    let iteration_vecbook = same_price_book::<VecBook<SimpleOrder>>(iteration_orders);
    let iteration_levelbook = same_price_book::<LevelBook<SimpleOrder>>(iteration_orders);
    let iteration_flatbook = same_price_book::<FlatLevelBook<SimpleOrder>>(iteration_orders);
    group.throughput(Throughput::Elements(u64::from(iteration_orders)));
    group.bench_function("iterate/vecbook", |bencher| {
        bencher.iter(|| {
            black_box(
                iteration_vecbook
                    .iter()
                    .fold(0_u64, |sum, order| sum.wrapping_add(u64::from(order.id()))),
            )
        });
    });
    group.bench_function("iterate/levelbook", |bencher| {
        bencher.iter(|| {
            black_box(
                iteration_levelbook
                    .iter()
                    .fold(0_u64, |sum, order| sum.wrapping_add(u64::from(order.id()))),
            )
        });
    });
    group.bench_function("iterate/flatbook", |bencher| {
        bencher.iter(|| {
            black_box(
                iteration_flatbook
                    .iter()
                    .fold(0_u64, |sum, order| sum.wrapping_add(u64::from(order.id()))),
            )
        });
    });

    let mut large_vecbook = VecBook::default();
    let mut large_levelbook = LevelBook::default();
    let mut large_flatbook = FlatLevelBook::default();
    let maker = LargeOrder::sell(0, 1_000, 100);
    let _ = large_vecbook.submit(maker.clone());
    let _ = large_levelbook.submit(maker.clone());
    let _ = large_flatbook.submit(maker);
    let taker = LargeOrder::buy(1, 1, 100);
    group.throughput(Throughput::Elements(1));
    group.bench_function("partial_large_order/vecbook", |bencher| {
        bencher.iter_batched_ref(
            || (large_vecbook.clone(), Some(taker.clone())),
            |(book, taker)| {
                let taker = taker.take().expect("taker was prepared");
                black_box(book.submit(black_box(taker)).len())
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("partial_large_order/levelbook", |bencher| {
        bencher.iter_batched_ref(
            || (large_levelbook.clone(), Some(taker.clone())),
            |(book, taker)| {
                let taker = taker.take().expect("taker was prepared");
                black_box(book.submit(black_box(taker)).len())
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("partial_large_order/flatbook", |bencher| {
        bencher.iter_batched_ref(
            || (large_flatbook.clone(), Some(taker.clone())),
            |(book, taker)| {
                let taker = taker.take().expect("taker was prepared");
                black_box(book.submit(black_box(taker)).len())
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn distinct_level_book<BookType>(levels: u32) -> BookType
where
    BookType: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = BookType::default();
    for order_id in 0..levels {
        let _ = book.submit(SimpleOrder::buy(order_id, 10, 10_000 + order_id));
    }
    book
}

fn clone_with_spare_level<BookType>(book: &BookType, spare: SimpleOrder) -> BookType
where
    BookType: Clone + OrderBook<Order = SimpleOrder>,
{
    let mut clone = book.clone();
    // Vec::clone does not preserve spare capacity. Grow and remove a level after cloning so the
    // timed insertion measures steady-state capacity reuse rather than forced reallocation.
    let _ = clone.submit(spare);
    let _ = clone.cancel(&spare.id());
    clone
}

fn layered_book<BookType>(levels: u32, orders_per_level: u32) -> BookType
where
    BookType: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = BookType::default();
    for level in 0..levels {
        for position in 0..orders_per_level {
            let order_id = level * orders_per_level + position;
            let _ = book.submit(SimpleOrder::buy(order_id, 10, 10_000 + level));
        }
    }
    book
}

fn level_lookup_benchmarks(criterion: &mut Criterion) {
    const ORDERS_PER_LEVEL: u32 = 4;
    let mut group = criterion.benchmark_group("level_lookup");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(3));

    for levels in [8_u32, 128, 2_048, 8_192] {
        let levelbook = layered_book::<LevelBook<SimpleOrder>>(levels, ORDERS_PER_LEVEL);
        let flatbook = layered_book::<FlatLevelBook<SimpleOrder>>(levels, ORDERS_PER_LEVEL);
        let middle_order = (levels / 2) * ORDERS_PER_LEVEL + 1;
        let best_order = (levels - 1) * ORDERS_PER_LEVEL + 1;

        for (position, order_id) in [("middle", middle_order), ("best", best_order)] {
            group.bench_function(
                BenchmarkId::new(format!("cancel_{position}_level"), levels),
                |bencher| {
                    bencher.iter_batched_ref(
                        || levelbook.clone(),
                        |book| black_box(book.cancel(black_box(&order_id))),
                        BatchSize::SmallInput,
                    );
                },
            );
            group.bench_function(
                BenchmarkId::new(format!("cancel_{position}_flat"), levels),
                |bencher| {
                    bencher.iter_batched_ref(
                        || flatbook.clone(),
                        |book| black_box(book.cancel(black_box(&order_id))),
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    }
    group.finish();
}

fn price_level_benchmarks(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("price_levels");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(2));

    for levels in [8_u32, 32, 128, 512, 2_048, 8_192] {
        let levelbook = distinct_level_book::<LevelBook<SimpleOrder>>(levels);
        let flatbook = distinct_level_book::<FlatLevelBook<SimpleOrder>>(levels);
        let spare = SimpleOrder::buy(levels + 1, 10, 9_000);
        let inputs = [
            (
                "existing",
                SimpleOrder::buy(levels, 10, 10_000 + levels / 2),
            ),
            ("new_best", SimpleOrder::buy(levels, 10, 10_000 + levels)),
            ("new_worst", SimpleOrder::buy(levels, 10, 9_999)),
        ];

        for (operation, order) in inputs {
            group.bench_function(
                BenchmarkId::new(format!("{operation}_level"), levels),
                |bencher| {
                    bencher.iter_batched_ref(
                        || clone_with_spare_level(&levelbook, spare),
                        |book| black_box(book.submit(black_box(order)).len()),
                        BatchSize::SmallInput,
                    );
                },
            );
            group.bench_function(
                BenchmarkId::new(format!("{operation}_flat"), levels),
                |bencher| {
                    bencher.iter_batched_ref(
                        || clone_with_spare_level(&flatbook, spare),
                        |book| black_box(book.submit(black_box(order)).len()),
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    }
    group.finish();
}

fn hash_benchmarks(criterion: &mut Criterion) {
    const ENTRIES: u32 = 100_000;
    let std_map = (0..ENTRIES)
        .map(|value| (value, value))
        .collect::<HashMap<_, _>>();
    let fx_map = (0..ENTRIES)
        .map(|value| (value, value))
        .collect::<FxHashMap<_, _>>();
    let identity_map = (0..ENTRIES)
        .map(|value| (value, value))
        .collect::<IdentityHashMap<_, _>>();
    let keys = (0..ENTRIES).rev().collect::<Vec<_>>();

    let mut group = criterion.benchmark_group("hash_lookup");
    group.throughput(Throughput::Elements(u64::from(ENTRIES)));
    group.bench_function("std", |bencher| {
        bencher.iter(|| {
            let mut checksum = 0_u64;
            for key in black_box(&keys) {
                checksum += u64::from(std_map[key]);
            }
            black_box(checksum)
        });
    });
    group.bench_function("fx", |bencher| {
        bencher.iter(|| {
            let mut checksum = 0_u64;
            for key in black_box(&keys) {
                checksum += u64::from(fx_map[key]);
            }
            black_box(checksum)
        });
    });
    group.bench_function("identity", |bencher| {
        bencher.iter(|| {
            let mut checksum = 0_u64;
            for key in black_box(&keys) {
                checksum += u64::from(identity_map[key]);
            }
            black_box(checksum)
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    operation_benchmarks,
    level_lookup_benchmarks,
    price_level_benchmarks,
    hash_benchmarks
);
criterion_main!(benches);

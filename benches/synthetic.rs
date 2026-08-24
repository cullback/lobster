use std::collections::HashMap;
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use lobster::{Fill, LevelBook, OrderBook, SimpleOrder, VecBook};

const MID_PRICE: u32 = 10_000;

#[derive(Clone, Copy, Debug)]
enum Action {
    Submit(SimpleOrder),
    Cancel(u32),
    Reduce { order_id: u32, quantity: u32 },
}

#[derive(Clone, Copy)]
struct Workload {
    name: &'static str,
    seed: u64,
    initial_orders: usize,
    actions: usize,
    submit_percent: u32,
    cancel_percent: u32,
    marketable_percent: u32,
    price_levels: u32,
    max_cross_ticks: u32,
}

struct ActiveOrders {
    ids: Vec<u32>,
    positions: HashMap<u32, usize>,
}

impl ActiveOrders {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            ids: Vec::with_capacity(capacity),
            positions: HashMap::with_capacity(capacity),
        }
    }

    fn insert(&mut self, order_id: u32) {
        if self.positions.contains_key(&order_id) {
            return;
        }
        self.positions.insert(order_id, self.ids.len());
        self.ids.push(order_id);
    }

    fn remove(&mut self, order_id: u32) {
        let Some(index) = self.positions.remove(&order_id) else {
            return;
        };
        self.ids.swap_remove(index);
        if index < self.ids.len() {
            self.positions.insert(self.ids[index], index);
        }
    }

    fn choose(&self, rng: &mut Rng) -> Option<u32> {
        if self.ids.is_empty() {
            None
        } else {
            Some(self.ids[rng.index(self.ids.len())])
        }
    }
}

struct Rng(u64);

impl Rng {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64*: deterministic and sufficient for workload generation.
        let mut value = self.0;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.0 = value;
        value.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn range(&mut self, upper: u32) -> u32 {
        debug_assert!(upper > 0);
        (self.next_u64() % u64::from(upper)) as u32
    }

    fn index(&mut self, upper: usize) -> usize {
        debug_assert!(upper > 0);
        self.next_u64() as usize % upper
    }

    fn chance(&mut self, percent: u32) -> bool {
        self.range(100) < percent
    }
}

fn quantity(rng: &mut Rng) -> u32 {
    match rng.range(100) {
        0..=59 => 1 + rng.range(10),
        60..=89 => 10 + rng.range(91),
        _ => 100 + rng.range(901),
    }
}

fn passive_order(
    order_id: u32,
    is_buy: bool,
    mid_price: u32,
    price_levels: u32,
    rng: &mut Rng,
) -> SimpleOrder {
    let offset = 1 + rng.range(price_levels);
    let price = if is_buy {
        mid_price.saturating_sub(offset)
    } else {
        mid_price.saturating_add(offset)
    };
    if is_buy {
        SimpleOrder::buy(order_id, quantity(rng), price)
    } else {
        SimpleOrder::sell(order_id, quantity(rng), price)
    }
}

fn marketable_order(
    order_id: u32,
    is_buy: bool,
    mid_price: u32,
    max_cross_ticks: u32,
    rng: &mut Rng,
) -> SimpleOrder {
    let offset = 1 + rng.range(max_cross_ticks);
    let price = if is_buy {
        mid_price.saturating_add(offset)
    } else {
        mid_price.saturating_sub(offset)
    };
    if is_buy {
        SimpleOrder::buy(order_id, quantity(rng), price)
    } else {
        SimpleOrder::sell(order_id, quantity(rng), price)
    }
}

fn record_submit(book: &mut VecBook<SimpleOrder>, active: &mut ActiveOrders, order: SimpleOrder) {
    let order_id = order.id();
    for fill in book.submit(order) {
        if let Fill::Full(maker) = fill {
            active.remove(maker.id());
        }
    }
    if book.contains(&order_id) {
        active.insert(order_id);
    }
}

fn generate(config: Workload) -> (VecBook<SimpleOrder>, Vec<Action>) {
    let mut rng = Rng::new(config.seed);
    let mut book = VecBook::new();
    let mut active = ActiveOrders::with_capacity(config.initial_orders + config.actions / 2);
    let mut next_order_id = 0_u32;

    // Warm the book with balanced, non-crossing liquidity before taking the benchmark snapshot.
    for index in 0..config.initial_orders {
        let order = passive_order(
            next_order_id,
            index % 2 == 0,
            MID_PRICE,
            config.price_levels,
            &mut rng,
        );
        next_order_id += 1;
        record_submit(&mut book, &mut active, order);
    }
    let initial_book = book.clone();

    let mut actions = Vec::with_capacity(config.actions);
    let mut mid_price = MID_PRICE;

    while actions.len() < config.actions {
        // A slow random walk creates changing but bounded price regimes.
        if rng.chance(3) {
            mid_price = if rng.chance(50) {
                mid_price.saturating_add(1)
            } else {
                mid_price.saturating_sub(1).max(100)
            };
        }

        let action_roll = rng.range(100);
        if action_roll < config.submit_percent || active.ids.is_empty() {
            let is_buy = rng.chance(50);
            let order = if rng.chance(config.marketable_percent) {
                marketable_order(
                    next_order_id,
                    is_buy,
                    mid_price,
                    config.max_cross_ticks,
                    &mut rng,
                )
            } else {
                passive_order(
                    next_order_id,
                    is_buy,
                    mid_price,
                    config.price_levels,
                    &mut rng,
                )
            };
            next_order_id += 1;
            actions.push(Action::Submit(order));
            record_submit(&mut book, &mut active, order);
        } else if action_roll < config.submit_percent + config.cancel_percent {
            let order_id = active.choose(&mut rng).expect("active order was present");
            actions.push(Action::Cancel(order_id));
            let removed = book.cancel(&order_id).expect("generated cancel was valid");
            active.remove(removed.id());
        } else {
            let order_id = active.choose(&mut rng).expect("active order was present");
            let current_quantity = book
                .get(&order_id)
                .expect("generated reduction target was valid")
                .quantity();
            if current_quantity <= 1 {
                actions.push(Action::Cancel(order_id));
                let removed = book.cancel(&order_id).expect("generated cancel was valid");
                active.remove(removed.id());
            } else {
                let new_quantity = 1 + rng.range(current_quantity - 1);
                actions.push(Action::Reduce {
                    order_id,
                    quantity: new_quantity,
                });
                book.reduce(&order_id, new_quantity)
                    .expect("generated reduction was valid");
            }
        }
    }

    (initial_book, actions)
}

fn replay<BookType>(book: &mut BookType, actions: &[Action]) -> u64
where
    BookType: OrderBook<Order = SimpleOrder>,
{
    let mut checksum = 0_u64;
    for action in actions {
        match *action {
            Action::Submit(order) => {
                let fills = book.submit(order);
                checksum = checksum.wrapping_add(fills.len() as u64);
                for fill in fills {
                    checksum = checksum
                        .wrapping_mul(31)
                        .wrapping_add(u64::from(fill.maker().id()))
                        .wrapping_add(u64::from(*fill.quantity()));
                }
            }
            Action::Cancel(order_id) => {
                if let Some(order) = book.cancel(&order_id) {
                    checksum = checksum.wrapping_add(u64::from(order.id()));
                }
            }
            Action::Reduce { order_id, quantity } => {
                checksum =
                    checksum.wrapping_add(u64::from(book.reduce(&order_id, quantity).is_ok()));
            }
        }
    }
    checksum
}

fn synthetic_benchmarks(criterion: &mut Criterion) {
    let workloads = [
        Workload {
            name: "mixed",
            seed: 0x004D_4958_4544,
            initial_orders: 20_000,
            actions: 100_000,
            submit_percent: 55,
            cancel_percent: 25,
            marketable_percent: 20,
            price_levels: 64,
            max_cross_ticks: 8,
        },
        Workload {
            name: "cancel_heavy",
            seed: 0x4341_4E43_454C,
            initial_orders: 40_000,
            actions: 100_000,
            submit_percent: 30,
            cancel_percent: 60,
            marketable_percent: 5,
            price_levels: 64,
            max_cross_ticks: 4,
        },
        Workload {
            name: "sweep_heavy",
            seed: 0x0053_5745_4550,
            initial_orders: 20_000,
            actions: 100_000,
            submit_percent: 70,
            cancel_percent: 20,
            marketable_percent: 70,
            price_levels: 32,
            max_cross_ticks: 16,
        },
    ];

    let mut group = criterion.benchmark_group("synthetic");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(5));

    for workload in workloads {
        let (initial_vecbook, actions) = generate(workload);
        let mut initial_levelbook = LevelBook::new();
        for order in initial_vecbook.iter().copied() {
            let _ = initial_levelbook.submit(order);
        }

        let expected = replay(&mut initial_vecbook.clone(), &actions);
        let actual = replay(&mut initial_levelbook.clone(), &actions);
        assert_eq!(actual, expected, "implementations diverged");

        group.throughput(Throughput::Elements(actions.len() as u64));
        group.bench_function(BenchmarkId::new(workload.name, "vecbook"), |bencher| {
            bencher.iter_batched(
                || initial_vecbook.clone(),
                |mut book| black_box(replay(&mut book, black_box(&actions))),
                BatchSize::LargeInput,
            );
        });
        group.bench_function(BenchmarkId::new(workload.name, "levelbook"), |bencher| {
            bencher.iter_batched(
                || initial_levelbook.clone(),
                |mut book| black_box(replay(&mut book, black_box(&actions))),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, synthetic_benchmarks);
criterion_main!(benches);

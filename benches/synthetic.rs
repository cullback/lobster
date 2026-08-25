use std::collections::{BTreeMap, HashMap};
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use lobster::{Fill, FlatLevelBook, LevelBook, OrderBook, SimpleOrder, VecBook};

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
    empirical: bool,
}

struct ActiveOrders {
    ids: Vec<u32>,
    positions: HashMap<u32, usize>,
    births: HashMap<u32, usize>,
}

impl ActiveOrders {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            ids: Vec::with_capacity(capacity),
            positions: HashMap::with_capacity(capacity),
            births: HashMap::with_capacity(capacity),
        }
    }

    fn insert(&mut self, order_id: u32, birth: usize) {
        if self.positions.contains_key(&order_id) {
            return;
        }
        self.positions.insert(order_id, self.ids.len());
        self.births.insert(order_id, birth);
        self.ids.push(order_id);
    }

    fn remove(&mut self, order_id: u32) {
        let Some(index) = self.positions.remove(&order_id) else {
            return;
        };
        self.births.remove(&order_id);
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

    fn choose_empirical(&self, book: &VecBook<SimpleOrder>, rng: &mut Rng) -> Option<u32> {
        let first = self.choose(rng)?;
        let mode = rng.range(100);
        let mut selected = first;

        for _ in 0..15 {
            let candidate = self.choose(rng).expect("active order was present");
            if mode < 60 {
                if self.births[&candidate] > self.births[&selected] {
                    selected = candidate;
                }
            } else if mode < 85 {
                let candidate_distance = distance_from_best(book, candidate);
                let selected_distance = distance_from_best(book, selected);
                if candidate_distance < selected_distance {
                    selected = candidate;
                }
            }
        }
        Some(selected)
    }
}

fn usize_to_u32(value: usize) -> u32 {
    u32::try_from(value).expect("trace count did not fit u32")
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).expect("trace count did not fit u64")
}

fn usize_to_f64(value: usize) -> f64 {
    f64::from(usize_to_u32(value))
}

struct Rng(u64);

impl Rng {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.0 = value;
        value.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn range(&mut self, upper: u32) -> u32 {
        debug_assert!(upper > 0);
        u32::try_from(self.next_u64() % u64::from(upper)).expect("value reduced modulo u32 bound")
    }

    fn index(&mut self, upper: usize) -> usize {
        debug_assert!(upper > 0);
        let upper = u64::try_from(upper).expect("index bound did not fit u64");
        usize::try_from(self.next_u64() % upper).expect("index did not fit usize")
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

fn empirical_quantity(rng: &mut Rng) -> u32 {
    match rng.range(100) {
        0..=29 => 100,
        30..=44 => 200,
        45..=54 => 500,
        55..=84 => 1 + rng.range(100),
        85..=96 => 100 + rng.range(901),
        _ => 1_000 + rng.range(9_001),
    }
}

fn heavy_tailed_offset(rng: &mut Rng) -> u32 {
    match rng.range(1_000) {
        0..=599 => rng.range(2),
        600..=849 => 2 + rng.range(4),
        850..=949 => 6 + rng.range(10),
        950..=989 => 16 + rng.range(48),
        _ => 64 + rng.range(192),
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

fn empirical_order(
    book: &VecBook<SimpleOrder>,
    order_id: u32,
    is_buy: bool,
    marketable: bool,
    max_cross_ticks: u32,
    rng: &mut Rng,
) -> SimpleOrder {
    let price = if marketable {
        let cross = 1 + rng.range(max_cross_ticks);
        if is_buy {
            book.best_ask()
                .map_or(MID_PRICE + cross, |ask| ask.price().saturating_add(cross))
        } else {
            book.best_bid()
                .map_or(MID_PRICE.saturating_sub(cross), |bid| {
                    bid.price().saturating_sub(cross)
                })
        }
    } else {
        let offset = heavy_tailed_offset(rng);
        if is_buy {
            book.best_bid().map_or(MID_PRICE.saturating_sub(1), |bid| {
                bid.price().saturating_sub(offset)
            })
        } else {
            book.best_ask()
                .map_or(MID_PRICE + 1, |ask| ask.price().saturating_add(offset))
        }
    };
    let quantity = empirical_quantity(rng);
    if is_buy {
        SimpleOrder::buy(order_id, quantity, price)
    } else {
        SimpleOrder::sell(order_id, quantity, price)
    }
}

fn distance_from_best(book: &VecBook<SimpleOrder>, order_id: u32) -> u32 {
    let Some(order) = book.get(&order_id) else {
        return u32::MAX;
    };
    if order.is_buy() {
        book.best_bid()
            .map_or(0, |best| best.price().saturating_sub(order.price()))
    } else {
        book.best_ask()
            .map_or(0, |best| order.price().saturating_sub(best.price()))
    }
}

fn record_submit(
    book: &mut VecBook<SimpleOrder>,
    active: &mut ActiveOrders,
    order: SimpleOrder,
    event: usize,
) {
    let order_id = order.id();
    for fill in book.submit(order) {
        if let Fill::Full(maker) = fill {
            active.remove(maker.id());
        }
    }
    if book.contains(&order_id) {
        active.insert(order_id, event);
    }
}

fn generate(config: Workload) -> (VecBook<SimpleOrder>, Vec<Action>) {
    let mut rng = Rng::new(config.seed);
    let mut book = VecBook::default();
    let mut active = ActiveOrders::with_capacity(config.initial_orders + config.actions / 2);
    let mut next_order_id = 0_u32;

    for index in 0..config.initial_orders {
        let order = passive_order(
            next_order_id,
            index % 2 == 0,
            MID_PRICE,
            config.price_levels,
            &mut rng,
        );
        next_order_id += 1;
        record_submit(&mut book, &mut active, order, 0);
    }
    let initial_book = book.clone();

    let mut actions = Vec::with_capacity(config.actions);
    let mut mid_price = MID_PRICE;
    let mut regime = 1;

    while actions.len() < config.actions {
        let event = actions.len() + 1;
        if config.empirical && rng.chance(2) {
            regime = rng.range(3);
        }
        if !config.empirical && rng.chance(3) {
            mid_price = if rng.chance(50) {
                mid_price.saturating_add(1)
            } else {
                mid_price.saturating_sub(1).max(100)
            };
        }

        let (submit_percent, cancel_percent, marketable_percent) = if config.empirical {
            match regime {
                0 => (52, 43, 4),
                1 => (50, 44, 12),
                _ => (47, 43, 40),
            }
        } else {
            (
                config.submit_percent,
                config.cancel_percent,
                config.marketable_percent,
            )
        };

        let action_roll = rng.range(100);
        if action_roll < submit_percent || active.ids.is_empty() {
            let is_buy = rng.chance(50);
            let order = if config.empirical {
                let marketable = rng.chance(marketable_percent);
                empirical_order(
                    &book,
                    next_order_id,
                    is_buy,
                    marketable,
                    config.max_cross_ticks,
                    &mut rng,
                )
            } else if rng.chance(marketable_percent) {
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
            record_submit(&mut book, &mut active, order, event);
        } else if action_roll < submit_percent + cancel_percent {
            let order_id = if config.empirical {
                active
                    .choose_empirical(&book, &mut rng)
                    .expect("active order was present")
            } else {
                active.choose(&mut rng).expect("active order was present")
            };
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

#[derive(Default)]
struct TraceStats {
    submits: usize,
    cancels: usize,
    reductions: usize,
    marketable: usize,
    full_fills: usize,
    partial_fills: usize,
    live_sum: usize,
    live_max: usize,
    samples: usize,
    level_max: usize,
    quantities: Vec<u32>,
    fills_per_submit: Vec<u32>,
    spreads: Vec<u32>,
    lifetimes: Vec<u32>,
    cancel_distances: Vec<u32>,
}

fn percentile(values: &mut [u32], percentile: usize) -> u32 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[(values.len() - 1) * percentile / 100]
}

fn is_marketable(book: &VecBook<SimpleOrder>, order: SimpleOrder) -> bool {
    if order.is_buy() {
        book.best_ask()
            .is_some_and(|ask| order.price() >= ask.price())
    } else {
        book.best_bid()
            .is_some_and(|bid| order.price() <= bid.price())
    }
}

fn collect_stats(initial_book: &VecBook<SimpleOrder>, actions: &[Action]) -> TraceStats {
    let mut book = initial_book.clone();
    let mut births = book
        .iter()
        .map(|order| (order.id(), 0_usize))
        .collect::<HashMap<_, _>>();
    let mut stats = TraceStats::default();

    for (index, action) in actions.iter().enumerate() {
        let event = index + 1;
        match *action {
            Action::Submit(order) => {
                stats.submits += 1;
                stats.quantities.push(order.quantity());
                stats.marketable += usize::from(is_marketable(&book, order));
                let mut completed = Vec::new();
                let fills = book.submit(order);
                stats.fills_per_submit.push(usize_to_u32(fills.len()));
                for fill in fills {
                    match fill {
                        Fill::Full(maker) => {
                            stats.full_fills += 1;
                            completed.push(maker.id());
                        }
                        Fill::Partial { .. } => stats.partial_fills += 1,
                    }
                }
                for order_id in completed {
                    if let Some(birth) = births.remove(&order_id) {
                        stats.lifetimes.push(usize_to_u32(event - birth));
                    }
                }
                if book.contains(&order.id()) {
                    births.insert(order.id(), event);
                }
            }
            Action::Cancel(order_id) => {
                stats.cancels += 1;
                stats
                    .cancel_distances
                    .push(distance_from_best(&book, order_id));
                if book.cancel(&order_id).is_some() {
                    if let Some(birth) = births.remove(&order_id) {
                        stats.lifetimes.push(usize_to_u32(event - birth));
                    }
                }
            }
            Action::Reduce { order_id, quantity } => {
                stats.reductions += 1;
                let _ = book.reduce(&order_id, quantity);
            }
        }

        stats.live_sum += book.len();
        stats.live_max = stats.live_max.max(book.len());
        stats.samples += 1;
        if let (Some(bid), Some(ask)) = (book.best_bid(), book.best_ask()) {
            stats.spreads.push(ask.price().saturating_sub(bid.price()));
        }
        if event % 1_000 == 0 {
            let mut levels = BTreeMap::<(bool, u32), usize>::new();
            for order in book.iter() {
                *levels.entry((order.is_buy(), order.price())).or_default() += 1;
            }
            stats.level_max = stats.level_max.max(levels.len());
        }
    }
    stats
}

fn print_stats(name: &str, mut stats: TraceStats) {
    let actions = stats.submits + stats.cancels + stats.reductions;
    let fills = stats.full_fills + stats.partial_fills;
    let quantity_p50 = percentile(&mut stats.quantities, 50);
    let quantity_p95 = percentile(&mut stats.quantities, 95);
    let fills_p95 = percentile(&mut stats.fills_per_submit, 95);
    let lifetime_p50 = percentile(&mut stats.lifetimes, 50);
    let lifetime_p95 = percentile(&mut stats.lifetimes, 95);
    let spread_p50 = percentile(&mut stats.spreads, 50);
    let cancel_distance_p50 = percentile(&mut stats.cancel_distances, 50);

    eprintln!(
        "trace {name}: actions={actions} submit={:.1}% cancel={:.1}% reduce={:.1}% \
         marketable={:.1}% cancel_exit_share={:.1}% fills={fills} (full={}, partial={}) \
         fills_p95={} live_mean={} live_max={} levels_max={} qty_p50/p95={}/{} \
         lifetime_p50/p95={}/{} spread_p50={} cancel_distance_p50={}",
        100.0 * usize_to_f64(stats.submits) / usize_to_f64(actions),
        100.0 * usize_to_f64(stats.cancels) / usize_to_f64(actions),
        100.0 * usize_to_f64(stats.reductions) / usize_to_f64(actions),
        100.0 * usize_to_f64(stats.marketable) / usize_to_f64(stats.submits.max(1)),
        100.0 * usize_to_f64(stats.cancels)
            / usize_to_f64((stats.cancels + stats.full_fills).max(1)),
        stats.full_fills,
        stats.partial_fills,
        fills_p95,
        stats.live_sum / stats.samples.max(1),
        stats.live_max,
        stats.level_max,
        quantity_p50,
        quantity_p95,
        lifetime_p50,
        lifetime_p95,
        spread_p50,
        cancel_distance_p50,
    );
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
                checksum = checksum.wrapping_add(usize_to_u64(fills.len()));
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

fn workloads() -> [Workload; 4] {
    [
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
            empirical: false,
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
            empirical: false,
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
            empirical: false,
        },
        Workload {
            name: "empirical",
            seed: 0x454D_5049_5249_4341,
            initial_orders: 20_000,
            actions: 100_000,
            submit_percent: 50,
            cancel_percent: 44,
            marketable_percent: 12,
            price_levels: 64,
            max_cross_ticks: 8,
            empirical: true,
        },
    ]
}

fn synthetic_benchmarks(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("synthetic");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(5));

    for workload in workloads() {
        let (initial_vecbook, actions) = generate(workload);
        print_stats(workload.name, collect_stats(&initial_vecbook, &actions));

        let mut initial_levelbook = LevelBook::default();
        let mut initial_flatbook = FlatLevelBook::default();
        for order in initial_vecbook.iter().copied() {
            let _ = initial_levelbook.submit(order);
            let _ = initial_flatbook.submit(order);
        }

        let mut final_vecbook = initial_vecbook.clone();
        let mut final_levelbook = initial_levelbook.clone();
        let mut final_flatbook = initial_flatbook.clone();
        let expected = replay(&mut final_vecbook, &actions);
        let actual = replay(&mut final_levelbook, &actions);
        let flat_actual = replay(&mut final_flatbook, &actions);
        assert_eq!(actual, expected, "LevelBook diverged");
        assert_eq!(flat_actual, expected, "FlatLevelBook diverged");
        assert_eq!(
            final_levelbook.bids().copied().collect::<Vec<_>>(),
            final_vecbook.bids().copied().collect::<Vec<_>>(),
            "final bids diverged",
        );
        assert_eq!(
            final_levelbook.asks().copied().collect::<Vec<_>>(),
            final_vecbook.asks().copied().collect::<Vec<_>>(),
            "final asks diverged",
        );
        assert_eq!(
            final_flatbook.bids().copied().collect::<Vec<_>>(),
            final_vecbook.bids().copied().collect::<Vec<_>>(),
            "FlatLevelBook final bids diverged",
        );
        assert_eq!(
            final_flatbook.asks().copied().collect::<Vec<_>>(),
            final_vecbook.asks().copied().collect::<Vec<_>>(),
            "FlatLevelBook final asks diverged",
        );

        group.throughput(Throughput::Elements(usize_to_u64(actions.len())));
        group.bench_function(BenchmarkId::new(workload.name, "vecbook"), |bencher| {
            bencher.iter_batched_ref(
                || initial_vecbook.clone(),
                |book| black_box(replay(book, black_box(&actions))),
                BatchSize::LargeInput,
            );
        });
        group.bench_function(BenchmarkId::new(workload.name, "levelbook"), |bencher| {
            bencher.iter_batched_ref(
                || initial_levelbook.clone(),
                |book| black_box(replay(book, black_box(&actions))),
                BatchSize::LargeInput,
            );
        });
        group.bench_function(BenchmarkId::new(workload.name, "flatbook"), |bencher| {
            bencher.iter_batched_ref(
                || initial_flatbook.clone(),
                |book| black_box(replay(book, black_box(&actions))),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, synthetic_benchmarks);
criterion_main!(benches);

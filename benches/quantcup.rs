use std::fs::read_to_string;
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use lobster::{FlatLevelBook, LevelBook, OrderBook, SimpleOrder, VecBook};

#[derive(Clone, Copy)]
enum Action {
    Submit(SimpleOrder),
    Cancel(u32),
}

fn load_actions() -> Vec<Action> {
    let file = read_to_string("benches/orders.csv").expect("failed to open QuantCup trace");
    let mut actions = Vec::with_capacity(35_760);
    let mut order_id = 0;

    for line in file.lines().skip(1) {
        let mut fields = line.split(',');
        let _trader_id: u32 = fields.next().expect("missing trader id").parse().unwrap();
        let side = fields.next().expect("missing side");
        let price: u32 = fields.next().expect("missing price").parse().unwrap();
        let quantity: u32 = fields.next().expect("missing quantity").parse().unwrap();

        if price == 0 {
            actions.push(Action::Cancel(quantity));
        } else {
            let order = match side {
                "Bid" => SimpleOrder::buy(order_id, quantity, price),
                "Ask" => SimpleOrder::sell(order_id, quantity, price),
                _ => panic!("invalid side: {side}"),
            };
            actions.push(Action::Submit(order));
            order_id += 1;
        }
    }
    actions
}

fn replay<BookType>(book: &mut BookType, actions: &[Action]) -> u64
where
    BookType: OrderBook<Order = SimpleOrder>,
{
    let mut checksum = 0_u64;
    for action in actions {
        match *action {
            Action::Submit(order) => {
                for fill in book.submit(order) {
                    checksum = checksum
                        .wrapping_mul(31)
                        .wrapping_add(u64::from(fill.maker().id()))
                        .wrapping_add(u64::from(*fill.quantity()));
                }
            }
            Action::Cancel(order_id) => {
                checksum = checksum.wrapping_add(u64::from(book.cancel(&order_id).is_some()));
            }
        }
    }
    checksum.wrapping_add(book.len() as u64)
}

fn quantcup_benchmarks(criterion: &mut Criterion) {
    let actions = load_actions();
    let mut vecbook = VecBook::new();
    let mut levelbook = LevelBook::new();
    let mut flatbook = FlatLevelBook::new();
    let expected = replay(&mut vecbook, &actions);
    assert_eq!(
        replay(&mut levelbook, &actions),
        expected,
        "LevelBook diverged on QuantCup trace",
    );
    assert_eq!(
        replay(&mut flatbook, &actions),
        expected,
        "FlatLevelBook diverged on QuantCup trace",
    );
    assert_eq!(
        levelbook.iter().copied().collect::<Vec<_>>(),
        vecbook.iter().copied().collect::<Vec<_>>(),
        "LevelBook final book diverged on QuantCup trace",
    );
    assert_eq!(
        flatbook.iter().copied().collect::<Vec<_>>(),
        vecbook.iter().copied().collect::<Vec<_>>(),
        "FlatLevelBook final book diverged on QuantCup trace",
    );

    let submits = actions
        .iter()
        .filter(|action| matches!(action, Action::Submit(_)))
        .count();
    eprintln!(
        "QuantCup trace: actions={} submit={:.1}% cancel={:.1}% final_orders={}",
        actions.len(),
        100.0 * submits as f64 / actions.len() as f64,
        100.0 * (actions.len() - submits) as f64 / actions.len() as f64,
        vecbook.len(),
    );

    let mut group = criterion.benchmark_group("quantcup");
    group.sample_size(50);
    group.throughput(Throughput::Elements(actions.len() as u64));
    group.bench_function(BenchmarkId::from_parameter("vecbook"), |bencher| {
        bencher.iter_batched_ref(
            VecBook::new,
            |book| black_box(replay(book, black_box(&actions))),
            criterion::BatchSize::SmallInput,
        );
    });
    group.bench_function(BenchmarkId::from_parameter("levelbook"), |bencher| {
        bencher.iter_batched_ref(
            LevelBook::new,
            |book| black_box(replay(book, black_box(&actions))),
            criterion::BatchSize::SmallInput,
        );
    });
    group.bench_function(BenchmarkId::from_parameter("flatbook"), |bencher| {
        bencher.iter_batched_ref(
            FlatLevelBook::new,
            |book| black_box(replay(book, black_box(&actions))),
            criterion::BatchSize::SmallInput,
        );
    });
    group.finish();
}

criterion_group!(benches, quantcup_benchmarks);
criterion_main!(benches);

use crate::{Fill, FlatLevelBook, LevelBook, Order, OrderBook, ReduceError, SimpleOrder, VecBook};

type Book = VecBook<SimpleOrder>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApplicationSide {
    Bid,
    Offer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApplicationOrder {
    id: String,
    quantity: u32,
    price: u32,
    side: ApplicationSide,
    account: String,
}

impl Order for ApplicationOrder {
    type OrderId = String;
    type Quantity = u32;
    type Price = u32;

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
        matches!(self.side, ApplicationSide::Bid)
    }

    fn set_quantity(&mut self, quantity: Self::Quantity) {
        self.quantity = quantity;
    }
}

fn full(maker: SimpleOrder) -> Fill<SimpleOrder> {
    Fill::Full(maker)
}

fn partial(maker: SimpleOrder, quantity: u32) -> Fill<SimpleOrder> {
    Fill::Partial { maker, quantity }
}

fn assert_same_state<IndexedBook>(vecbook: &Book, indexed: &IndexedBook)
where
    IndexedBook: OrderBook<Order = SimpleOrder>,
{
    assert_eq!(indexed.len(), vecbook.len());
    assert_eq!(
        indexed.bids().copied().collect::<Vec<_>>(),
        vecbook.bids().copied().collect::<Vec<_>>()
    );
    assert_eq!(
        indexed.asks().copied().collect::<Vec<_>>(),
        vecbook.asks().copied().collect::<Vec<_>>()
    );
}

fn submit_both(vecbook: &mut Book, levelbook: &mut LevelBook<SimpleOrder>, order: SimpleOrder) {
    let expected = vecbook.submit(order).to_vec();
    let actual = levelbook.submit(order).to_vec();
    assert_eq!(actual, expected);
    assert_same_state(vecbook, levelbook);
    levelbook.assert_invariants();
}

#[test]
fn levelbook_matches_vecbook_across_mixed_operations() {
    let mut vecbook = Book::new();
    let mut levelbook = LevelBook::new();

    for order in [
        SimpleOrder::buy(0, 4, 99),
        SimpleOrder::buy(1, 3, 100),
        SimpleOrder::buy(2, 2, 100),
        SimpleOrder::sell(3, 2, 101),
        SimpleOrder::sell(4, 5, 101),
        SimpleOrder::sell(5, 3, 102),
    ] {
        submit_both(&mut vecbook, &mut levelbook, order);
    }

    assert_eq!(levelbook.cancel(&4), vecbook.cancel(&4));
    assert_eq!(levelbook.reduce(&1, 2), vecbook.reduce(&1, 2));
    assert_same_state(&vecbook, &levelbook);
    levelbook.assert_invariants();

    submit_both(&mut vecbook, &mut levelbook, SimpleOrder::buy(6, 4, 102));
    submit_both(&mut vecbook, &mut levelbook, SimpleOrder::sell(7, 6, 99));
    submit_both(&mut vecbook, &mut levelbook, SimpleOrder::buy(8, 10, 103));
}

#[test]
fn fills_preserve_application_owned_order_data() {
    let maker = ApplicationOrder {
        id: "maker".into(),
        quantity: 5,
        price: 10,
        side: ApplicationSide::Offer,
        account: "account-a".into(),
    };
    let taker = ApplicationOrder {
        id: "taker".into(),
        quantity: 2,
        price: 10,
        side: ApplicationSide::Bid,
        account: "account-b".into(),
    };
    let mut book = VecBook::new();

    let _ = book.submit(maker.clone());
    let fills = book.submit(taker);

    assert_eq!(fills, [Fill::Partial { maker, quantity: 2 }]);
}

#[test]
fn partially_fills_a_maker() {
    let maker = SimpleOrder::sell(0, 2, 5);
    let taker = SimpleOrder::buy(1, 1, 5);
    let mut book = Book::new();

    let _ = book.submit(maker);
    let fills = book.submit(taker);

    assert_eq!(fills, [partial(maker, 1)]);
    assert_eq!(book.get(&maker.id()).unwrap().quantity(), 1);
}

#[test]
fn completely_fills_both_orders() {
    let maker = SimpleOrder::sell(0, 2, 5);
    let taker = SimpleOrder::buy(1, 2, 5);
    let mut book = Book::new();

    let _ = book.submit(maker);
    let fills = book.submit(taker);

    assert_eq!(fills, [full(maker)]);
    assert!(book.is_empty());
}

#[test]
fn rests_a_takers_remaining_quantity() {
    let maker = SimpleOrder::sell(0, 2, 5);
    let taker = SimpleOrder::buy(1, 3, 5);
    let mut book = Book::new();

    let _ = book.submit(maker);
    let fills = book.submit(taker);

    assert_eq!(fills, [full(maker)]);
    assert_eq!(book.get(&taker.id()).unwrap().quantity(), 1);
}

#[test]
fn matches_in_price_time_order() {
    let makers = [
        SimpleOrder::sell(0, 1, 5),
        SimpleOrder::sell(1, 2, 5),
        SimpleOrder::sell(2, 1, 6),
    ];
    let taker = SimpleOrder::buy(3, 4, 6);
    let mut book = Book::new();

    for maker in makers {
        let _ = book.submit(maker);
    }
    let fills = book.submit(taker);

    assert_eq!(fills, [full(makers[0]), full(makers[1]), full(makers[2])]);
    assert!(book.is_empty());
}

#[test]
fn preserves_bid_time_priority() {
    let makers = [
        SimpleOrder::buy(0, 1, 5),
        SimpleOrder::buy(1, 1, 5),
        SimpleOrder::buy(2, 1, 5),
    ];
    let taker = SimpleOrder::sell(3, 3, 5);
    let mut book = Book::new();

    for maker in makers {
        let _ = book.submit(maker);
    }
    let fills = book.submit(taker);

    assert_eq!(fills, [full(makers[0]), full(makers[1]), full(makers[2])]);
}

#[test]
fn allows_a_non_crossing_spread() {
    let bid = SimpleOrder::buy(0, 1, 100);
    let ask = SimpleOrder::sell(1, 1, 101);
    let mut book = Book::new();

    assert!(book.submit(bid).is_empty());
    assert!(book.submit(ask).is_empty());

    assert_eq!(book.best_bid(), Some(&bid));
    assert_eq!(book.best_ask(), Some(&ask));
    assert_eq!(book.bids().copied().collect::<Vec<_>>(), [bid]);
    assert_eq!(book.asks().copied().collect::<Vec<_>>(), [ask]);
}

#[test]
fn accepts_zero_quantity_orders() {
    let maker = SimpleOrder::sell(0, 0, 5);
    let taker = SimpleOrder::buy(1, 1, 5);
    let mut book = Book::new();

    assert!(book.submit(maker).is_empty());
    assert_eq!(book.submit(taker), [full(maker)]);
    assert_eq!(book.get(&taker.id()).unwrap().quantity(), 1);
}

#[test]
fn leaves_identifier_uniqueness_to_the_caller() {
    let mut book = Book::new();

    let _ = book.submit(SimpleOrder::buy(7, 1, 4));
    let _ = book.submit(SimpleOrder::buy(7, 1, 5));

    assert_eq!(book.len(), 2);
}

#[test]
fn cancels_an_order() {
    let order = SimpleOrder::buy(1, 1, 2);
    let mut book = Book::new();

    let _ = book.submit(order);

    assert!(book.contains(&order.id()));
    assert_eq!(book.cancel(&order.id()), Some(order));
    assert_eq!(book.cancel(&order.id()), None);
}

#[test]
fn reduces_an_order_without_losing_priority() {
    let first = SimpleOrder::sell(0, 2, 5);
    let second = SimpleOrder::sell(1, 2, 5);
    let taker = SimpleOrder::buy(2, 2, 5);
    let mut book = Book::new();

    let _ = book.submit(first);
    let _ = book.submit(second);
    assert_eq!(book.reduce(&first.id(), 1), Ok(()));
    let fills = book.submit(taker);

    assert_eq!(
        fills,
        [
            full(SimpleOrder::sell(first.id(), 1, first.price())),
            partial(second, 1),
        ]
    );
}

#[test]
fn reports_reduction_errors() {
    let order = SimpleOrder::buy(0, 2, 5);
    let mut book = Book::new();
    let _ = book.submit(order);

    assert_eq!(book.reduce(&99, 1), Err(ReduceError::NotFound));
    assert_eq!(book.reduce(&order.id(), 2), Err(ReduceError::NotReduced));
    assert_eq!(book.reduce(&order.id(), 3), Err(ReduceError::NotReduced));
    assert_eq!(book.reduce(&order.id(), 0), Ok(()));
}

#[test]
fn clears_all_open_orders() {
    let mut book = Book::new();
    let _ = book.submit(SimpleOrder::buy(0, 1, 4));
    let _ = book.submit(SimpleOrder::sell(1, 1, 5));

    book.clear();

    assert!(book.is_empty());
    assert_eq!(book.iter().count(), 0);
}

#[test]
fn levelbook_reuses_arena_high_water_mark() {
    let mut book = LevelBook::new();
    for order_id in 0..10_000 {
        let _ = book.submit(SimpleOrder::buy(order_id, 1, 100));
    }
    let capacity = book.arena_capacities();

    for order_id in 0..10_000 {
        assert!(book.cancel(&order_id).is_some());
    }
    for order_id in 10_000..20_000 {
        let _ = book.submit(SimpleOrder::buy(order_id, 1, 100));
    }

    assert_eq!(book.arena_capacities(), capacity);
    book.assert_invariants();
}

#[test]
fn flatbook_reuses_arena_high_water_mark() {
    let mut book = FlatLevelBook::new();
    for order_id in 0..10_000 {
        let _ = book.submit(SimpleOrder::buy(order_id, 1, 100));
    }
    let capacity = book.arena_capacities();

    for order_id in 0..10_000 {
        assert!(book.cancel(&order_id).is_some());
    }
    for order_id in 10_000..20_000 {
        let _ = book.submit(SimpleOrder::buy(order_id, 1, 100));
    }

    assert_eq!(book.arena_capacities(), capacity);
    book.assert_invariants();
}

mod property_tests {
    use proptest::prelude::*;

    use super::{assert_same_state, Book, FlatLevelBook, LevelBook, OrderBook, SimpleOrder};

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn indexed_books_are_differentially_equivalent_to_vecbook(
            actions in prop::collection::vec(
                (any::<u8>(), any::<bool>(), any::<u16>(), any::<u8>(), any::<u16>()),
                1..500,
            )
        ) {
            let mut vecbook = Book::new();
            let mut levelbook = LevelBook::new();
            let mut flatbook = FlatLevelBook::new();
            let mut next_order_id = 0_u32;

            for (kind, is_buy, raw_price, raw_quantity, selector) in actions {
                let open_ids = vecbook.iter().map(SimpleOrder::id).collect::<Vec<_>>();
                if kind % 4 < 2 || open_ids.is_empty() {
                    let price = 95 + u32::from(raw_price % 11);
                    let quantity = u32::from(raw_quantity % 21);
                    let order = if is_buy {
                        SimpleOrder::buy(next_order_id, quantity, price)
                    } else {
                        SimpleOrder::sell(next_order_id, quantity, price)
                    };
                    next_order_id += 1;
                    let expected = vecbook.submit(order).to_vec();
                    let actual = levelbook.submit(order).to_vec();
                    let flat_actual = flatbook.submit(order).to_vec();
                    prop_assert_eq!(&actual, &expected);
                    prop_assert_eq!(&flat_actual, &expected);
                } else {
                    let order_id = open_ids[usize::from(selector) % open_ids.len()];
                    if kind % 4 == 2 {
                        let expected = vecbook.cancel(&order_id);
                        prop_assert_eq!(levelbook.cancel(&order_id), expected);
                        prop_assert_eq!(flatbook.cancel(&order_id), expected);
                    } else {
                        let current = vecbook.get(&order_id).expect("selected order was open").quantity();
                        if current > 0 {
                            let new_quantity = u32::from(raw_quantity) % current;
                            let expected = vecbook.reduce(&order_id, new_quantity);
                            prop_assert_eq!(levelbook.reduce(&order_id, new_quantity), expected);
                            prop_assert_eq!(flatbook.reduce(&order_id, new_quantity), expected);
                        }
                    }
                }

                assert_same_state(&vecbook, &levelbook);
                assert_same_state(&vecbook, &flatbook);
                levelbook.assert_invariants();
                flatbook.assert_invariants();
            }
        }
    }
}

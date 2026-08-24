mod conformance;

use crate::{FlatLevelBook, LevelBook, OrderBook, SimpleOrder, VecBook};

type ReferenceBook = VecBook<SimpleOrder>;

fn assert_same_state<IndexedBook>(reference: &ReferenceBook, indexed: &IndexedBook)
where
    IndexedBook: OrderBook<Order = SimpleOrder>,
{
    assert_eq!(indexed.len(), reference.len());
    assert_eq!(
        indexed.bids().copied().collect::<Vec<_>>(),
        reference.bids().copied().collect::<Vec<_>>()
    );
    assert_eq!(
        indexed.asks().copied().collect::<Vec<_>>(),
        reference.asks().copied().collect::<Vec<_>>()
    );
}

fn submit_both(
    reference: &mut ReferenceBook,
    levelbook: &mut LevelBook<SimpleOrder>,
    order: SimpleOrder,
) {
    let expected = reference.submit(order).to_vec();
    let actual = levelbook.submit(order).to_vec();
    assert_eq!(actual, expected);
    assert_same_state(reference, levelbook);
    levelbook.assert_invariants();
}

#[test]
fn levelbook_matches_vecbook_across_mixed_operations() {
    let mut reference = ReferenceBook::new();
    let mut levelbook = LevelBook::new();

    for order in [
        SimpleOrder::buy(0, 4, 99),
        SimpleOrder::buy(1, 3, 100),
        SimpleOrder::buy(2, 2, 100),
        SimpleOrder::sell(3, 2, 101),
        SimpleOrder::sell(4, 5, 101),
        SimpleOrder::sell(5, 3, 102),
    ] {
        submit_both(&mut reference, &mut levelbook, order);
    }

    assert_eq!(levelbook.cancel(&4), reference.cancel(&4));
    assert_eq!(levelbook.reduce(&1, 2), reference.reduce(&1, 2));
    assert_same_state(&reference, &levelbook);
    levelbook.assert_invariants();

    submit_both(&mut reference, &mut levelbook, SimpleOrder::buy(6, 4, 102));
    submit_both(&mut reference, &mut levelbook, SimpleOrder::sell(7, 6, 99));
    submit_both(&mut reference, &mut levelbook, SimpleOrder::buy(8, 10, 103));
}

#[test]
fn vecbook_leaves_identifier_uniqueness_to_the_caller() {
    let mut book = ReferenceBook::new();

    let _ = book.submit(SimpleOrder::buy(7, 1, 4));
    let _ = book.submit(SimpleOrder::buy(7, 1, 5));

    assert_eq!(book.len(), 2);
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

    use super::{
        assert_same_state, FlatLevelBook, LevelBook, OrderBook, ReferenceBook, SimpleOrder,
    };

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn indexed_books_are_differentially_equivalent_to_vecbook(
            actions in prop::collection::vec(
                (any::<u8>(), any::<bool>(), any::<u16>(), any::<u8>(), any::<u16>()),
                1..500,
            )
        ) {
            let mut reference = ReferenceBook::new();
            let mut levelbook = LevelBook::new();
            let mut flatbook = FlatLevelBook::new();
            let mut next_order_id = 0_u32;

            for (kind, is_buy, raw_price, raw_quantity, selector) in actions {
                let open_ids = reference.iter().map(SimpleOrder::id).collect::<Vec<_>>();
                if kind % 4 < 2 || open_ids.is_empty() {
                    let price = 95 + u32::from(raw_price % 11);
                    let quantity = u32::from(raw_quantity % 21);
                    let order = if is_buy {
                        SimpleOrder::buy(next_order_id, quantity, price)
                    } else {
                        SimpleOrder::sell(next_order_id, quantity, price)
                    };
                    next_order_id += 1;
                    let expected = reference.submit(order).to_vec();
                    let actual = levelbook.submit(order).to_vec();
                    let flat_actual = flatbook.submit(order).to_vec();
                    prop_assert_eq!(&actual, &expected);
                    prop_assert_eq!(&flat_actual, &expected);
                } else {
                    let order_id = open_ids[usize::from(selector) % open_ids.len()];
                    if kind % 4 == 2 {
                        let expected = reference.cancel(&order_id);
                        prop_assert_eq!(levelbook.cancel(&order_id), expected);
                        prop_assert_eq!(flatbook.cancel(&order_id), expected);
                    } else {
                        let current = reference
                            .get(&order_id)
                            .expect("selected order was open")
                            .quantity();
                        if current > 0 {
                            let new_quantity = u32::from(raw_quantity) % current;
                            let expected = reference.reduce(&order_id, new_quantity);
                            prop_assert_eq!(levelbook.reduce(&order_id, new_quantity), expected);
                            prop_assert_eq!(flatbook.reduce(&order_id, new_quantity), expected);
                        }
                    }
                }

                assert_same_state(&reference, &levelbook);
                assert_same_state(&reference, &flatbook);
                levelbook.assert_invariants();
                flatbook.assert_invariants();
            }
        }
    }
}

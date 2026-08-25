use core::ops::Sub;

use crate::{Fill, FlatLevelBook, LevelBook, Order, OrderBook, ReduceError, SimpleOrder, VecBook};

fn full(maker: SimpleOrder) -> Fill<SimpleOrder> {
    Fill::Full(maker)
}

fn partial(mut maker: SimpleOrder, quantity: u32) -> Fill<SimpleOrder> {
    maker.set_quantity(quantity);
    Fill::Partial(maker)
}

fn default_is_empty<Book>()
where
    Book: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = Book::default();

    assert_eq!(book.len(), 0);
    assert!(book.is_empty());
    assert_eq!(book.iter().count(), 0);
    assert_eq!(book.bids().count(), 0);
    assert_eq!(book.asks().count(), 0);
    assert_eq!(book.best_bid(), None);
    assert_eq!(book.best_ask(), None);
    assert_eq!(book.get(&7), None);
    assert!(!book.contains(&7));
    assert_eq!(book.cancel(&7), None);
    assert_eq!(book.reduce(&7, 1), Err(ReduceError::NotFound));
}

fn rests_and_iterates_in_price_time_order<Book>()
where
    Book: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = Book::default();
    let bid_low = SimpleOrder::buy(0, 1, 99);
    let bid_best_first = SimpleOrder::buy(1, 1, 100);
    let bid_best_second = SimpleOrder::buy(2, 1, 100);
    let ask_high = SimpleOrder::sell(3, 1, 102);
    let ask_best_first = SimpleOrder::sell(4, 1, 101);
    let ask_best_second = SimpleOrder::sell(5, 1, 101);

    for order in [
        bid_low,
        ask_high,
        bid_best_first,
        ask_best_first,
        bid_best_second,
        ask_best_second,
    ] {
        assert!(book.submit(order).is_empty());
    }

    assert_eq!(book.len(), 6);
    assert!(!book.is_empty());
    assert_eq!(book.best_bid(), Some(&bid_best_first));
    assert_eq!(book.best_ask(), Some(&ask_best_first));
    assert_eq!(
        book.bids().copied().collect::<Vec<_>>(),
        [bid_best_first, bid_best_second, bid_low]
    );
    assert_eq!(
        book.asks().copied().collect::<Vec<_>>(),
        [ask_best_first, ask_best_second, ask_high]
    );
    assert_eq!(
        book.iter().copied().collect::<Vec<_>>(),
        [
            bid_best_first,
            bid_best_second,
            bid_low,
            ask_best_first,
            ask_best_second,
            ask_high,
        ]
    );

    for order in [bid_low, bid_best_first, bid_best_second, ask_high] {
        assert!(book.contains(&order.id()));
        assert_eq!(book.get(&order.id()), Some(&order));
    }
}

fn respects_crossing_boundaries<Book>()
where
    Book: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = Book::default();
    let bid = SimpleOrder::buy(0, 1, 100);
    let ask = SimpleOrder::sell(1, 1, 101);

    assert!(book.submit(bid).is_empty());
    assert!(book.submit(ask).is_empty());
    assert_eq!(book.best_bid(), Some(&bid));
    assert_eq!(book.best_ask(), Some(&ask));

    assert_eq!(book.submit(SimpleOrder::buy(2, 1, 101)), [full(ask)]);
    assert_eq!(book.submit(SimpleOrder::sell(3, 1, 100)), [full(bid)]);
    assert!(book.is_empty());
}

fn partially_fills_makers_on_both_sides<Book>()
where
    Book: OrderBook<Order = SimpleOrder> + Default,
{
    let mut ask_book = Book::default();
    let ask = SimpleOrder::sell(0, 5, 100);
    let _ = ask_book.submit(ask);
    assert_eq!(
        ask_book.submit(SimpleOrder::buy(1, 2, 100)),
        [partial(ask, 2)]
    );
    assert_eq!(ask_book.get(&ask.id()).unwrap().quantity(), 3);

    let mut bid_book = Book::default();
    let bid = SimpleOrder::buy(2, 5, 100);
    let _ = bid_book.submit(bid);
    assert_eq!(
        bid_book.submit(SimpleOrder::sell(3, 2, 100)),
        [partial(bid, 2)]
    );
    assert_eq!(bid_book.get(&bid.id()).unwrap().quantity(), 3);
}

fn matches_fifo_on_both_sides<Book>()
where
    Book: OrderBook<Order = SimpleOrder> + Default,
{
    let mut ask_book = Book::default();
    let asks = [
        SimpleOrder::sell(0, 1, 100),
        SimpleOrder::sell(1, 1, 100),
        SimpleOrder::sell(2, 1, 100),
    ];
    for maker in asks {
        let _ = ask_book.submit(maker);
    }
    assert_eq!(ask_book.submit(SimpleOrder::buy(3, 3, 100)), asks.map(full));

    let mut bid_book = Book::default();
    let bids = [
        SimpleOrder::buy(4, 1, 100),
        SimpleOrder::buy(5, 1, 100),
        SimpleOrder::buy(6, 1, 100),
    ];
    for maker in bids {
        let _ = bid_book.submit(maker);
    }
    assert_eq!(
        bid_book.submit(SimpleOrder::sell(7, 3, 100)),
        bids.map(full)
    );
}

fn sweeps_levels_and_rests_the_remainder<Book>()
where
    Book: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = Book::default();
    let makers = [
        SimpleOrder::sell(0, 2, 100),
        SimpleOrder::sell(1, 2, 100),
        SimpleOrder::sell(2, 3, 101),
        SimpleOrder::sell(3, 1, 102),
    ];
    for maker in makers {
        let _ = book.submit(maker);
    }

    let taker = SimpleOrder::buy(4, 8, 101);
    assert_eq!(
        book.submit(taker),
        [full(makers[0]), full(makers[1]), full(makers[2])]
    );
    assert_eq!(book.get(&taker.id()).unwrap().quantity(), 1);
    assert_eq!(book.best_bid().unwrap().id(), taker.id());
    assert_eq!(book.best_ask(), Some(&makers[3]));
}

fn stops_a_sweep_at_the_limit_price<Book>()
where
    Book: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = Book::default();
    let makers = [
        SimpleOrder::sell(0, 1, 100),
        SimpleOrder::sell(1, 1, 101),
        SimpleOrder::sell(2, 1, 102),
    ];
    for maker in makers {
        let _ = book.submit(maker);
    }

    let taker = SimpleOrder::buy(3, 5, 101);
    assert_eq!(book.submit(taker), [full(makers[0]), full(makers[1])]);
    assert_eq!(book.get(&taker.id()).unwrap().quantity(), 3);
    assert_eq!(book.best_ask(), Some(&makers[2]));
}

fn cancels_head_middle_tail_and_reuses_levels<Book>()
where
    Book: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = Book::default();
    let orders = [
        SimpleOrder::buy(0, 1, 100),
        SimpleOrder::buy(1, 1, 100),
        SimpleOrder::buy(2, 1, 100),
        SimpleOrder::buy(3, 1, 99),
    ];
    for order in orders {
        let _ = book.submit(order);
    }

    assert_eq!(book.cancel(&1), Some(orders[1]));
    assert_eq!(book.cancel(&0), Some(orders[0]));
    assert_eq!(book.cancel(&2), Some(orders[2]));
    assert_eq!(book.cancel(&2), None);
    assert!(!book.contains(&2));
    assert_eq!(book.best_bid(), Some(&orders[3]));

    let replacement = SimpleOrder::buy(4, 1, 100);
    let _ = book.submit(replacement);
    assert_eq!(book.best_bid(), Some(&replacement));
    assert_eq!(book.get(&2), None);

    let tail = SimpleOrder::buy(5, 1, 100);
    let _ = book.submit(tail);
    assert_eq!(book.cancel(&5), Some(tail));
    assert_eq!(
        book.bids().copied().collect::<Vec<_>>(),
        [replacement, orders[3]]
    );
}

fn reduces_without_losing_priority_and_reports_errors<Book>()
where
    Book: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = Book::default();
    let first = SimpleOrder::sell(0, 3, 100);
    let second = SimpleOrder::sell(1, 3, 100);
    let _ = book.submit(first);
    let _ = book.submit(second);

    assert_eq!(book.reduce(&99, 1), Err(ReduceError::NotFound));
    assert_eq!(book.reduce(&0, 3), Err(ReduceError::NotReduced));
    assert_eq!(book.reduce(&0, 4), Err(ReduceError::NotReduced));
    assert_eq!(book.reduce(&0, 1), Ok(()));
    assert_eq!(
        book.submit(SimpleOrder::buy(2, 2, 100)),
        [full(first.with_quantity(1)), partial(second, 1)]
    );
}

fn clears_orders_and_fill_state<Book>()
where
    Book: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = Book::default();
    let maker = SimpleOrder::sell(0, 1, 100);
    assert!(book.submit(maker).is_empty());
    assert_eq!(book.submit(SimpleOrder::buy(1, 1, 100)), [full(maker)]);

    assert!(book.submit(SimpleOrder::buy(2, 1, 99)).is_empty());
    let _ = book.submit(SimpleOrder::sell(3, 1, 101));
    assert_eq!(book.len(), 2);
    book.clear();

    assert!(book.is_empty());
    assert_eq!(book.iter().count(), 0);
    assert_eq!(book.best_bid(), None);
    assert_eq!(book.best_ask(), None);
    assert!(book.submit(SimpleOrder::buy(4, 1, 98)).is_empty());
}

fn follows_the_zero_quantity_policy<Book>()
where
    Book: OrderBook<Order = SimpleOrder> + Default,
{
    let mut book = Book::default();
    let zero = SimpleOrder::sell(0, 0, 100);
    assert!(book.submit(zero).is_empty());
    assert_eq!(book.submit(SimpleOrder::buy(1, 1, 100)), [full(zero)]);
    assert_eq!(book.get(&1).unwrap().quantity(), 1);
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ApplicationQuantity(u32);

impl Sub for ApplicationQuantity {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ApplicationPrice(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApplicationSide {
    Bid,
    Offer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApplicationOrder {
    id: String,
    quantity: ApplicationQuantity,
    price: ApplicationPrice,
    side: ApplicationSide,
    account: String,
    metadata: Vec<String>,
}

impl Order for ApplicationOrder {
    type OrderId = String;
    type Quantity = ApplicationQuantity;
    type Price = ApplicationPrice;

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

fn preserves_application_owned_types_and_metadata<Book>()
where
    Book: OrderBook<Order = ApplicationOrder> + Default,
{
    let maker = ApplicationOrder {
        id: "maker".into(),
        quantity: ApplicationQuantity(5),
        price: ApplicationPrice(100),
        side: ApplicationSide::Offer,
        account: "account-a".into(),
        metadata: vec!["maker-data".into()],
    };
    let taker = ApplicationOrder {
        id: "taker".into(),
        quantity: ApplicationQuantity(2),
        price: ApplicationPrice(100),
        side: ApplicationSide::Bid,
        account: "account-b".into(),
        metadata: vec!["taker-data".into()],
    };
    let mut book = Book::default();

    let _ = book.submit(maker.clone());
    let mut executed_maker = maker.clone();
    executed_maker.set_quantity(ApplicationQuantity(2));
    assert_eq!(book.submit(taker), [Fill::Partial(executed_maker)]);
    assert_eq!(
        book.get(&maker.id).unwrap().quantity,
        ApplicationQuantity(3)
    );
    assert_eq!(book.get(&maker.id).unwrap().account, "account-a");
    assert_eq!(book.get(&maker.id).unwrap().metadata, ["maker-data"]);
}

macro_rules! order_book_conformance_suite {
    ($module:ident, $book:ident) => {
        mod $module {
            use super::*;

            type Book = $book<SimpleOrder>;
            type ApplicationBook = $book<ApplicationOrder>;

            #[test]
            fn default_is_empty() {
                super::default_is_empty::<Book>();
            }

            #[test]
            fn rests_and_iterates_in_price_time_order() {
                super::rests_and_iterates_in_price_time_order::<Book>();
            }

            #[test]
            fn respects_crossing_boundaries() {
                super::respects_crossing_boundaries::<Book>();
            }

            #[test]
            fn partially_fills_makers_on_both_sides() {
                super::partially_fills_makers_on_both_sides::<Book>();
            }

            #[test]
            fn matches_fifo_on_both_sides() {
                super::matches_fifo_on_both_sides::<Book>();
            }

            #[test]
            fn sweeps_levels_and_rests_the_remainder() {
                super::sweeps_levels_and_rests_the_remainder::<Book>();
            }

            #[test]
            fn stops_a_sweep_at_the_limit_price() {
                super::stops_a_sweep_at_the_limit_price::<Book>();
            }

            #[test]
            fn cancels_head_middle_tail_and_reuses_levels() {
                super::cancels_head_middle_tail_and_reuses_levels::<Book>();
            }

            #[test]
            fn reduces_without_losing_priority_and_reports_errors() {
                super::reduces_without_losing_priority_and_reports_errors::<Book>();
            }

            #[test]
            fn clears_orders_and_fill_state() {
                super::clears_orders_and_fill_state::<Book>();
            }

            #[test]
            fn follows_the_zero_quantity_policy() {
                super::follows_the_zero_quantity_policy::<Book>();
            }

            #[test]
            fn preserves_application_owned_types_and_metadata() {
                super::preserves_application_owned_types_and_metadata::<ApplicationBook>();
            }
        }
    };
}

order_book_conformance_suite!(vecbook, VecBook);
order_book_conformance_suite!(levelbook, LevelBook);
order_book_conformance_suite!(flatbook, FlatLevelBook);

//! A flat price-level order book backed by indexed arenas.

use core::fmt;
use core::ops::Sub;
use rustc_hash::FxHashMap;

use crate::arena::{Arena, Key};
use crate::{Fill, Order, OrderBook, ReduceError};

#[derive(Clone, Copy, Debug)]
enum OrderTag {}

#[derive(Clone, Copy, Debug)]
enum LevelTag {}

type OrderKey = Key<OrderTag>;
type LevelKey = Key<LevelTag>;

#[derive(Clone, Debug)]
struct OrderNode<OrderType> {
    order: OrderType,
    level: LevelKey,
    previous: Option<OrderKey>,
    next: Option<OrderKey>,
}

#[derive(Clone, Debug, Default)]
struct LevelNode {
    head: Option<OrderKey>,
    tail: Option<OrderKey>,
}

#[derive(Clone, Debug)]
struct FlatLevels<Price> {
    // Both sides are stored worst-to-best, making the best level the final entry. Bids therefore
    // use ascending prices and asks use descending prices.
    entries: Vec<(Price, LevelKey)>,
}

impl<Price> Default for FlatLevels<Price> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<Price: Ord> FlatLevels<Price> {
    fn search(&self, price: &Price, is_buy: bool) -> Result<usize, usize> {
        if is_buy {
            self.entries
                .binary_search_by(|(candidate, _)| candidate.cmp(price))
        } else {
            self.entries
                .binary_search_by(|(candidate, _)| price.cmp(candidate))
        }
    }

    fn get(&self, price: &Price, is_buy: bool) -> Option<LevelKey> {
        self.search(price, is_buy)
            .ok()
            .map(|index| self.entries[index].1)
    }

    fn insert(&mut self, price: Price, level: LevelKey, is_buy: bool) {
        let index = self
            .search(&price, is_buy)
            .expect_err("price level was already indexed");
        self.entries.insert(index, (price, level));
    }

    fn remove(&mut self, price: &Price, is_buy: bool) -> Option<LevelKey> {
        let index = self.search(price, is_buy).ok()?;
        Some(self.entries.remove(index).1)
    }

    fn best(&self) -> Option<LevelKey> {
        self.entries.last().map(|(_, level)| *level)
    }

    fn best_first(&self) -> impl Iterator<Item = (&Price, LevelKey)> {
        self.entries
            .iter()
            .rev()
            .map(|(price, level)| (price, *level))
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

struct LevelOrders<'book, OrderType: Order> {
    arena: &'book Arena<OrderNode<OrderType>, OrderTag>,
    next: Option<OrderKey>,
}

impl<'book, OrderType: Order> Iterator for LevelOrders<'book, OrderType> {
    type Item = &'book OrderType;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.arena.get(self.next?)?;
        self.next = node.next;
        Some(&node.order)
    }
}

/// A price-time-priority order book organized into indexed price levels.
///
/// Prices are held in sorted contiguous vectors, with the best price at the end of each side. Each
/// level is a doubly linked list of arena-backed orders, and an order identifier hash index provides
/// direct access for cancellation, reduction, and lookup. This implementation is intended for books
/// with relatively few active price levels. It requires cloneable identifiers and prices because it
/// owns keys in those indexes. The identifier index uses a fast, non-cryptographic hasher intended
/// for trusted exchange input.
#[derive(Clone)]
pub struct FlatLevelBook<OrderType: Order> {
    orders: Arena<OrderNode<OrderType>, OrderTag>,
    levels: Arena<LevelNode, LevelTag>,
    bids: FlatLevels<OrderType::Price>,
    asks: FlatLevels<OrderType::Price>,
    by_id: FxHashMap<OrderType::OrderId, OrderKey>,
    fills: Vec<Fill<OrderType>>,
}

impl<OrderType: Order> FlatLevelBook<OrderType> {
    fn level_orders(&self, level: LevelKey) -> LevelOrders<'_, OrderType> {
        LevelOrders {
            arena: &self.orders,
            next: self.levels.get(level).and_then(|node| node.head),
        }
    }

    #[cfg(test)]
    pub(crate) fn assert_invariants(&self) {
        use std::collections::HashSet;

        assert_eq!(self.levels.len(), self.bids.len() + self.asks.len());
        assert_eq!(self.orders.len(), self.by_id.len());

        let mut reachable = HashSet::with_capacity(self.orders.len());
        for (is_buy, levels) in [(true, &self.bids), (false, &self.asks)] {
            for (price, level_key) in levels.best_first() {
                let level = self.levels.get(level_key).expect("level key was invalid");
                assert!(level.head.is_some());
                assert!(level.tail.is_some());

                let mut previous = None;
                let mut current = level.head;
                while let Some(order_key) = current {
                    assert!(reachable.insert(order_key), "order was linked twice");
                    let node = self.orders.get(order_key).expect("order link was invalid");
                    assert_eq!(node.level, level_key);
                    assert_eq!(node.previous, previous);
                    assert_eq!(node.order.is_buy(), is_buy);
                    assert!(node.order.price() == price);
                    previous = current;
                    current = node.next;
                }
                assert_eq!(previous, level.tail);
            }
        }

        assert_eq!(reachable.len(), self.orders.len());
        for (order_id, order_key) in &self.by_id {
            assert!(reachable.contains(order_key));
            let node = self
                .orders
                .get(*order_key)
                .expect("order index was invalid");
            assert!(node.order.id() == order_id);
        }

        if let (Some(bid_level), Some(ask_level)) = (self.bids.best(), self.asks.best()) {
            let bid_key = self.levels.get(bid_level).and_then(|level| level.head);
            let ask_key = self.levels.get(ask_level).and_then(|level| level.head);
            let bid = self.orders.get(bid_key.expect("best bid was missing"));
            let ask = self.orders.get(ask_key.expect("best ask was missing"));
            assert!(
                bid.expect("best bid was invalid").order.price()
                    < ask.expect("best ask was invalid").order.price(),
                "resting book was crossed",
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn arena_capacities(&self) -> (usize, usize) {
        (self.orders.capacity(), self.levels.capacity())
    }
}

impl<OrderType: Order> Default for FlatLevelBook<OrderType> {
    fn default() -> Self {
        Self {
            orders: Arena::new(),
            levels: Arena::new(),
            bids: FlatLevels::default(),
            asks: FlatLevels::default(),
            by_id: FxHashMap::default(),
            fills: Vec::new(),
        }
    }
}

impl<OrderType> fmt::Debug for FlatLevelBook<OrderType>
where
    OrderType: Order + fmt::Debug,
    OrderType::OrderId: Clone,
    OrderType::Price: Clone,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FlatLevelBook")
            .field("bids", &self.bids().collect::<Vec<_>>())
            .field("asks", &self.asks().collect::<Vec<_>>())
            .finish()
    }
}

impl<OrderType> OrderBook for FlatLevelBook<OrderType>
where
    OrderType: Order,
    OrderType::OrderId: Clone,
    OrderType::Price: Clone,
{
    type Order = OrderType;

    fn len(&self) -> usize {
        self.orders.len()
    }

    fn clear(&mut self) {
        self.orders.clear();
        self.levels.clear();
        self.bids.clear();
        self.asks.clear();
        self.by_id.clear();
        self.fills.clear();
    }

    fn get(&self, order_id: &OrderType::OrderId) -> Option<&OrderType> {
        self.by_id
            .get(order_id)
            .and_then(|key| self.orders.get(*key))
            .map(|node| &node.order)
    }

    fn contains(&self, order_id: &OrderType::OrderId) -> bool {
        self.by_id.contains_key(order_id)
    }

    fn bids(&self) -> impl Iterator<Item = &OrderType> {
        self.bids
            .best_first()
            .flat_map(|(_, level)| self.level_orders(level))
    }

    fn asks(&self) -> impl Iterator<Item = &OrderType> {
        self.asks
            .best_first()
            .flat_map(|(_, level)| self.level_orders(level))
    }

    fn submit(&mut self, mut taker: OrderType) -> &[Fill<OrderType>] {
        self.fills.clear();

        while let Some(maker_key) = self.best_maker(taker.is_buy()) {
            let maker = &self
                .orders
                .get(maker_key)
                .expect("best level contained an invalid order")
                .order;
            if !Self::crosses(&taker, maker) {
                break;
            }

            let taker_quantity = taker.quantity().clone();
            let maker_quantity = maker.quantity().clone();
            match taker_quantity.cmp(&maker_quantity) {
                core::cmp::Ordering::Equal => {
                    let maker = self
                        .remove_order(maker_key)
                        .expect("best maker was present");
                    self.fills.push(Fill::Full(maker));
                    return &self.fills;
                }
                core::cmp::Ordering::Greater => {
                    let maker = self
                        .remove_order(maker_key)
                        .expect("best maker was present");
                    Self::subtract_quantity(&mut taker, maker_quantity);
                    self.fills.push(Fill::Full(maker));
                }
                core::cmp::Ordering::Less => {
                    let maker = self
                        .orders
                        .get_mut(maker_key)
                        .expect("best maker was present");
                    let maker_snapshot = maker.order.clone();
                    Self::subtract_quantity(&mut maker.order, taker_quantity.clone());
                    self.fills.push(Fill::Partial {
                        maker: maker_snapshot,
                        quantity: taker_quantity,
                    });
                    return &self.fills;
                }
            }
        }

        self.insert_resting(taker);
        &self.fills
    }

    fn cancel(&mut self, order_id: &OrderType::OrderId) -> Option<OrderType> {
        let key = self.by_id.remove(order_id)?;
        self.unlink_order(key)
    }

    fn reduce(
        &mut self,
        order_id: &OrderType::OrderId,
        new_quantity: OrderType::Quantity,
    ) -> Result<(), ReduceError> {
        let key = *self.by_id.get(order_id).ok_or(ReduceError::NotFound)?;
        let order = &mut self
            .orders
            .get_mut(key)
            .expect("order index contained an invalid key")
            .order;
        if order.quantity() <= &new_quantity {
            return Err(ReduceError::NotReduced);
        }
        order.set_quantity(new_quantity);
        Ok(())
    }
}

impl<OrderType> FlatLevelBook<OrderType>
where
    OrderType: Order,
    OrderType::OrderId: Clone,
    OrderType::Price: Clone,
{
    fn best_maker(&self, taker_is_buy: bool) -> Option<OrderKey> {
        let level_key = if taker_is_buy {
            self.asks.best()?
        } else {
            self.bids.best()?
        };
        self.levels.get(level_key)?.head
    }

    fn crosses(taker: &OrderType, maker: &OrderType) -> bool {
        if taker.is_buy() {
            taker.price() >= maker.price()
        } else {
            taker.price() <= maker.price()
        }
    }

    fn insert_resting(&mut self, order: OrderType) {
        let order_id = order.id().clone();
        let level_key = if order.is_buy() {
            if let Some(level) = self.bids.get(order.price(), true) {
                level
            } else {
                let level = self.levels.insert(LevelNode::default());
                self.bids.insert(order.price().clone(), level, true);
                level
            }
        } else if let Some(level) = self.asks.get(order.price(), false) {
            level
        } else {
            let level = self.levels.insert(LevelNode::default());
            self.asks.insert(order.price().clone(), level, false);
            level
        };

        let previous = self
            .levels
            .get(level_key)
            .expect("price index contained an invalid level")
            .tail;
        let order_key = self.orders.insert(OrderNode {
            order,
            level: level_key,
            previous,
            next: None,
        });

        if let Some(previous) = previous {
            self.orders
                .get_mut(previous)
                .expect("level tail was invalid")
                .next = Some(order_key);
        }
        let level = self
            .levels
            .get_mut(level_key)
            .expect("price index contained an invalid level");
        if level.head.is_none() {
            level.head = Some(order_key);
        }
        level.tail = Some(order_key);

        let replaced = self.by_id.insert(order_id, order_key);
        debug_assert!(replaced.is_none(), "duplicate order id");
    }

    fn remove_order(&mut self, key: OrderKey) -> Option<OrderType> {
        let order = self.unlink_order(key)?;
        let indexed = self.by_id.remove(order.id());
        debug_assert_eq!(indexed, Some(key), "maker was missing from order index");
        Some(order)
    }

    fn unlink_order(&mut self, key: OrderKey) -> Option<OrderType> {
        let node = self.orders.get(key)?;
        let level_key = node.level;
        let previous = node.previous;
        let next = node.next;

        if let Some(previous) = previous {
            self.orders
                .get_mut(previous)
                .expect("previous order link was invalid")
                .next = next;
        }
        if let Some(next) = next {
            self.orders
                .get_mut(next)
                .expect("next order link was invalid")
                .previous = previous;
        }

        let level = self
            .levels
            .get_mut(level_key)
            .expect("order contained an invalid level");
        if level.head == Some(key) {
            level.head = next;
        }
        if level.tail == Some(key) {
            level.tail = previous;
        }
        let remove_level = level.head.is_none();

        let node = self
            .orders
            .remove(key)
            .expect("validated order was missing from arena");

        if remove_level {
            if node.order.is_buy() {
                self.bids.remove(node.order.price(), true);
            } else {
                self.asks.remove(node.order.price(), false);
            }
            self.levels
                .remove(level_key)
                .expect("empty price level was missing from arena");
        }

        Some(node.order)
    }

    fn subtract_quantity(order: &mut OrderType, quantity: OrderType::Quantity) {
        let remaining = order.quantity().clone().sub(quantity);
        order.set_quantity(remaining);
    }
}

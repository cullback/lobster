//! A price-level order book backed by indexed arenas.

use core::fmt;
use core::ops::Sub;
use std::collections::BTreeMap;

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
    orders: usize,
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
/// Each price level is a doubly linked list of arena-backed orders. An order identifier hash index
/// provides direct access for cancellation, reduction, and lookup. This implementation requires
/// cloneable identifiers and prices because it owns keys in those indexes. The identifier index
/// uses a fast, non-cryptographic hasher intended for trusted exchange input.
pub struct LevelBook<OrderType: Order> {
    orders: Arena<OrderNode<OrderType>, OrderTag>,
    levels: Arena<LevelNode, LevelTag>,
    bids: BTreeMap<OrderType::Price, LevelKey>,
    asks: BTreeMap<OrderType::Price, LevelKey>,
    by_id: FxHashMap<OrderType::OrderId, OrderKey>,
    fills: Vec<Fill<OrderType>>,
}

impl<OrderType: Order> LevelBook<OrderType> {
    /// Creates an empty order book.
    #[must_use]
    pub fn new() -> Self {
        Self {
            orders: Arena::new(),
            levels: Arena::new(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            by_id: FxHashMap::default(),
            fills: Vec::new(),
        }
    }

    fn level_orders(&self, level: LevelKey) -> LevelOrders<'_, OrderType> {
        LevelOrders {
            arena: &self.orders,
            next: self.levels.get(level).and_then(|node| node.head),
        }
    }
}

impl<OrderType: Order> Default for LevelBook<OrderType> {
    fn default() -> Self {
        Self::new()
    }
}

impl<OrderType> Clone for LevelBook<OrderType>
where
    OrderType: Order,
    OrderType::OrderId: Clone,
    OrderType::Price: Clone,
{
    fn clone(&self) -> Self {
        Self {
            orders: self.orders.clone(),
            levels: self.levels.clone(),
            bids: self.bids.clone(),
            asks: self.asks.clone(),
            by_id: self.by_id.clone(),
            fills: self.fills.clone(),
        }
    }
}

impl<OrderType> fmt::Debug for LevelBook<OrderType>
where
    OrderType: Order + fmt::Debug,
    OrderType::OrderId: Clone,
    OrderType::Price: Clone,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LevelBook")
            .field("bids", &self.bids().collect::<Vec<_>>())
            .field("asks", &self.asks().collect::<Vec<_>>())
            .finish()
    }
}

impl<OrderType> OrderBook for LevelBook<OrderType>
where
    OrderType: Order,
    OrderType::OrderId: Clone,
    OrderType::Price: Clone,
{
    type Order = OrderType;

    fn len(&self) -> usize {
        self.orders.len()
    }

    fn is_empty(&self) -> bool {
        self.orders.is_empty()
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
            .values()
            .rev()
            .flat_map(|level| self.level_orders(*level))
    }

    fn asks(&self) -> impl Iterator<Item = &OrderType> {
        self.asks
            .values()
            .flat_map(|level| self.level_orders(*level))
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
        let key = *self.by_id.get(order_id)?;
        self.remove_order(key)
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

impl<OrderType> LevelBook<OrderType>
where
    OrderType: Order,
    OrderType::OrderId: Clone,
    OrderType::Price: Clone,
{
    fn best_maker(&self, taker_is_buy: bool) -> Option<OrderKey> {
        let level_key = if taker_is_buy {
            *self.asks.first_key_value()?.1
        } else {
            *self.bids.last_key_value()?.1
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
            if let Some(level) = self.bids.get(order.price()) {
                *level
            } else {
                let level = self.levels.insert(LevelNode::default());
                self.bids.insert(order.price().clone(), level);
                level
            }
        } else if let Some(level) = self.asks.get(order.price()) {
            *level
        } else {
            let level = self.levels.insert(LevelNode::default());
            self.asks.insert(order.price().clone(), level);
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
        level.orders += 1;

        let replaced = self.by_id.insert(order_id, order_key);
        debug_assert!(replaced.is_none(), "duplicate order id");
    }

    fn remove_order(&mut self, key: OrderKey) -> Option<OrderType> {
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
        level.orders -= 1;
        let remove_level = level.orders == 0;

        let node = self
            .orders
            .remove(key)
            .expect("validated order was missing from arena");
        self.by_id.remove(node.order.id());

        if remove_level {
            if node.order.is_buy() {
                self.bids.remove(node.order.price());
            } else {
                self.asks.remove(node.order.price());
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

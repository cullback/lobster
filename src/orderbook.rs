//! The order book interface.

use crate::{Fill, Order, ReduceError};

/// A price-time-priority limit order book.
pub trait OrderBook {
    /// The order type stored by the book.
    type Order: Order;

    /// Returns the number of open orders in the book.
    #[must_use]
    fn len(&self) -> usize;

    /// Returns whether the book contains no open orders.
    #[must_use]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes all open orders.
    fn clear(&mut self);

    /// Returns all open orders, with bids followed by asks.
    fn iter(&self) -> impl Iterator<Item = &Self::Order> {
        self.bids().chain(self.asks())
    }

    /// Returns an open order by identifier.
    fn get(&self, order_id: &<Self::Order as Order>::OrderId) -> Option<&Self::Order> {
        self.iter().find(|order| order.id() == order_id)
    }

    /// Returns whether an open order has the supplied identifier.
    fn contains(&self, order_id: &<Self::Order as Order>::OrderId) -> bool {
        self.get(order_id).is_some()
    }

    /// Returns bids from best to worst, preserving time priority at each price.
    fn bids(&self) -> impl Iterator<Item = &Self::Order>;

    /// Returns asks from best to worst, preserving time priority at each price.
    fn asks(&self) -> impl Iterator<Item = &Self::Order>;

    /// Returns the bid with the highest price.
    #[must_use]
    fn best_bid(&self) -> Option<&Self::Order> {
        self.bids().next()
    }

    /// Returns the ask with the lowest price.
    #[must_use]
    fn best_ask(&self) -> Option<&Self::Order> {
        self.asks().next()
    }

    /// Submits an order and returns the fills generated while matching it.
    ///
    /// The returned slice is owned by the book and remains valid until the next mutable operation.
    /// The caller must supply an identifier not currently open in the book and a non-negative
    /// quantity. Zero quantity is valid. These requirements are not validated by the book.
    fn submit(&mut self, order: Self::Order) -> &[Fill<Self::Order>];

    /// Cancels and returns an open order by identifier.
    fn cancel(&mut self, order_id: &<Self::Order as Order>::OrderId) -> Option<Self::Order>;

    /// Reduces an open order to a new non-negative total quantity without changing its priority.
    ///
    /// Zero quantity is valid. The book checks that the new quantity is lower than the current
    /// quantity but otherwise leaves quantity validation to the caller.
    ///
    /// # Errors
    ///
    /// Returns [`ReduceError::NotFound`] when the identifier is not open and
    /// [`ReduceError::NotReduced`] when the new quantity is not lower than the current quantity.
    fn reduce(
        &mut self,
        order_id: &<Self::Order as Order>::OrderId,
        new_quantity: <Self::Order as Order>::Quantity,
    ) -> Result<(), ReduceError>;
}

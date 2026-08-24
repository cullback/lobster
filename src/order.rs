//! Orders accepted by an order book.

use core::{hash::Hash, ops::Sub};

/// An order accepted by an order book.
///
/// Implementations retain ownership of their domain types and any additional order data. The book
/// only requires the operations needed to compare and reduce limit orders.
pub trait Order: Clone {
    /// The order identifier type. Hashing supports indexed order book implementations.
    type OrderId: Eq + Hash;
    /// The order quantity type.
    type Quantity: Clone + Ord + Sub<Output = Self::Quantity>;
    /// The order price type.
    type Price: Ord;

    /// Returns the order's unique identifier.
    fn id(&self) -> &Self::OrderId;

    /// Returns the order's open quantity.
    fn quantity(&self) -> &Self::Quantity;

    /// Returns the order's limit price.
    fn price(&self) -> &Self::Price;

    /// Returns `true` for a buy order and `false` for a sell order.
    fn is_buy(&self) -> bool;

    /// Replaces the order's open quantity.
    ///
    /// Order book implementations use this while matching. Users should submit changes through the
    /// order book so its invariants remain intact.
    fn set_quantity(&mut self, quantity: Self::Quantity);
}

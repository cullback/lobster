//! A basic order implementation using unsigned 32-bit values.

use crate::Order;

/// A basic limit order with a `u32` identifier, quantity, and price.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimpleOrder {
    order_id: u32,
    quantity: u32,
    price: u32,
    is_buy: bool,
}

impl SimpleOrder {
    /// Creates a buy order.
    #[must_use]
    pub const fn buy(order_id: u32, quantity: u32, price: u32) -> Self {
        Self {
            order_id,
            quantity,
            price,
            is_buy: true,
        }
    }

    /// Creates a sell order.
    #[must_use]
    pub const fn sell(order_id: u32, quantity: u32, price: u32) -> Self {
        Self {
            order_id,
            quantity,
            price,
            is_buy: false,
        }
    }

    /// Returns a copy of this order with a different quantity.
    #[must_use]
    pub const fn with_quantity(mut self, quantity: u32) -> Self {
        self.quantity = quantity;
        self
    }

    /// Returns the order's identifier.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.order_id
    }

    /// Returns the order's open quantity.
    #[must_use]
    pub const fn quantity(&self) -> u32 {
        self.quantity
    }

    /// Returns the order's limit price.
    #[must_use]
    pub const fn price(&self) -> u32 {
        self.price
    }

    /// Returns whether this is a buy order.
    #[must_use]
    pub const fn is_buy(&self) -> bool {
        self.is_buy
    }
}

impl Order for SimpleOrder {
    type OrderId = u32;
    type Quantity = u32;
    type Price = u32;

    fn id(&self) -> &Self::OrderId {
        &self.order_id
    }

    fn quantity(&self) -> &Self::Quantity {
        &self.quantity
    }

    fn price(&self) -> &Self::Price {
        &self.price
    }

    fn is_buy(&self) -> bool {
        self.is_buy
    }

    fn set_quantity(&mut self, quantity: Self::Quantity) {
        self.quantity = quantity;
    }
}

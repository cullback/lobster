//! Fills produced while matching an order.

use crate::Order;

/// A fill against a resting maker order.
///
/// The contained order's quantity is always the quantity executed. A full fill owns the maker
/// removed from the book. A partial fill owns a clone whose quantity was changed to the executed
/// quantity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fill<OrderType> {
    /// The maker was completely filled and removed from the book.
    Full(OrderType),
    /// The maker retained open quantity in the book.
    Partial(OrderType),
}

impl<OrderType> Fill<OrderType> {
    /// Returns the maker order associated with the fill.
    #[must_use]
    pub const fn maker(&self) -> &OrderType {
        match self {
            Self::Full(maker) | Self::Partial(maker) => maker,
        }
    }

    /// Returns whether the maker was completely filled.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        matches!(self, Self::Full(_))
    }

    /// Consumes the fill and returns its maker order.
    #[must_use]
    pub fn into_maker(self) -> OrderType {
        match self {
            Self::Full(maker) | Self::Partial(maker) => maker,
        }
    }
}

impl<OrderType: Order> Fill<OrderType> {
    /// Returns the quantity executed.
    #[must_use]
    pub fn quantity(&self) -> &OrderType::Quantity {
        self.maker().quantity()
    }
}

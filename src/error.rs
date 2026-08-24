//! Errors returned by order book operations.

use core::fmt;

/// An error returned when reducing an order's quantity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReduceError {
    /// No open order has the supplied identifier.
    NotFound,
    /// The new quantity is greater than or equal to the current quantity.
    NotReduced,
}

impl fmt::Display for ReduceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("order not found"),
            Self::NotReduced => {
                formatter.write_str("new quantity must be less than the current quantity")
            }
        }
    }
}

impl std::error::Error for ReduceError {}

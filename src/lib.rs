#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![cfg_attr(
    not(test),
    warn(
        clippy::float_arithmetic,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::panic,
        clippy::unwrap_used,
    )
)]

mod arena;
mod error;
mod fill;
mod flatbook;
mod levelbook;
mod order;
mod orderbook;
mod simple_order;
mod vecbook;

#[cfg(test)]
mod test;

pub use error::ReduceError;
pub use fill::Fill;
pub use flatbook::FlatLevelBook;
pub use levelbook::LevelBook;
pub use order::Order;
pub use orderbook::OrderBook;
pub use simple_order::SimpleOrder;
pub use vecbook::VecBook;

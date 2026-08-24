#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod arena;
mod error;
mod fill;
mod levelbook;
mod order;
mod orderbook;
mod simple_order;
mod vecbook;

#[cfg(test)]
mod test;

pub use error::ReduceError;
pub use fill::Fill;
pub use levelbook::LevelBook;
pub use order::Order;
pub use orderbook::OrderBook;
pub use simple_order::SimpleOrder;
pub use vecbook::VecBook;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

mod error;
pub mod deflate;
pub mod header;
pub mod inflate;

pub use crate::deflate::{CompressionLevel, Yaz0Writer};
pub use crate::error::Error;
pub use crate::header::Yaz0Header;
pub use crate::inflate::Yaz0Archive;

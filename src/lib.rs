extern crate byteorder;
extern crate arrayvec;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

#[cfg(test)]
extern crate rand;
#[cfg(test)]
extern crate indicatif;

mod error;
pub mod deflate;
pub mod header;
pub mod inflate;

pub use deflate::{CompressionLevel, Yaz0Writer};
pub use error::Error;
pub use header::Yaz0Header;
pub use inflate::Yaz0Archive;

extern crate byteorder;

mod inflate;
mod deflate;

pub use inflate::Yaz0;

use std::convert::From;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    InvalidMagic,
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Io(err)
    }
}

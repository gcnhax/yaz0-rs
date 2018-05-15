use std::io;
use std::fmt;
use std::error::Error as StdError;
use std::borrow::Cow;
use std::convert::From;

#[derive(Debug)]
pub enum Error {
    /// An error was encountered performing IO operations.
    Io(io::Error),
    /// The Yaz0 file header's magic was invalid.
    InvalidMagic,
}

impl Error {
    fn detail(&self) -> Cow<str> {
        match *self {
            Error::Io(ref io_err) => format!("IO error: {}", io_err.description()).into(),
            _ => self.description().into(),
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        fmt.write_str(&self.detail())
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Io(ref io_err) => io_err.description(),
            Error::InvalidMagic => "Invalid magic in file header",
        }
    }
}

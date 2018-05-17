use std::io;
use std::fmt;
use std::error::Error as StdError;
use std::convert::From;

#[derive(Debug)]
pub enum Error {
    /// An error was encountered performing IO operations.
    Io(io::Error),
    /// The Yaz0 file header's magic was invalid.
    InvalidMagic,
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Error::Io(io_err) => write!(f, "IO error: {}", io_err),
            _ => f.write_str(self.description()),
        }
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
        match self {
            Error::Io(io_err) => io_err.description(),
            Error::InvalidMagic => "Invalid magic in file header",
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match self {
            Error::Io(io_err) => Some(io_err),
            _ => None,
        }
    }
}

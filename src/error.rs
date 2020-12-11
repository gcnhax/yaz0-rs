use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    /// An error was encountered performing IO operations.
    #[error("backing i/o error")]
    Io(#[from] std::io::Error),
    /// The Yaz0 file header's magic was invalid.
    #[error("yaz0 header magic invalid")]
    InvalidMagic,
}

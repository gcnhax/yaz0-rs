use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use crate::error::Error;
use std::io::{Read, Seek, SeekFrom, Write};

/// The header on a Yaz0 file.
#[derive(Debug)]
pub struct Yaz0Header {
    /// Expected size of the decompressed file
    pub expected_size: usize,
}

impl Yaz0Header {
    pub fn new(expected_size: usize) -> Yaz0Header {
        Yaz0Header { expected_size }
    }

    /// Parses the header of a Yaz0 file, provided via the passed reader.
    /// Leaves the read head at the start of the data block.
    pub fn parse<R>(reader: &mut R) -> Result<Yaz0Header, Error>
    where
        R: Read + Seek,
    {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != b"Yaz0" {
            return Err(Error::InvalidMagic);
        }

        let expected_size = reader.read_u32::<BigEndian>()?;

        // consume 8 bytes
        reader.seek(SeekFrom::Current(8))?;

        Ok(Yaz0Header::new(expected_size as usize))
    }

    /// Writes the header of a Yaz0 file to the passed writer.
    /// Leaves the write head at the start of the data block.
    pub fn write<W>(&self, writer: &mut W) -> Result<(), Error>
    where
        W: Write,
    {
        writer.write_all(b"Yaz0")?;
        writer.write_u32::<BigEndian>(self.expected_size as u32)?;
        writer.write_all(&[0x0; 8])?;

        Ok(())
    }
}

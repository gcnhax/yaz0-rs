use byteorder::ReadBytesExt;
use std::io::{Read, Seek, SeekFrom};

use crate::header::Yaz0Header;
use crate::Error;

/// Wraps a reader of Yaz0 data, providing decompression methods.
#[derive(Debug)]
pub struct Yaz0Archive<R>
where
    R: Read + Seek,
{
    reader: R,

    data_start: usize,
    header: Yaz0Header,
}

impl<R> Yaz0Archive<R>
where
    R: Read + Seek,
{
    /// Creates a new `Yaz0` from a reader.
    pub fn new(mut reader: R) -> Result<Yaz0Archive<R>, Error> {
        // Parses header and advances reader to start of data
        let header = Yaz0Header::parse(&mut reader)?;

        let data_start = reader.seek(SeekFrom::Current(0))?;

        Ok(Yaz0Archive {
            reader,
            header,
            data_start: data_start as usize,
        })
    }

    /// Get the expected size of inflated data from parsed `Yaz0Header`.
    pub fn expected_size(&self) -> usize {
        self.header.expected_size
    }

    /// Decompresses the Yaz0 file, producing a `Vec<u8>` of the decompressed data.
    pub fn decompress(&mut self) -> Result<Vec<u8>, Error> {
        let mut dest: Vec<u8> = Vec::with_capacity(self.header.expected_size);
        dest.resize(self.header.expected_size, 0x00);
        self.decompress_into(&mut dest)?;
        Ok(dest)
    }

    /// Decompresses the Yaz0 file into a destination buffer.
    ///
    /// # Invariants
    /// `dest` must have a length of at least the required size to decompress successfully (consider using [`Yaz0Archive::expected_size`] to determine this)
    pub fn decompress_into(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        assert!(dest.len() >= self.expected_size());

        let mut dest_pos: usize = 0;

        let mut ops_left: u8 = 0;
        let mut code_byte: u8 = 0;

        while dest_pos < self.header.expected_size {
            if ops_left == 0 {
                code_byte = self.reader.read_u8()?;
                ops_left = 8;
            }

            if code_byte & 0x80 != 0 {
                dest[dest_pos] = self.reader.read_u8()?;
                dest_pos += 1;
            } else {
                let byte1: u8 = self.reader.read_u8()?;
                let byte2: u8 = self.reader.read_u8()?;

                // Calculate where the copy should start
                let dist = (((byte1 & 0xf) as usize) << 8) | (byte2 as usize);
                let run_base = dest_pos - (dist + 1);

                // Figure out how many bytes we have to copy
                let copy_len: usize = match byte1 >> 4 {
                    0 => self.reader.read_u8()? as usize + 0x12, // read the next input byte and add 0x12
                                                                 // to get the length to copy
                    n => n as usize + 2 // otherwise, just take the upper nybble of byte1 and add 2 to get the length
                };

                for i in 0..copy_len {
                    dest[dest_pos] = dest[run_base + i];
                    dest_pos += 1;
                }
            }

            // use next operation bit from the code byte
            code_byte <<= 1;
            ops_left -= 1;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use pretty_assertions::assert_eq;

    /// Deflate a test .szs file encoded by yaz0enc, and compare to the decompressed file produced by yaz0dec.
    #[test]
    fn test_deflate_bianco() {
        let data: &[u8] = include_bytes!("../data/test.yaz0");
        let reference_decompressed: &[u8] = include_bytes!("../data/test");

        let reader = Cursor::new(data);

        let mut f = Yaz0Archive::new(reader).unwrap();

        let deflated = f.decompress().unwrap();

        println!("{} :: {}", deflated.len(), reference_decompressed.len());

        assert!(deflated == reference_decompressed, "deflated bianco0 did not match reference deflation!");
    }

    /// Test loading a small constructed Yaz0 file containing random data.
    /// Note: this file will almost certainly error if decompression is attempted.
    #[test]
    fn test_load() {
        let data: &[u8] = &[
            // 'Yaz0'
            0x59, 0x61, 0x7a, 0x30,
            // 13371337 bytes, when deflated
            0x00, 0xcc, 0x07, 0xc9,
            // 8 bytes of zeros
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,

            // 20 bytes of (random) data
            0x69, 0x95, 0xa4, 0xa3,
            0x5f, 0xfd, 0xf6, 0x8c,
            0x7d, 0xee, 0x93, 0xc5,
            0x4a, 0x1f, 0xd3, 0x19,
            0xdc, 0x78, 0xfd, 0x3f,
        ];

        let cursor = Cursor::new(&data);
        let f = Yaz0Archive::new(cursor).unwrap();

        assert_eq!(f.header.expected_size, 13371337);
    }

    /// Check that the Yaz0 header parsing fails when provided with a file not starting with the Yaz0 magic.
    #[test]
    fn test_bad_magic() {
        let data: &[u8] = &[
            // 'Foo0'
            0x46, 0x6f, 0x6f, 0x30,
            // 13371337 bytes, when deflated
            0x00, 0xcc, 0x07, 0xc9,
            // 8 bytes of zeros
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];

        let cursor = Cursor::new(&data);
        let result = Yaz0Archive::new(cursor);

        assert!(result.is_err());
    }
}

#![allow(unused_variables)]
use arrayvec::ArrayVec;
use header::Yaz0Header;
use std::io::Write;
use Error;

pub struct Yaz0Writer {
    data: Vec<u8>,
    header: Yaz0Header,
}

fn deflate_naive(src: &[u8], quality: usize) -> Vec<u8> {
    const MAX_LOOKBACK: usize = 0x1000;
    let lookback = MAX_LOOKBACK / (quality as f32 / 10.).floor() as usize;

    let mut read_head = 0;
    let mut encoded = Vec::new();
    // -- encode a packet stream
    while read_head < src.len() {
        // the chunk codon
        let mut codon: u8 = 0x0;

        // we use this as an arena for preparing packets.
        // justification for the size:
        //   8 codes * 3 bytes/code = 24 bytes of packet (abs. max.)
        let mut packets = ArrayVec::<[u8; 24]>::new();

        // -- encode the packets
        for packet_n in 0..=7 {
            // -- search back for existing data

            // where we start
            let search_start = read_head.saturating_sub(lookback);

            // the best runlength we've seen so far, and where the match occured
            let mut best_runlength = 0;
            let mut best_match_cursor = 0;

            for search_head in search_start..read_head {
                // incremental check for every possible substring after the read head
                let mut max_runlength = 0;
                for runlength in 0..(src.len() - read_head) {
                    if src[search_head + runlength] != src[read_head + runlength] {
                        max_runlength = runlength;
                        break;
                    }
                }

                // if this search position was better than we've seen before, update our best-seen values.
                if max_runlength > best_runlength {
                    best_runlength = max_runlength;
                    best_match_cursor = search_head;
                }
            }

            if best_match_cursor > 3 {
                let dist = read_head - best_match_cursor - 1;

                if best_runlength >= 12 {
                    // 3-byte
                } else {
                    // 2-byte
                    packets.push(((best_runlength as u8 - 2) << 4) | (dist as u32 >> 8) as u8); // the rest of dist
                    packets.push((dist as u32 & 0xff) as u8); // the lsb chunk

                    read_head += best_runlength;
                }
            } else {
                // force a failout if we've hit the end of the file.
                if read_head >= src.len() {
                    break;
                }

                packets.push(src[read_head]);

                // mark the codon with the packet
                codon |= 0x80 >> packet_n;

                // push the read head forward
                read_head += 1;
            }
        }

        // -- write (codon :: packets) into the compressed stream
        encoded.push(codon);
        encoded.extend(&packets);
    }

    encoded
}

fn deflate(data: &[u8], level: CompressionLevel) -> Vec<u8> {
    match level {
        CompressionLevel::Naive { quality } => deflate_naive(data, quality),
    }
}

impl Yaz0Writer {
    pub fn new(data: Vec<u8>) -> Yaz0Writer {
        let size = data.len();
        Yaz0Writer {
            data,
            header: Yaz0Header::new(size),
        }
    }

    pub fn write<W>(self, writer: &mut W, level: CompressionLevel) -> Result<(), Error>
    where
        W: Write,
    {
        self.header.write(writer)?;
        let deflated = deflate(&self.data, level);
        writer.write(&deflated)?;

        Ok(())
    }
}

pub enum CompressionLevel {
    Naive { quality: usize },
}

#[cfg(test)]
#[cfg_attr(rustfmt, rustfmt_skip)] // don't mess up our arrays 😅
mod test {
    use super::*;

    #[test]
    fn test_deflate_naive() {
        assert_eq!(deflate_naive(&[12, 34, 56], 10), [0xe0, 12, 34, 56]);

        assert_eq!(
            deflate_naive(&[0, 1, 2, 0xa, 0, 1, 2, 3, 0xb, 0, 1, 2, 3, 4, 5, 6, 7], 10),
            [
                0xff, /* | */ 0, 1, 2, 0xa, 0, 1, 2, 3,
                0xbc, /* | */ 0xb, /**/ 32, 4, /**/ 4, 5, 6, 7,
            ]
        );
    }
}

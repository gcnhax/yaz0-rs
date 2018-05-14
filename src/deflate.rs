#![allow(unused_variables)]
use arrayvec::{self, ArrayVec};
use header::Yaz0Header;
use std::io::Write;
use Error;

pub struct Yaz0Writer<'a, W: 'a>
where
    W: Write,
{
    writer: &'a mut W,
}

#[derive(Debug, Clone, Copy)]
struct Run {
    cursor: usize,
    length: usize,
}

impl Run {
    pub fn zero() -> Run {
        Run {
            cursor: 0,
            length: 0,
        }
    }

    pub fn swap_if_better(self, other: Run) -> Run {
        if self.length > other.length {
            self
        } else {
            other
        }
    }
}

fn find_naive_run(src: &[u8], cursor: usize, lookback: usize) -> Run {
    // the location which we start searching at, `lookback` bytes before
    // the current read cursor. saturating_sub prevents underflow.
    let search_start = cursor.saturating_sub(lookback);

    // the best runlength we've seen so far, and where the match occured.
    let mut run = Run::zero();

    for search_head in search_start..cursor {
        // incremental check for every possible substring after the read head.
        let mut max_runlength = 0;
        for runlength in 0..(src.len() - cursor) {
            if src[search_head + runlength] != src[cursor + runlength] {
                max_runlength = runlength;
                break;
            }
        }

        // if this search position was better than we've seen before, update our best run.
        run = run.swap_if_better(Run {
            cursor: search_head,
            length: max_runlength,
        })
    }

    run
}

fn find_lookahead_run(src: &[u8], cursor: usize, lookback: usize) -> (bool, Run) {
    // the location which we start searching at, `lookback` bytes before
    // the current read cursor. saturating_sub prevents underflow.
    let search_start = cursor.saturating_sub(lookback);

    // get the best naive run.
    let run = find_naive_run(src, cursor, lookback);

    // was this run worthwhile at all?
    if run.length >= 3 {
        // if we look forward a single byte and reencode, how does that look?
        let lookahead_run = find_naive_run(src, cursor + 1, lookback);

        // if it's +2 better than the original naive run, pick it.
        if lookahead_run.length >= run.length + 2 {
            return (true, lookahead_run);
        }
    }

    return (false, run);
}

fn write_run<A>(read_head: usize, run: &Run, destination: &mut ArrayVec<A>) -> usize
where
    A: arrayvec::Array<Item = u8>,
{
    // compute how far back the start of the run is from the read head, minus an offset of 1
    // due to the offst, reading the byte before the read head is encoded as dist = 0.
    let dist = read_head - run.cursor - 1;

    // if the run is longer than 18 bytes, we must use a 3-byte packet instead of a 2-byte one.
    if run.length >= 0x12 {
        // 3-byte packet. this looks like the following:
        //
        // 1 byte                   2 bytes         3 bytes
        // ├────────┬───────────────┼───────────────┼───────────┐
        // │ 0b0000 │ dist (4 msbs) │ dist (8 lsbs) │ length-12 │
        // └────────┴───────────────┴───────────────┴───────────┘

        destination.push((dist as u32 >> 8) as u8);
        destination.push((dist as u32 & 0xff) as u8);
        let actual_runlength = run.length.min(0xff + 0x12); // clip to maximum possible runlength
        destination.push((actual_runlength - 0x12) as u8);

        return actual_runlength;
    } else {
        // 2-byte packet. this looks like the following:
        //
        // 1 byte                     2 bytes
        // ├──────────┬───────────────┼───────────────┐
        // │ length-2 │ dist (4 msbs) │ dist (8 lsbs) │
        // └──────────┴───────────────┴───────────────┘

        destination.push(((run.length as u8 - 2) << 4) | (dist as u32 >> 8) as u8);
        destination.push((dist as u32 & 0xff) as u8);

        return run.length;
    }
}

fn compress_lookaround(src: &[u8], quality: usize, level: CompressionLevel) -> Vec<u8> {
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
        let mut packet_n = 0;
        while packet_n < 8 {
            // -- search back for existing data
            let (is_lookahead, best_run) = match level {
                CompressionLevel::Naive { .. } => (false, find_naive_run(src, read_head, lookback)),
                CompressionLevel::Lookahead { .. } => find_lookahead_run(src, read_head, lookback),
            };

            if best_run.length >= 3 {
                // if we hit a lookahead sequence, we need to write the head byte in preparation for the run.
                if is_lookahead {
                    // push the head byte's packet
                    packets.push(src[read_head]);

                    // mark the codon with the packet
                    codon |= 0x80 >> packet_n;

                    // push the read head forward
                    read_head += 1;
                    // this is its own packet, so advance
                    packet_n += 1;
                }

                read_head += write_run(read_head, &best_run, &mut packets);
            } else {
                // force a failout if we've hit the end of the file.
                if read_head >= src.len() {
                    break;
                }

                // push the packet data
                packets.push(src[read_head]);

                // mark the codon with the packet
                codon |= 0x80 >> packet_n;

                // push the read head forward
                read_head += 1;
            }

            // advance the packet counter
            packet_n += 1;
        }

        // -- write (codon :: packets) into the compressed stream
        encoded.push(codon);
        encoded.extend(&packets);
    }

    encoded
}

fn compress(data: &[u8], level: CompressionLevel) -> Vec<u8> {
    match level {
        CompressionLevel::Naive { quality } => compress_lookaround(data, quality, level),
        CompressionLevel::Lookahead { quality } => compress_lookaround(data, quality, level),
    }
}

impl<'a, W> Yaz0Writer<'a, W>
where
    W: Write,
{
    pub fn new(writer: &'a mut W) -> Yaz0Writer<W>
    where
        W: Write,
    {
        Yaz0Writer { writer }
    }

    pub fn compress_and_write(self, data: &[u8], level: CompressionLevel) -> Result<(), Error> {
        // -- construct and write the header
        let header = Yaz0Header::new(data.len());
        header.write(self.writer)?;

        // -- compress and write the data
        let compressed = compress(data, level);
        self.writer.write(&compressed)?;

        Ok(())
    }
}

pub enum CompressionLevel {
    Naive { quality: usize },
    Lookahead { quality: usize },
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[cfg_attr(rustfmt, rustfmt_skip)] // don't mess up our arrays 😅
    fn test_deflate_naive() {
        const Q: CompressionLevel = CompressionLevel::Naive {quality: 10};

        assert_eq!(compress(&[12, 34, 56], Q), [0xe0, 12, 34, 56]);

        assert_eq!(
            compress(&[0, 1, 2, 0xa, 0, 1, 2, 3, 0xb, 0, 1, 2, 3, 4, 5, 6, 7], Q),
            [
                0xf6, /* | id:  */ 0, 1, 2, 0xa,
                      /*   run: */ 0x10, 0x03,
                      /*   id:  */ 3, 0xb,
                      /*   run: */ 0x20, 0x04,
                0xf0, /* | id:  */ 4, 5, 6, 7,
            ]
        );
    }

    #[test]
    #[cfg_attr(rustfmt, rustfmt_skip)] // don't mess up our arrays 😅
    fn test_deflate_lookahead() {
        const Q: CompressionLevel = CompressionLevel::Lookahead {quality: 10};

        assert_eq!(
            compress(&[0, 0, 0, 0xa, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xa], Q),
            [
                0xfa, /* | id:  */ 0, 0, 0, 10, 0,
                      /*   run: */ 0x70, 0x00,
                      /*   id:  */ 0xa,
            ]
        );
    }

    #[test]
    fn inverts() {
        use inflate::Yaz0Archive;
        use rand::distributions::Standard;
        use rand::{self, Rng};
        use std::io::Cursor;

        for _ in 0..10 {
            let data: Vec<u8> = rand::thread_rng().sample_iter(&Standard).take(50).collect();

            let mut deflated = Vec::new();
            Yaz0Writer::new(&mut deflated)
                .compress_and_write(&data, CompressionLevel::Lookahead { quality: 10 })
                .expect("Could not deflate");

            let inflated = Yaz0Archive::new(Cursor::new(deflated))
                .expect("Error creating Yaz0Archive")
                .decompress()
                .expect("Error deflating Yaz0 archive");

            assert_eq!(inflated, data);
        }
    }

    #[test]
    fn inverts_bianco() {
        use inflate::Yaz0Archive;
        use std::io::Cursor;

        let data: &[u8] = include_bytes!("../data/bianco0");
        let reader = Cursor::new(data);

        let mut deflated = Vec::new();
        Yaz0Writer::new(&mut deflated)
            .compress_and_write(&data, CompressionLevel::Lookahead { quality: 10 })
            .expect("Could not deflate");

        let inflated = Yaz0Archive::new(reader)
            .expect("Error creating Yaz0Archive")
            .decompress()
            .expect("Error deflating Yaz0 archive");

        assert_eq!(inflated, data);
    }
}

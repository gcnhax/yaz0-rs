use arrayvec::{self, ArrayVec};
use header::Yaz0Header;
use std::io::Write;
use std::sync::mpsc::{self, Sender};
use Error;

pub struct Yaz0Writer<'a, W: 'a>
where
    W: Write,
{
    writer: &'a mut W,
}

/// Represents a compression run of length `length` starting at `cursor`.
#[derive(Debug, Clone, Copy)]
struct Run {
    pub cursor: usize,
    pub length: usize,
}

impl Run {
    /// Returns a run of zero length starting at position 0.
    pub fn zero() -> Run {
        Run {
            cursor: 0,
            length: 0,
        }
    }

    /// Returns `self` unless `other` is a longer run, in which case it returns `other`.
    pub fn swap_if_better(self, other: Run) -> Run {
        if self.length > other.length {
            self
        } else {
            other
        }
    }
}


/// Message sent by the compressor to inform other threads of the compression progress.
#[derive(Debug)]
pub struct ProgressMsg {
    pub read_head: usize,
}

/// Naively looks back in the input stream, trying to find the longest possible
/// substring that matches the data after the current read cursor.
fn find_naive_run(src: &[u8], cursor: usize, lookback: usize) -> Run {
    // the location which we start searching at, `lookback` bytes before
    // the current read cursor. saturating_sub prevents underflow.
    let search_start = cursor.saturating_sub(lookback);

    // the best runlength we've seen so far, and where the match occured.
    let mut run = Run::zero();

    for search_head in search_start..cursor {
        // incremental check for every possible substring after the read head.
        let mut runlength = 0;
        while runlength < src.len() - cursor {
            if src[search_head + runlength] != src[cursor + runlength] {
                break;
            }
            runlength += 1;
        }

        // if this search position was better than we've seen before, update our best run.
        run = run.swap_if_better(Run {
            cursor: search_head,
            length: runlength,
        })
    }

    run
}

/// Looks back in the input stream, finding a naive run; if one is found, it tries
/// copying a single byte of that run and then finding a new one.
/// If it's at least two bytes longer than the initial run, it picks that instead and signals
/// that we need to copy that byte before copying the run.
///
/// Returns a tuple of whether we need to copy an initial byte for a lookahead run, and whatever run was found.
///
/// This is much better than plain naive search in most cases. It's also pretty much what Nintendo does.
fn find_lookahead_run(src: &[u8], cursor: usize, lookback: usize) -> (bool, Run) {
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

/// Writes a [Run] to the `destination`, with the cursor at `read_head`.
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

/// Compresses the data in `src` at [CompressionLevel] `level`, using either naive or
/// lookahead compression, sending progress updates over `progress_tx`. Returns a [Vec] containing
/// the compressed payload.
fn compress_lookaround(
    src: &[u8],
    level: CompressionLevel,
    progress_tx: Sender<ProgressMsg>,
) -> Vec<u8> {
    let quality = match level {
        CompressionLevel::Naive { quality } => quality,
        CompressionLevel::Lookahead { quality } => quality,
    };
    const MAX_LOOKBACK: usize = 0x1000;
    let lookback = MAX_LOOKBACK / (quality as f32 / 10.).floor() as usize;

    // used to cache lookahead runs to put in the next packet,
    // since we need to write a head packet first
    let mut lookahead_cache: Option<Run> = None;
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
            // -- search back for existing data. if we already have data in the lookahead cache, use that instead.
            let (hit_lookahead, best_run) = if let Some(cache) = lookahead_cache.take() {
                (false, cache)
            } else {
                match level {
                    CompressionLevel::Lookahead { .. } => {
                        find_lookahead_run(src, read_head, lookback)
                    }
                    CompressionLevel::Naive { .. } => {
                        (false, find_naive_run(src, read_head, lookback))
                    }
                }
            };

            if hit_lookahead {
                lookahead_cache = Some(best_run);
            }

            // if we hit a lookahead sequence, we need to write the head byte in preparation for the run.
            // otherwise, if the run was a compression, just do the thing.
            if best_run.length >= 3 && !hit_lookahead {
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

        if read_head % 10 == 0 || read_head == src.len() - 1 {
            // ignore errors if the rx is disconnected
            let _ = progress_tx.send(ProgressMsg { read_head });
        }
    }

    encoded
}

/// Compresses `data` with [CompressionLevel] `level`, sending progress updates over `progress_tx`.
/// Returns a [Vec] of the compressed payload.
fn compress_with_progress(
    data: &[u8],
    level: CompressionLevel,
    progress_tx: Sender<ProgressMsg>,
) -> Vec<u8> {
    match level {
        CompressionLevel::Naive { .. } | CompressionLevel::Lookahead { .. } => {
            compress_lookaround(data, level, progress_tx)
        }
    }
}

/// Compresses `data` with [CompressionLevel] `level`.
/// Returns a [Vec] of the compressed payload.
fn compress(data: &[u8], level: CompressionLevel) -> Vec<u8> {
    let (tx, _) = mpsc::channel();
    compress_with_progress(data, level, tx)
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

    /// Compress and write the passed `data`, at compression level `level`.
    pub fn compress_and_write(self, data: &[u8], level: CompressionLevel) -> Result<(), Error> {
        // -- construct and write the header
        let header = Yaz0Header::new(data.len());
        header.write(self.writer)?;

        // -- compress and write the data
        let compressed = compress(data, level);
        self.writer.write_all(&compressed)?;

        Ok(())
    }

    /// Compress and write the passed `data`, at compression level `level`.
    /// Progress updates are streamed out of `progress_tx`.
    pub fn compress_and_write_with_progress(
        self,
        data: &[u8],
        level: CompressionLevel,
        progress_tx: Sender<ProgressMsg>,
    ) -> Result<(), Error> {
        // -- construct and write the header
        let header = Yaz0Header::new(data.len());
        header.write(self.writer)?;

        // -- compress and write the data
        let compressed = compress_with_progress(data, level, progress_tx);
        self.writer.write_all(&compressed)?;

        Ok(())
    }
}

/// Represents the agressiveness of lookback used by the compressor.
#[derive(Clone, Copy)]
pub enum CompressionLevel {
    Naive {
        /// Lookback distance. Set between 1 and 10; 10 corresponds to greatest lookback distance.
        quality: usize
    },
    Lookahead {
        /// Lookback distance. Set between 1 and 10; 10 corresponds to greatest lookback distance.
        quality: usize
    },
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[cfg_attr(rustfmt, rustfmt_skip)] // don't mess up our arrays 😅
    fn deflate_naive() {
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
    fn deflate_with_lookahead() {
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
    #[cfg_attr(rustfmt, rustfmt_skip)]
    fn deflate_run() {
        const Q: CompressionLevel = CompressionLevel::Lookahead {quality: 10};

        assert_eq!(compress(&[0;30], Q), [0x80, /*| id: */ 0, /* compr: */ 0, 0, 11]);
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
    // this takes way too long on CI. TODO: figure out how to still test this on CI;
    // maybe just build _this one test_ with --release.
    #[ignore]
    fn inverts_bianco() {
        use indicatif::{ProgressBar, ProgressDrawTarget};
        use inflate::Yaz0Archive;
        use std::io::Cursor;
        use std::thread;

        let data: &[u8] = include_bytes!("../data/bianco0");

        let (tx, rx) = mpsc::channel::<ProgressMsg>();
        let pb = ProgressBar::new(data.len() as u64);
        pb.set_draw_target(ProgressDrawTarget::stdout());
        thread::spawn(move || {
            while let Ok(progress) = rx.recv() {
                pb.set_position(progress.read_head as u64);
            }
        });

        let mut deflated = Vec::new();
        Yaz0Writer::new(&mut deflated)
            .compress_and_write_with_progress(
                &data,
                CompressionLevel::Lookahead { quality: 10 },
                tx,
            )
            .expect("Could not deflate");

        let reader = Cursor::new(&deflated);

        let inflated = Yaz0Archive::new(reader)
            .expect("Error creating Yaz0Archive")
            .decompress()
            .expect("Error deflating Yaz0 archive");

        println!(
            "original: {:#x} / compressed (w/ header): {:#x} ({:.3}%)",
            data.len(),
            deflated.len(),
            deflated.len() as f64 * 100. / data.len() as f64
        );

        assert_eq!(inflated, data);
    }
}

# yaz0 - a Rust crate for de/compressing Nintendo Yaz0 ("SZS") files

[![Build Status](https://travis-ci.com/gcnhax/yaz0-rs.svg?branch=master)](https://travis-ci.com/gcnhax/yaz0-rs)
[![codecov](https://codecov.io/gh/gcnhax/yaz0-rs/branch/master/graph/badge.svg)](https://codecov.io/gh/gcnhax/yaz0-rs)
[![Crates.io Version](https://img.shields.io/crates/v/yaz0.svg)](https://crates.io/crates/yaz0)

Yaz0 is Nintendo's version of Haruhiko Okumura's in/famous 1989 LZSS implementation. It's been continually used in first party titles, wrapping various other formats, since the N64 era.

**2024 note**: you might want to look into [szs](https://crates.io/crates/szs), by riidefi, which has much higher performance and a wide selection of compression methods, including emulations of compression methods used for specific games. It's not pure Rust, but it's unlikely that matters in any application you're using a SZS decompressor in.

## tools
To install `yaztool`, a de/flating utility for yaz0 files, do
```
$ cargo install yaz0 --features=yaztool
```

## licensing
All code in this repository is licensed under the MIT license; see `LICENSE`.

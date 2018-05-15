extern crate clap;
extern crate indicatif;
extern crate yaz0;

use std::io::Write;
use clap::{App, Arg};
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use yaz0::Yaz0Archive;

fn main() -> Result<(), Box<Error>> {
    let matches = App::new("yaztool")
        .author("Erin Moon <erin@hashbang.sh>")
        .about("(de)compresses Yaz0 files")
        .arg(Arg::with_name("INPUT")
            .required(true))
        .arg(Arg::with_name("OUTPUT")
            .required(true))
        .get_matches();



    let in_path = Path::new(matches.value_of("INPUT").unwrap());
    let out_path = Path::new(matches.value_of("OUTPUT").unwrap());

    let reader = BufReader::new(File::open(in_path)?);

    let mut yazfile = Yaz0Archive::new(reader)?;
    let deflated = yazfile.decompress()?;

    let mut outfile = File::create(out_path)?;
    outfile.write_all(&deflated)?;

    Ok(())
}

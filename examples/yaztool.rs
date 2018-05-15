extern crate clap;
extern crate indicatif;
extern crate yaz0;

use std::io::Write;
use clap::{App, AppSettings, Arg, SubCommand};
use indicatif::ProgressBar;
use std::error::Error;
use std::fs::File;
use std::io::{Read, BufReader};
use std::sync::mpsc;
use std::thread;
use std::path::Path;
use yaz0::{Yaz0Archive, Yaz0Writer, CompressionLevel};
use yaz0::deflate::ProgressMsg;

fn main() -> Result<(), Box<Error>> {
    let matches = App::new("yaztool")
        .author("Erin Moon <erin@hashbang.sh>")
        .about("(de)compresses Yaz0 files")
        .setting(AppSettings::ArgRequiredElseHelp)
        .subcommand(SubCommand::with_name("decompress")
                    .arg(Arg::with_name("INPUT")
                        .required(true))
                    .arg(Arg::with_name("OUTPUT")
                        .required(true)))
        .subcommand(SubCommand::with_name("compress")
            .arg(Arg::with_name("INPUT")
                .required(true))
            .arg(Arg::with_name("OUTPUT")
                .required(true)))
        .get_matches();

    match matches.subcommand() {
        ("decompress", Some(matches)) => {
            let in_path = Path::new(matches.value_of("INPUT").unwrap());
            let out_path = Path::new(matches.value_of("OUTPUT").unwrap());

            let reader = BufReader::new(File::open(in_path)?);

            let mut yazfile = Yaz0Archive::new(reader)?;
            let inflated = yazfile.decompress()?;

            let mut outfile = File::create(out_path)?;
            outfile.write_all(&inflated)?;
        },
        ("compress", Some(matches)) => {
            let in_path = Path::new(matches.value_of("INPUT").unwrap());
            let out_path = Path::new(matches.value_of("OUTPUT").unwrap());

            let data = {
                let mut d = Vec::new();
                File::open(in_path)?.read_to_end(&mut d)?;
                d
            };

            let pb = ProgressBar::new(data.len() as u64);
            let (tx, rx) = mpsc::channel::<ProgressMsg>();
            thread::spawn(move || {
                while let Ok(progress) = rx.recv() {
                    pb.set_position(progress.read_head as u64);
                }
            });

            let quality = CompressionLevel::Lookahead {quality: 10};
            let deflated = {
                let mut d = Vec::new();
                Yaz0Writer::new(&mut d)
                    .compress_and_write_with_progress(&data, quality, tx)?;
                d
            };

            let mut outfile = File::create(out_path)?;
            outfile.write_all(&deflated)?;
        },
        _ => unreachable!(),
    }

    Ok(())
}

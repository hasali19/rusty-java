use std::fs::File;
use std::io::BufReader;

use clap::Parser;
use color_eyre::eyre::{self, eyre, Context};
use rusty_java::reader::ClassReader;

#[derive(clap::Parser)]
struct Args {
    class_file: String,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let args = Args::parse();
    let class_file = ClassReader::new(BufReader::new(File::open(&args.class_file)?))
        .read_class_file()
        .wrap_err_with(|| eyre!("failed to read class file at '{}'", args.class_file))?;

    println!("{class_file:#?}");

    Ok(())
}

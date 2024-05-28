use std::io::Write;

use clap::Parser;
use color_eyre::eyre::{self, ContextCompat};
use jdk_tools::Jvm;

#[derive(Parser)]
struct Args {
    class: String,
    #[clap(short, long)]
    out: Option<String>,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let args = Args::parse();
    let out_path = args
        .out
        .or_else(|| {
            let class_name = args.class.split('/').next_back()?;
            Some(format!("{class_name}.class"))
        })
        .wrap_err("could not determine a suitable output path, please specify one")?;

    let bytes = Jvm::new()?.extract_jrt_class(&args.class)?;

    if out_path == "-" {
        std::io::stdout().write_all(&bytes)?;
    } else {
        std::fs::write(out_path, &bytes)?;
    }

    Ok(())
}

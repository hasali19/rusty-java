use bumpalo::Bump;
use clap::Parser;
use color_eyre::eyre::{self, Context, ContextCompat};
use rusty_java::vm::Vm;

#[derive(clap::Parser)]
struct Args {
    class_file: String,
    #[clap(long)]
    dump: bool,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let args = Args::parse();

    let arena = Bump::new();
    let mut vm = Vm::new(&arena);

    let class = vm.load_class_file(&args.class_file)?;

    if args.dump {
        println!("{class:#?}");
    } else {
        let main = class
            .method("main", "([Ljava/lang/String;)V")
            .wrap_err("main method not found")?;

        vm.call_method(class, main)
            .wrap_err("failed to execute main method")?;
    }

    Ok(())
}

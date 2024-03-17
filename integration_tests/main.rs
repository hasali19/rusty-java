#![feature(exit_status_error)]

use std::fs::{self, File};
use std::path::Path;
use std::process::Command;

use bumpalo::Bump;
use color_eyre::eyre::{self, ContextCompat};
use libtest_mimic::{Arguments, Failed, Trial};
use rusty_java::vm::Vm;

fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let args = Arguments::from_args();
    let tests_dir = Path::new(file!()).parent().unwrap();

    let tests = fs::read_dir(tests_dir)?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let ext = path.extension()?.to_str()?;

            if ext == "java" {
                Some(path.file_stem()?.to_str()?.to_owned())
            } else {
                None
            }
        })
        .map(create_trial)
        .collect();

    libtest_mimic::run(&args, tests).exit();
}

fn create_trial(name: String) -> Trial {
    Trial::test(name.clone(), move || {
        if let Err(e) = run_trial(&name) {
            eprintln!("{e:?}");
            return Err(Failed::without_message());
        }
        Ok(())
    })
}

fn run_trial(name: &str) -> eyre::Result<()> {
    let arena = Bump::new();
    let mut stdout = Vec::new();
    let mut vm = Vm::new(&arena, &mut stdout);

    let source_file_path = Path::new(file!())
        .parent()
        .unwrap()
        .join(&name)
        .with_extension("java");

    if !check_stamp(&source_file_path) {
        eprintln!("{source_file_path:?} was modified, recompiling");
        Command::new("javac")
            .arg(&source_file_path)
            .status()?
            .exit_ok()?;
        File::create(source_file_path.with_extension("stamp"))?;
    }

    let class_file_path = source_file_path.with_extension("class");
    let class = vm.load_class_file(class_file_path.to_str().unwrap())?;

    vm.call_method(
        class,
        class
            .method("main", "([Ljava/lang/String;)V")
            .wrap_err("main method not found")?,
    )?;

    let stdout = String::from_utf8(stdout)?;

    insta::assert_snapshot!(name, stdout);

    Ok(())
}

fn check_stamp(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    let stamp_path = path.with_extension("stamp");

    if !stamp_path.exists() {
        return false;
    }

    let mtime = path.metadata().unwrap().modified().unwrap();
    let stamp_mtime = stamp_path.metadata().unwrap().modified().unwrap();

    stamp_mtime > mtime
}

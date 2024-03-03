use std::io::Write;

use clap::Parser;
use color_eyre::eyre::{self, ContextCompat};
use jni::objects::{JByteArray, JObject, JValue};
use jni::{InitArgsBuilder, JNIVersion, JavaVM};

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

    let jvm = JavaVM::new(
        InitArgsBuilder::new()
            .version(JNIVersion::V8)
            .option("-Xcheck:jni")
            .build()?,
    )?;

    let mut env = jvm.attach_current_thread()?;

    let root_path = env.new_string("jrt:/")?;

    let uri = env.call_static_method(
        "java/net/URI",
        "create",
        "(Ljava/lang/String;)Ljava/net/URI;",
        &[JValue::from(&root_path)],
    )?;

    let jrt_fs = env.call_static_method(
        "java/nio/file/FileSystems",
        "getFileSystem",
        "(Ljava/net/URI;)Ljava/nio/file/FileSystem;",
        &[JValue::from(&uri)],
    )?;

    let path_components = env.new_object_array(0, "java/lang/String", JObject::null())?;

    let relative_class_path = env.new_string(format!("modules/java.base/{}.class", args.class))?;
    let class_path = env.call_method(
        jrt_fs.l()?,
        "getPath",
        "(Ljava/lang/String;[Ljava/lang/String;)Ljava/nio/file/Path;",
        &[
            JValue::from(&relative_class_path),
            JValue::from(&path_components),
        ],
    )?;

    let bytes = env.call_static_method(
        "java/nio/file/Files",
        "readAllBytes",
        "(Ljava/nio/file/Path;)[B",
        &[JValue::from(&class_path)],
    )?;

    let bytes = JByteArray::from(bytes.l()?);
    let bytes = env.convert_byte_array(bytes)?;

    if out_path == "-" {
        std::io::stdout().write_all(&bytes)?;
    } else {
        std::fs::write(out_path, &bytes)?;
    }

    Ok(())
}

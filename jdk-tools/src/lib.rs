use color_eyre::eyre;
use jni::objects::{JByteArray, JObject, JValue};
use jni::{InitArgsBuilder, JNIVersion, JavaVM};

pub struct Jvm {
    jvm: JavaVM,
}

impl Jvm {
    pub fn new() -> eyre::Result<Jvm> {
        Ok(Jvm {
            jvm: JavaVM::new(
                InitArgsBuilder::new()
                    .version(JNIVersion::V8)
                    .option("-Xcheck:jni")
                    .build()?,
            )?,
        })
    }

    pub fn extract_jrt_class(&self, class_name: &str) -> eyre::Result<Vec<u8>> {
        let mut env = self.jvm.attach_current_thread()?;

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

        let relative_class_path =
            env.new_string(format!("modules/java.base/{class_name}.class"))?;
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

        Ok(bytes)
    }
}

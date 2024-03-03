use std::collections::HashMap;

use color_eyre::eyre::{self, ContextCompat};

use crate::class_file::constant_pool::ConstantPool;
use crate::class_file::{ClassFile, MethodInfo};

#[derive(Debug)]
pub struct Class<'a> {
    class_file: &'a ClassFile,
    methods: HashMap<&'a str, &'a MethodInfo>,
}

impl<'a> Class<'a> {
    pub fn new(class_file: &'a ClassFile) -> eyre::Result<Class> {
        Ok(Class {
            class_file,
            methods: class_file
                .methods
                .iter()
                .map(|method| -> eyre::Result<_> {
                    let name = class_file
                        .constant_pool
                        .get(method.name_index)
                        .wrap_err("missing method name in constant pool")?
                        .try_as_utf_8_ref()
                        .wrap_err("invalid method name in constant pool")?;
                    Ok((name.as_str(), method))
                })
                .collect::<eyre::Result<HashMap<_, _>>>()?,
        })
    }

    pub fn method(&self, name: &str) -> Option<&'a MethodInfo> {
        self.methods.get(name).copied()
    }

    pub fn constant_pool(&self) -> &'a ConstantPool {
        &self.class_file.constant_pool
    }
}

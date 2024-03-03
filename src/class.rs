use std::collections::HashMap;
use std::fmt::Debug;

use color_eyre::eyre::{self, ContextCompat};

use crate::class_file::constant_pool::ConstantPool;
use crate::class_file::{ClassFile, MethodInfo};

#[derive(Debug)]
pub struct Class<'a> {
    class_file: &'a ClassFile,
    methods: HashMap<MethodId<'a>, &'a MethodInfo>,
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

                    let descriptor = class_file
                        .constant_pool
                        .get(method.descriptor_index)
                        .wrap_err("missing method descriptor in constant pool")?
                        .try_as_utf_8_ref()
                        .wrap_err("invalid method descriptor in constant pool")?;

                    Ok((MethodId { name, descriptor }, method))
                })
                .collect::<eyre::Result<HashMap<_, _>>>()?,
        })
    }

    pub fn method(&self, name: &str, descriptor: &str) -> Option<&'a MethodInfo> {
        self.methods.get(&MethodId { name, descriptor }).copied()
    }

    pub fn constant_pool(&self) -> &'a ConstantPool {
        &self.class_file.constant_pool
    }
}

#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct MethodId<'a> {
    name: &'a str,
    descriptor: &'a str,
}

impl<'a> Debug for MethodId<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}{}\"", self.name, self.descriptor)
    }
}

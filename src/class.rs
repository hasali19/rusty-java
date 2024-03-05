use std::fmt::Debug;

use bumpalo::Bump;
use color_eyre::eyre::{self, ContextCompat};
use hashbrown::hash_map::DefaultHashBuilder;
use hashbrown::HashMap;

use crate::class_file::constant_pool::ConstantPool;
use crate::class_file::{ClassFile, MethodInfo};

#[derive(Debug)]
pub struct Class<'a> {
    class_file: &'a ClassFile<'a>,
    methods: HashMap<MethodId<'a>, &'a MethodInfo<'a>, DefaultHashBuilder, &'a Bump>,
}

impl<'a> Class<'a> {
    pub fn new(arena: &'a Bump, class_file: &'a ClassFile) -> eyre::Result<Class<'a>> {
        Ok(Class {
            class_file,
            methods: {
                let mut methods = HashMap::new_in(arena);
                for method in &class_file.methods {
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

                    methods.insert(MethodId { name, descriptor }, method);
                }
                methods
            },
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

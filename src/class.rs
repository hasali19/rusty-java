use std::fmt::Debug;

use bumpalo::Bump;
use color_eyre::eyre::{self, ContextCompat};
use hashbrown::hash_map::DefaultHashBuilder;
use hashbrown::HashMap;

use crate::class_file::constant_pool::ConstantPool;
use crate::class_file::{ClassFile, MethodAccessFlags};
use crate::instructions::Instruction;

#[derive(Debug)]
pub struct Class<'a> {
    class_file: &'a ClassFile<'a>,
    methods: HashMap<MethodId<'a>, Method<'a>, DefaultHashBuilder, &'a Bump>,
}

#[derive(Debug)]
pub struct Method<'a> {
    pub access_flags: MethodAccessFlags,
    pub body: Option<MethodBody<'a>>,
}

#[derive(Debug)]
pub struct MethodBody<'a> {
    pub locals: usize,
    pub stack_size: usize,
    pub code: &'a [Instruction],
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

                    methods.insert(
                        MethodId { name, descriptor },
                        Method {
                            access_flags: method.access_flags,
                            body: method.attributes.iter().find_map(|attr| {
                                let attr = attr.try_as_code_ref()?;
                                Some(MethodBody {
                                    locals: attr.max_locals as usize,
                                    stack_size: attr.max_stack as usize,
                                    code: attr.code.as_slice(),
                                })
                            }),
                        },
                    );
                }
                methods
            },
        })
    }

    pub fn method<'b: 'a>(&'a self, name: &'b str, descriptor: &'b str) -> Option<&'a Method<'a>> {
        self.methods.get(&MethodId { name, descriptor })
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

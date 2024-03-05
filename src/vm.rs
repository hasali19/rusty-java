use std::fs::File;
use std::io::BufReader;

use bumpalo::Bump;
use color_eyre::eyre::{self, bail, eyre, Context, ContextCompat};

use crate::class::{Class, Method};
use crate::class_file::constant_pool::{self, ConstantInfo};
use crate::class_file::MethodAccessFlags;
use crate::instructions::{Instruction, InvokeKind, LoadStoreType, NumberType, ReturnType};
use crate::reader::ClassReader;

#[expect(unused)]
enum Operand<'a> {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Char(u16),
    Float(f32),
    Double(f64),
    Boolean(bool),
    ReturnAddress(usize),
    StringConst(&'a str),
}

#[expect(unused)]
#[derive(Clone, Copy, Debug)]
enum Local {
    None,
    Boolean(bool),
    Byte(i8),
    Char(u16),
    Short(i16),
    Int(i32),
    Float(f32),
    Reference(usize),
    ReturnAddress(usize),
}

pub struct Vm<'a> {
    arena: &'a Bump,
}

impl<'a> Vm<'a> {
    pub fn new(arena: &'a Bump) -> Vm<'a> {
        Vm { arena }
    }

    pub fn load_class_file(&mut self, name: &str) -> eyre::Result<&'a Class<'a>> {
        let class_file = self.arena.alloc(
            ClassReader::new(self.arena, BufReader::new(File::open(name)?))
                .read_class_file()
                .wrap_err_with(|| eyre!("failed to read class file at '{}'", name))?,
        );

        let class = self.arena.alloc(Class::new(self.arena, class_file)?);

        if let Some(clinit) = class.method("<clinit>", "()V")
            && clinit.access_flags.contains(MethodAccessFlags::STATIC)
        {
            self.call_method(class, clinit)?;
        }

        Ok(class)
    }

    pub fn call_method(&mut self, class: &Class, method: &Method) -> eyre::Result<()> {
        let body = method.body.as_ref().wrap_err("missing method body")?;

        let mut pc = 0;
        let mut locals = vec![Local::None; body.locals];
        let mut operand_stack = Vec::with_capacity(body.stack_size);

        loop {
            let instruction = &body.code[pc];
            match instruction {
                Instruction::r#return {
                    data_type: ReturnType::Void,
                } => {
                    // TODO: synchronized methods
                    break;
                }
                Instruction::r#const { data_type, value } => {
                    let operand = match data_type {
                        NumberType::Int => Operand::Int(*value as i32),
                        NumberType::Long => todo!(),
                        NumberType::Float => todo!(),
                        NumberType::Double => todo!(),
                    };
                    operand_stack.push(operand);
                    pc += 1;
                }
                Instruction::store {
                    data_type: LoadStoreType::Int,
                    index,
                } => {
                    let operand = operand_stack
                        .pop()
                        .wrap_err("no operand provided to istore")?;

                    locals[*index as usize] = match operand {
                        Operand::Byte(v) => Local::Byte(v),
                        Operand::StringConst(_) => todo!(),
                        Operand::Int(_) => todo!(),
                        Operand::Short(_) => todo!(),
                        Operand::Long(_) => todo!(),
                        Operand::Char(_) => todo!(),
                        Operand::Float(_) => todo!(),
                        Operand::Double(_) => todo!(),
                        Operand::Boolean(_) => todo!(),
                        Operand::ReturnAddress(_) => todo!(),
                    };

                    pc += 1;
                }
                Instruction::load {
                    data_type: LoadStoreType::Int,
                    index,
                } => {
                    let val = match locals[*index as usize] {
                        Local::None => 0,
                        Local::Int(v) => v,
                        Local::Byte(v) => v as i32,
                        local => bail!("iload called with invalid local: {local:?}"),
                    };

                    operand_stack.push(Operand::Int(val));

                    pc += 1;
                }
                Instruction::invoke {
                    kind: InvokeKind::Dynamic,
                    index,
                } => {
                    let invoke_dynamic = &class.constant_pool()[*index]
                        .try_as_invoke_dynamic_ref()
                        .wrap_err("invalid operand for invokedynamic")?;

                    let name_and_type = class.constant_pool()[invoke_dynamic.name_and_type_index]
                        .try_as_name_and_type_ref()
                        .wrap_err("expected name_and_type")?;

                    let name = class.constant_pool()[name_and_type.name_index]
                        .try_as_utf_8_ref()
                        .wrap_err("expected utf8")?;

                    panic!("exec {name}");
                }
                Instruction::invoke {
                    kind: InvokeKind::Static,
                    index,
                } => {
                    let invoke_dynamic = &class.constant_pool()[*index]
                        .try_as_method_ref_ref()
                        .wrap_err("expected methodref")?;

                    let name_and_type = class.constant_pool()[invoke_dynamic.name_and_type_index]
                        .try_as_name_and_type_ref()
                        .wrap_err("expected name_and_type")?;

                    let name = class.constant_pool()[name_and_type.name_index]
                        .try_as_utf_8_ref()
                        .wrap_err("expected utf8")?;

                    let descriptor = class.constant_pool()[name_and_type.descriptor_index]
                        .try_as_utf_8_ref()
                        .wrap_err("expected utf8")?;

                    let method = class
                        .method(name, descriptor)
                        .wrap_err_with(|| eyre!("method not found: {name}{descriptor}"))?;

                    if method.access_flags.contains(MethodAccessFlags::NATIVE) && name == "print" {
                        let arg = operand_stack.pop().wrap_err("missing argument to print")?;
                        match arg {
                            Operand::Byte(v) => print!("{v}"),
                            Operand::StringConst(v) => print!("{v}"),
                            Operand::Int(v) => print!("{v}"),
                            Operand::Short(_) => todo!(),
                            Operand::Long(_) => todo!(),
                            Operand::Char(_) => todo!(),
                            Operand::Float(_) => todo!(),
                            Operand::Double(_) => todo!(),
                            Operand::Boolean(_) => todo!(),
                            Operand::ReturnAddress(_) => todo!(),
                        }
                        pc += 1;
                    } else {
                        todo!("exec {name}");
                    }
                }
                Instruction::ldc { index } => {
                    match &class.constant_pool()[*index] {
                        ConstantInfo::String(constant_pool::String { string_index }) => {
                            operand_stack.push(Operand::StringConst(
                                class.constant_pool()[*string_index]
                                    .try_as_utf_8_ref()
                                    .wrap_err("expected utf8")?,
                            ))
                        }
                        _ => todo!(),
                    };
                    pc += 1;
                }
                _ => todo!("unimplemented instruction: {instruction:?}"),
            }
        }

        Ok(())
    }
}

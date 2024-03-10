use color_eyre::eyre::{self, bail, eyre, ContextCompat};

use crate::class::{Class, Method};
use crate::class_file::constant_pool::{self, ConstantInfo};
use crate::class_file::MethodAccessFlags;
use crate::instructions::{Instruction, InvokeKind, LoadStoreType, NumberType, ReturnType};

pub enum Operand<'a> {
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

#[derive(Clone, Copy, Debug)]
pub enum Local {
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

pub struct CallFrame<'a> {
    class: &'a Class<'a>,
    method: &'a Method<'a>,
    pc: usize,
    locals: Vec<Local>,
    operand_stack: Vec<Operand<'a>>,
}

impl<'a> CallFrame<'a> {
    pub fn new(
        class: &'a Class<'a>,
        method: &'a Method<'a>,
        args: impl Iterator<Item = Local>,
    ) -> eyre::Result<CallFrame<'a>> {
        let body = method.body.as_ref().wrap_err("missing method body")?;

        let mut locals = vec![Local::None; body.locals];

        for (i, arg) in (0..method.descriptor.params.len()).zip(args) {
            locals[i] = arg;
        }

        Ok(CallFrame {
            class,
            method,
            pc: 0,
            locals,
            operand_stack: Vec::with_capacity(body.stack_size),
        })
    }

    pub fn execute(mut self) -> eyre::Result<Option<Operand<'a>>> {
        let body = self.method.body.as_ref().wrap_err("missing method body")?;

        if self
            .method
            .access_flags
            .contains(MethodAccessFlags::SYNCHRONIZED)
        {
            todo!("synchronized methods")
        }

        loop {
            let instruction = &body.code[self.pc];
            match instruction {
                Instruction::r#return { data_type } => {
                    if self
                        .method
                        .access_flags
                        .contains(MethodAccessFlags::SYNCHRONIZED)
                    {
                        todo!("synchronized methods")
                    }

                    let ret = match data_type {
                        ReturnType::Void => None,
                        ReturnType::Int => {
                            return Ok(Some(
                                self.operand_stack.pop().wrap_err("missing return value")?,
                            ))
                        }
                        ReturnType::Long => todo!(),
                        ReturnType::Float => todo!(),
                        ReturnType::Double => todo!(),
                        ReturnType::Reference => todo!(),
                    };

                    return Ok(ret);
                }
                Instruction::r#const { data_type, value } => {
                    let operand = match data_type {
                        NumberType::Int => Operand::Int(*value as i32),
                        NumberType::Long => todo!(),
                        NumberType::Float => todo!(),
                        NumberType::Double => todo!(),
                    };
                    self.operand_stack.push(operand);
                    self.pc += 1;
                }
                Instruction::store {
                    data_type: LoadStoreType::Int,
                    index,
                } => {
                    let operand = self
                        .operand_stack
                        .pop()
                        .wrap_err("no operand provided to istore")?;

                    self.locals[*index as usize] = match operand {
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

                    self.pc += 1;
                }
                Instruction::load {
                    data_type: LoadStoreType::Int,
                    index,
                } => {
                    let val = match self.locals[*index as usize] {
                        Local::None => 0,
                        Local::Int(v) => v,
                        Local::Byte(v) => v as i32,
                        local => bail!("iload called with invalid local: {local:?}"),
                    };

                    self.operand_stack.push(Operand::Int(val));

                    self.pc += 1;
                }
                Instruction::ldc { index } => {
                    match &self.class.constant_pool()[*index] {
                        ConstantInfo::String(constant_pool::String { string_index }) => {
                            self.operand_stack.push(Operand::StringConst(
                                self.class.constant_pool()[*string_index]
                                    .try_as_utf_8_ref()
                                    .wrap_err("expected utf8")?,
                            ))
                        }
                        _ => todo!(),
                    };
                    self.pc += 1;
                }
                Instruction::invoke { kind, index } => {
                    self.execute_invoke(*index, *kind)?;
                    self.pc += 1;
                }
                _ => todo!("unimplemented instruction: {instruction:?}"),
            }
        }
    }

    fn execute_invoke(&mut self, const_index: u16, kind: InvokeKind) -> eyre::Result<()> {
        let method_ref = &self.class.constant_pool()[const_index]
            .try_as_method_ref_ref()
            .wrap_err("expected methodref")?;

        let name_and_type = self.class.constant_pool()[method_ref.name_and_type_index]
            .try_as_name_and_type_ref()
            .wrap_err("expected name_and_type")?;

        let name = self.class.constant_pool()[name_and_type.name_index]
            .try_as_utf_8_ref()
            .wrap_err("expected utf8")?;

        let descriptor = self.class.constant_pool()[name_and_type.descriptor_index]
            .try_as_utf_8_ref()
            .wrap_err("expected utf8")?;

        let method = self
            .class
            .method(name, descriptor)
            .wrap_err_with(|| eyre!("method not found: {name}{descriptor}"))?;

        match kind {
            InvokeKind::Static => {
                if method.access_flags.contains(MethodAccessFlags::NATIVE) && name == "print" {
                    let arg = self
                        .operand_stack
                        .pop()
                        .wrap_err("missing argument to print")?;

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
                } else {
                    let args = method
                        .descriptor
                        .params
                        .iter()
                        .map(|_| self.operand_stack.pop().unwrap())
                        .map(|op| match op {
                            Operand::Byte(_) => todo!(),
                            Operand::Short(_) => todo!(),
                            Operand::Int(v) => Local::Int(v),
                            Operand::Long(_) => todo!(),
                            Operand::Char(_) => todo!(),
                            Operand::Float(_) => todo!(),
                            Operand::Double(_) => todo!(),
                            Operand::Boolean(_) => todo!(),
                            Operand::ReturnAddress(_) => todo!(),
                            Operand::StringConst(_) => todo!(),
                        });

                    if let Some(ret) = CallFrame::new(self.class, method, args)?.execute()? {
                        self.operand_stack.push(ret);
                    }
                }
            }
            _ => todo!(),
        }

        Ok(())
    }
}

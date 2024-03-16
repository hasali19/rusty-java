use std::io;

use color_eyre::eyre::{self, bail, eyre, ContextCompat};
use strum::EnumTryAs;

use crate::class::{Class, Method};
use crate::class_file::constant_pool::{self, ConstantInfo};
use crate::class_file::MethodAccessFlags;
use crate::instructions::{
    Condition, Instruction, InvokeKind, LoadStoreType, NumberType, ReturnType,
};

#[derive(EnumTryAs)]
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

#[derive(Clone, Copy, Debug, EnumTryAs)]
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

pub struct CallFrame<'a, 'b> {
    class: &'a Class<'a>,
    method: &'a Method<'a>,
    pc: usize,
    locals: Vec<Local>,
    operand_stack: Vec<Operand<'a>>,
    stdout: &'b mut dyn io::Write,
}

impl<'a, 'b> CallFrame<'a, 'b> {
    pub fn new(
        class: &'a Class<'a>,
        method: &'a Method<'a>,
        args: impl Iterator<Item = Local>,
        stdout: &'b mut dyn io::Write,
    ) -> eyre::Result<CallFrame<'a, 'b>> {
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
            stdout,
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
                        Operand::Int(v) => Local::Int(v),
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
                Instruction::add { data_type } => {
                    let a = self.operand_stack.pop().wrap_err("missing add operand")?;
                    let b = self.operand_stack.pop().wrap_err("missing add operand")?;
                    match data_type {
                        NumberType::Int => self.operand_stack.push(Operand::Int(
                            a.try_as_int().wrap_err("invalid type")?
                                + b.try_as_int().wrap_err("invalid type")?,
                        )),
                        NumberType::Long => todo!(),
                        NumberType::Float => todo!(),
                        NumberType::Double => todo!(),
                    }
                    self.pc += 1;
                }
                Instruction::bipush { value } => {
                    self.operand_stack.push(Operand::Int(*value as i32));
                    self.pc += 1;
                }
                Instruction::if_icmp { condition, branch } => {
                    let v2 = self.operand_stack.pop().unwrap().try_as_int().unwrap();
                    let v1 = self.operand_stack.pop().unwrap().try_as_int().unwrap();

                    let condition = match condition {
                        Condition::Eq => v1 == v2,
                        Condition::Ne => v1 != v2,
                        Condition::Lt => v1 < v2,
                        Condition::Le => v1 <= v2,
                        Condition::Gt => v1 > v2,
                        Condition::Ge => v1 >= v2,
                    };

                    if condition {
                        self.pc = self.pc.checked_add_signed(*branch as isize).unwrap();
                    } else {
                        self.pc += 1;
                    }
                }
                Instruction::rem { data_type } => {
                    let result = match data_type {
                        NumberType::Int => {
                            let v2 = self.operand_stack.pop().unwrap().try_as_int().unwrap();
                            let v1 = self.operand_stack.pop().unwrap().try_as_int().unwrap();
                            Operand::Int(v1 % v2)
                        }
                        NumberType::Long => todo!(),
                        NumberType::Float => todo!(),
                        NumberType::Double => todo!(),
                    };

                    self.operand_stack.push(result);
                    self.pc += 1;
                }
                Instruction::r#if { condition, branch } => {
                    let value = self
                        .operand_stack
                        .pop()
                        .wrap_err("missing operand for if comparison")?
                        .try_as_int()
                        .wrap_err("expected int")?;

                    let condition = match condition {
                        Condition::Eq => value == 0,
                        Condition::Ne => value != 0,
                        Condition::Lt => value < 0,
                        Condition::Le => value <= 0,
                        Condition::Gt => value > 0,
                        Condition::Ge => value >= 0,
                    };

                    if condition {
                        self.pc = self.pc.checked_add_signed(*branch as isize).unwrap();
                    } else {
                        self.pc += 1;
                    }
                }
                Instruction::goto { branch } => {
                    self.pc = self.pc.checked_add_signed(*branch as isize).unwrap();
                }
                Instruction::inc { index, value } => {
                    *self.locals[*index as usize].try_as_int_mut().unwrap() += *value as i32;
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
                        Operand::Byte(v) => write!(self.stdout, "{v}")?,
                        Operand::StringConst(v) => write!(self.stdout, "{v}")?,
                        Operand::Int(v) => write!(self.stdout, "{v}")?,
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

                    if let Some(ret) =
                        CallFrame::new(self.class, method, args, self.stdout)?.execute()?
                    {
                        self.operand_stack.push(ret);
                    }
                }
            }
            _ => todo!(),
        }

        Ok(())
    }
}

use std::alloc::Layout;
use std::io;

use bumpalo::Bump;
use color_eyre::eyre::{self, bail, eyre, ContextCompat};
use strum::EnumTryAs;

use crate::class::{Class, Method};
use crate::class_file::constant_pool::{self, ConstantInfo};
use crate::class_file::MethodAccessFlags;
use crate::instructions::{
    ArrayLoadStoreType, ArrayType, Condition, Instruction, InvokeKind, LoadStoreType, NumberType,
    ReturnType,
};

#[derive(Debug, EnumTryAs)]
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
    Reference(usize),
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

#[derive(Debug)]
#[repr(C)]
struct ArrayHeader {
    atype: ArrayType,
    length: usize,
}

impl ArrayHeader {
    unsafe fn data<'a, T>(&mut self) -> eyre::Result<&'a mut [T]> {
        let header_layout = Layout::new::<ArrayHeader>();
        let array_data_layout = Layout::array::<T>(self.length)?;

        let (array_layout, _) = header_layout.extend(array_data_layout)?;
        let offset = array_layout.size() - array_data_layout.size();

        let header_ptr = self as *mut ArrayHeader;
        let data_ptr = (header_ptr as usize + offset) as *mut T;

        Ok(unsafe { std::slice::from_raw_parts_mut(data_ptr, self.length) })
    }
}

pub struct CallFrame<'a, 'b> {
    class: &'a Class<'a>,
    method: &'a Method<'a>,
    locals: Vec<Local>,
    operand_stack: Vec<Operand<'a>>,
    stdout: &'b mut dyn io::Write,
    heap: &'a Bump,
}

impl<'a, 'b> CallFrame<'a, 'b> {
    pub fn new(
        class: &'a Class<'a>,
        method: &'a Method<'a>,
        args: impl Iterator<Item = Local>,
        stdout: &'b mut dyn io::Write,
        heap: &'a Bump,
    ) -> eyre::Result<CallFrame<'a, 'b>> {
        let body = method.body.as_ref().wrap_err("missing method body")?;

        let mut locals = vec![Local::None; body.locals];

        for (i, arg) in (0..method.descriptor.params.len()).zip(args) {
            locals[i] = arg;
        }

        Ok(CallFrame {
            class,
            method,
            locals,
            operand_stack: Vec::with_capacity(body.stack_size),
            stdout,
            heap,
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

        let mut pc = 0;

        loop {
            let instruction = &body.code[pc];
            let mut next_instruction_offset = 1isize;
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
                        arg => todo!("{arg:?}"),
                    };
                }
                Instruction::store {
                    data_type: LoadStoreType::Reference,
                    index,
                } => {
                    let operand = self
                        .operand_stack
                        .pop()
                        .wrap_err("no operand provided to istore")?;

                    self.locals[*index as usize] = match operand {
                        Operand::Reference(v) => Local::Reference(v),
                        Operand::ReturnAddress(v) => Local::ReturnAddress(v),
                        arg => unreachable!("unsupported operand for astore: {arg:?}"),
                    };
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
                }
                Instruction::load {
                    data_type: LoadStoreType::Reference,
                    index,
                } => {
                    let val = match self.locals[*index as usize] {
                        Local::None => todo!(),
                        Local::Reference(v) => Operand::Reference(v),
                        Local::ReturnAddress(v) => Operand::ReturnAddress(v),
                        local => bail!("aload called with invalid local: {local:?}"),
                    };

                    self.operand_stack.push(val);
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
                }
                Instruction::invoke { kind, index } => {
                    self.execute_invoke(*index, *kind)?;
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
                }
                Instruction::bipush { value } => {
                    self.operand_stack.push(Operand::Int(*value as i32));
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
                        next_instruction_offset = *branch as isize;
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
                        next_instruction_offset = *branch as isize;
                    }
                }
                Instruction::goto { branch } => {
                    next_instruction_offset = *branch as isize;
                }
                Instruction::inc { index, value } => {
                    *self.locals[*index as usize].try_as_int_mut().unwrap() += *value as i32;
                }
                Instruction::newarray { atype } => {
                    let length = self
                        .operand_stack
                        .pop()
                        .wrap_err("missing count operand for newarray")?
                        .try_as_int()
                        .wrap_err("expected int")? as usize;

                    let array_data_layout = match atype {
                        ArrayType::Int => Layout::array::<i32>(length)?,
                        atype => todo!("{atype:?}"),
                    };

                    let (array_layout, _) =
                        Layout::new::<ArrayHeader>().extend(array_data_layout)?;
                    let layout = array_layout.pad_to_align();
                    let ptr = self.heap.alloc_layout(layout);

                    unsafe {
                        std::ptr::write_bytes(ptr.as_ptr(), 0, layout.size());

                        *(ptr.as_ptr() as *mut ArrayHeader) = ArrayHeader {
                            atype: *atype,
                            length,
                        };
                    }

                    self.operand_stack
                        .push(Operand::Reference(ptr.as_ptr() as _));
                }
                Instruction::arraylength => {
                    let reference = self
                        .operand_stack
                        .pop()
                        .unwrap()
                        .try_as_reference()
                        .unwrap();

                    let header = unsafe { &*(reference as *mut ArrayHeader) };

                    self.operand_stack.push(Operand::Int(header.length as i32));
                }
                Instruction::arraystore { data_type } => {
                    let value = self.operand_stack.pop().unwrap();
                    let index = self.operand_stack.pop().unwrap().try_as_int().unwrap();
                    let ptr = self
                        .operand_stack
                        .pop()
                        .unwrap()
                        .try_as_reference()
                        .unwrap();

                    let header = unsafe { (ptr as *mut ArrayHeader).as_mut().unwrap() };

                    match header.atype {
                        ArrayType::Int => {
                            if *data_type != ArrayLoadStoreType::Int {
                                bail!("invalid array type: {:?}", header.atype);
                            }

                            unsafe {
                                header.data::<i32>()?[index as usize] = value.try_as_int().unwrap();
                            }
                        }
                        t => todo!("{t:?}"),
                    }
                }
                _ => todo!("unimplemented instruction: {instruction:?}"),
            }

            pc = pc
                .checked_add_signed(next_instruction_offset)
                .wrap_err("program counter overflowed")?;
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
                        Operand::Reference(ptr) => {
                            let header = unsafe { (ptr as *mut ArrayHeader).as_mut().unwrap() };
                            match header.atype {
                                ArrayType::Int => {
                                    write!(self.stdout, "{:?}", unsafe { header.data::<i32>()? })?;
                                }
                                t => todo!("{t:?}"),
                            }
                        }
                        arg => todo!("{arg:?}"),
                    }
                } else {
                    let args = method
                        .descriptor
                        .params
                        .iter()
                        .map(|_| self.operand_stack.pop().unwrap())
                        .map(|op| match op {
                            Operand::Int(v) => Local::Int(v),
                            op => todo!("{op:?}"),
                        });

                    if let Some(ret) =
                        CallFrame::new(self.class, method, args, self.stdout, self.heap)?
                            .execute()?
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

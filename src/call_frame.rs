use std::alloc::Layout;
use std::cell::UnsafeCell;
use std::mem;
use std::ptr::NonNull;
use std::time::SystemTime;

use color_eyre::eyre::{self, bail, eyre, ContextCompat};
use strum::EnumTryAs;

use crate::class::{Class, Method};
use crate::class_file::constant_pool::{self, ConstantInfo};
use crate::class_file::MethodAccessFlags;
use crate::descriptor::{BaseType, FieldType};
use crate::instructions::{
    ArrayLoadStoreType, ArrayType, Condition, Instruction, InvokeKind, LoadStoreType, NumberType,
    ReturnType,
};
use crate::vm::Vm;

#[derive(Clone, Debug, EnumTryAs)]
pub enum JvmValue<'a> {
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

const _: () = {
    assert!(mem::size_of::<Option<JvmValue>>() == 24);
};

#[derive(Debug)]
#[repr(C)]
enum RefTypeHeader {
    Object(ObjectHeader),
    Array(ArrayHeader),
}

#[derive(Debug)]
#[repr(C)]
struct ObjectHeader {
    class: NonNull<Class<'static>>,
}

#[derive(Debug)]
#[repr(C)]
struct ArrayHeader {
    atype: ArrayType,
    length: usize,
}

const _: () = {
    assert!(mem::size_of::<RefTypeHeader>() == 24);
};

impl RefTypeHeader {
    unsafe fn array_data<'a, T>(&mut self) -> eyre::Result<&'a mut [T]> {
        let length = match self {
            Self::Object(_) => bail!("expected an array"),
            Self::Array(header) => header.length,
        };

        let header_layout = Layout::new::<RefTypeHeader>();
        let array_data_layout = Layout::array::<T>(length)?;

        let (array_layout, _) = header_layout.extend(array_data_layout)?;
        let offset = array_layout.size() - array_data_layout.size();

        let header_ptr = self as *mut RefTypeHeader;
        let data_ptr = (header_ptr as usize + offset) as *mut T;

        Ok(unsafe { std::slice::from_raw_parts_mut(data_ptr, length) })
    }

    unsafe fn object_data<'a>(&mut self) -> eyre::Result<&'a mut [JvmValue]> {
        let target_class = match self {
            Self::Object(object) => object.class,
            Self::Array(_) => bail!("expected an object"),
        };

        let fields_layout = Layout::array::<JvmValue>((*target_class.as_ptr()).fields().len())?;
        let (object_layout, _) = Layout::new::<RefTypeHeader>().extend(fields_layout)?;

        let offset = object_layout.size() - fields_layout.size();

        let header_ptr = self as *mut RefTypeHeader;
        let data_ptr = (header_ptr as usize + offset) as *mut JvmValue;

        Ok(unsafe {
            std::slice::from_raw_parts_mut(data_ptr, (*target_class.as_ptr()).fields().len())
        })
    }
}

pub struct CallFrame<'a, 'b> {
    class: &'a Class<'a>,
    method: &'a Method<'a>,
    locals: Vec<Option<JvmValue<'a>>>,
    operand_stack: Vec<JvmValue<'a>>,
    vm: &'b mut Vm<'a>,
}

impl<'a, 'b> CallFrame<'a, 'b> {
    pub fn new(
        class: &'a Class<'a>,
        method: &'a Method<'a>,
        args: impl Iterator<Item = JvmValue<'a>>,
        vm: &'b mut Vm<'a>,
    ) -> eyre::Result<CallFrame<'a, 'b>> {
        let body = method.body.as_ref().wrap_err("missing method body")?;

        let mut locals = vec![None; body.locals];

        for (i, arg) in args.enumerate() {
            locals[i] = Some(arg);
        }

        Ok(CallFrame {
            class,
            method,
            locals,
            operand_stack: Vec::with_capacity(body.stack_size),
            vm,
        })
    }

    pub fn execute(mut self) -> eyre::Result<Option<JvmValue<'a>>> {
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
                        NumberType::Int => JvmValue::Int(*value as i32),
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

                    self.locals[*index as usize] = Some(match operand {
                        JvmValue::Byte(v) => JvmValue::Byte(v),
                        JvmValue::StringConst(_) => todo!(),
                        JvmValue::Int(v) => JvmValue::Int(v),
                        arg => todo!("{arg:?}"),
                    });
                }
                Instruction::store {
                    data_type: LoadStoreType::Reference,
                    index,
                } => {
                    let operand = self
                        .operand_stack
                        .pop()
                        .wrap_err("no operand provided to istore")?;

                    self.locals[*index as usize] = Some(match operand {
                        JvmValue::Reference(v) => JvmValue::Reference(v),
                        JvmValue::ReturnAddress(v) => JvmValue::ReturnAddress(v),
                        arg => unreachable!("unsupported operand for astore: {arg:?}"),
                    });
                }
                Instruction::load {
                    data_type: LoadStoreType::Int,
                    index,
                } => {
                    let val = match &self.locals[*index as usize] {
                        None => 0,
                        Some(JvmValue::Int(v)) => *v,
                        Some(JvmValue::Byte(v)) => *v as i32,
                        local => bail!("iload called with invalid local: {local:?}"),
                    };

                    self.operand_stack.push(JvmValue::Int(val));
                }
                Instruction::load {
                    data_type: LoadStoreType::Reference,
                    index,
                } => {
                    let val = match &self.locals[*index as usize] {
                        None => JvmValue::Reference(0),
                        Some(JvmValue::Reference(v)) => JvmValue::Reference(*v),
                        Some(JvmValue::ReturnAddress(v)) => JvmValue::ReturnAddress(*v),
                        Some(JvmValue::StringConst(v)) => JvmValue::StringConst(*v),
                        local => bail!("aload called with invalid local: {local:?}"),
                    };

                    self.operand_stack.push(val);
                }
                Instruction::ldc { index } => {
                    match &self.class.constant_pool()[*index] {
                        ConstantInfo::String(constant_pool::String { string_index }) => {
                            self.operand_stack.push(JvmValue::StringConst(
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
                        NumberType::Int => self.operand_stack.push(JvmValue::Int(
                            a.try_as_int().wrap_err("invalid type")?
                                + b.try_as_int().wrap_err("invalid type")?,
                        )),
                        NumberType::Long => todo!(),
                        NumberType::Float => todo!(),
                        NumberType::Double => todo!(),
                    }
                }
                Instruction::bipush { value } => {
                    self.operand_stack.push(JvmValue::Int(*value as i32));
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
                            JvmValue::Int(v1 % v2)
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
                    *self.locals[*index as usize]
                        .as_mut()
                        .unwrap()
                        .try_as_int_mut()
                        .unwrap() += *value as i32;
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
                        Layout::new::<RefTypeHeader>().extend(array_data_layout)?;
                    let layout = array_layout.pad_to_align();
                    let ptr = self.vm.heap.alloc_layout(layout);

                    unsafe {
                        std::ptr::write_bytes(ptr.as_ptr(), 0, layout.size());

                        *(ptr.as_ptr() as *mut RefTypeHeader) = RefTypeHeader::Array(ArrayHeader {
                            atype: *atype,
                            length,
                        });
                    }

                    self.operand_stack
                        .push(JvmValue::Reference(ptr.as_ptr() as _));
                }
                Instruction::arraylength => {
                    let reference = self
                        .operand_stack
                        .pop()
                        .unwrap()
                        .try_as_reference()
                        .unwrap();

                    let header = unsafe { &*(reference as *mut RefTypeHeader) };
                    let RefTypeHeader::Array(array) = header else {
                        bail!("invalid header: {header:?}")
                    };

                    self.operand_stack.push(JvmValue::Int(array.length as i32));
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

                    let header = unsafe { (ptr as *mut RefTypeHeader).as_mut().unwrap() };
                    let RefTypeHeader::Array(array) = header else {
                        bail!("invalid header: {header:?}")
                    };

                    match array.atype {
                        ArrayType::Int => {
                            if *data_type != ArrayLoadStoreType::Int {
                                bail!("invalid array type: {:?}", array.atype);
                            }

                            unsafe {
                                header.array_data::<i32>()?[index as usize] =
                                    value.try_as_int().unwrap();
                            }
                        }
                        t => todo!("{t:?}"),
                    }
                }
                Instruction::putstatic { index } => unsafe {
                    // This *should* be safe as long as no other references to the field value exist
                    *self.get_static_field(*index)?.get() = self.operand_stack.pop().unwrap()
                },
                Instruction::getstatic { index } => unsafe {
                    let value = self.get_static_field(*index)?;
                    self.operand_stack.push((*value.get()).clone());
                },
                Instruction::aconst_null => {
                    self.operand_stack.push(JvmValue::Reference(0));
                }
                Instruction::new { index } => {
                    let target_class = self.class.constant_pool()[*index]
                        .try_as_class_ref()
                        .wrap_err("expected class")?;

                    let target_class_name = self.class.constant_pool()[target_class.name_index]
                        .try_as_utf_8_ref()
                        .wrap_err("expected utf8")?;

                    let target_class = self.vm.load_class_file(target_class_name)?;

                    let fields_layout = Layout::array::<JvmValue>(target_class.fields().len())?;
                    let (object_layout, _) =
                        Layout::new::<RefTypeHeader>().extend(fields_layout)?;

                    let layout = object_layout.pad_to_align();
                    let ptr = self.vm.heap.alloc_layout(layout);

                    unsafe {
                        ptr.as_ptr()
                            .cast::<RefTypeHeader>()
                            .write(RefTypeHeader::Object(ObjectHeader {
                                class: mem::transmute(target_class),
                            }));

                        let fields = ptr
                            .as_ptr()
                            .add(object_layout.size() - fields_layout.size())
                            .cast::<JvmValue>();

                        for (i, field) in target_class.fields().iter().enumerate() {
                            fields.add(i).write(match &field.descriptor.field_type {
                                FieldType::Base(t) => match t {
                                    BaseType::Byte => todo!(),
                                    BaseType::Char => todo!(),
                                    BaseType::Double => todo!(),
                                    BaseType::Float => todo!(),
                                    BaseType::Int => JvmValue::Int(0),
                                    BaseType::Long => todo!(),
                                    BaseType::Short => todo!(),
                                    BaseType::Boolean => JvmValue::Boolean(false),
                                    BaseType::Object(_) => JvmValue::Reference(0),
                                },
                                FieldType::Array(_, _) => JvmValue::Reference(0),
                            });
                        }
                    }

                    self.operand_stack
                        .push(JvmValue::Reference(ptr.as_ptr() as usize));
                }
                Instruction::putfield { index } => {
                    let value = self.operand_stack.pop().unwrap();
                    *self.get_instance_field(*index)? = value;
                }
                Instruction::getfield { index } => {
                    let value = self.get_instance_field(*index)?;
                    self.operand_stack.push((*value).clone());
                }
                Instruction::dup => {
                    self.operand_stack.push(
                        self.operand_stack
                            .last()
                            .wrap_err("operand stack is empty")?
                            .clone(),
                    );
                }
                _ => todo!("unimplemented instruction: {instruction:?}"),
            }

            pc = pc
                .checked_add_signed(next_instruction_offset)
                .wrap_err("program counter overflowed")?;
        }
    }

    fn get_static_field(&mut self, index: u16) -> eyre::Result<&'a UnsafeCell<JvmValue<'a>>> {
        let field_ref = self.class.constant_pool()[index]
            .try_as_field_ref_ref()
            .unwrap();

        let name_and_type = self.class.constant_pool()[field_ref.name_and_type_index]
            .try_as_name_and_type_ref()
            .wrap_err("expected name_and_type")?;

        let name = self.class.constant_pool()[name_and_type.name_index]
            .try_as_utf_8_ref()
            .wrap_err("expected utf8")?;

        let descriptor = self.class.constant_pool()[name_and_type.descriptor_index]
            .try_as_utf_8_ref()
            .wrap_err("expected utf8")?;

        let target_class = if field_ref.class_index == self.class.index() {
            self.class
        } else {
            let target_class = self.class.constant_pool()[field_ref.class_index]
                .try_as_class_ref()
                .wrap_err("expected class")?;

            let target_class_name = self.class.constant_pool()[target_class.name_index]
                .try_as_utf_8_ref()
                .wrap_err("expected utf8")?;

            self.vm.load_class_file(target_class_name)?
        };

        target_class
            .static_field(name, descriptor)
            .wrap_err_with(|| {
                let class_name = target_class.name();
                eyre!("field {name}({descriptor}) does not exist on {class_name}")
            })
    }

    fn get_instance_field(&mut self, index: u16) -> eyre::Result<&'b mut JvmValue<'a>> {
        let field_ref = self.class.constant_pool()[index]
            .try_as_field_ref_ref()
            .wrap_err_with(|| eyre!("unexpected: {:?}", self.class.constant_pool()[index]))?;

        let name_and_type = self.class.constant_pool()[field_ref.name_and_type_index]
            .try_as_name_and_type_ref()
            .wrap_err("expected name_and_type")?;

        let name = self.class.constant_pool()[name_and_type.name_index]
            .try_as_utf_8_ref()
            .wrap_err("expected utf8")?;

        let descriptor = self.class.constant_pool()[name_and_type.descriptor_index]
            .try_as_utf_8_ref()
            .wrap_err("expected utf8")?;

        let target_class = if field_ref.class_index == self.class.index() {
            self.class
        } else {
            let target_class = self.class.constant_pool()[field_ref.class_index]
                .try_as_class_ref()
                .wrap_err("expected class")?;

            let target_class_name = self.class.constant_pool()[target_class.name_index]
                .try_as_utf_8_ref()
                .wrap_err("expected utf8")?;

            self.vm.load_class_file(target_class_name)?
        };

        let objectref = self
            .operand_stack
            .pop()
            .unwrap()
            .try_as_reference()
            .unwrap();

        let field_index = target_class.field_ordinal(name, descriptor).unwrap();

        let data = unsafe {
            std::slice::from_raw_parts_mut(
                (objectref as *mut u8).add(24).cast::<JvmValue>(),
                target_class.fields().len(),
            )
        };

        Ok(&mut data[field_index])
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

        let mut target_class = if method_ref.class_index == self.class.index() {
            self.class
        } else {
            let target_class = self.class.constant_pool()[method_ref.class_index]
                .try_as_class_ref()
                .wrap_err("expected class")?;

            let target_class_name = self.class.constant_pool()[target_class.name_index]
                .try_as_utf_8_ref()
                .wrap_err("expected utf8")?;

            self.vm.load_class_file(target_class_name)?
        };

        // TODO: Do we need to ignore super class for static methods?
        let method = loop {
            let method = target_class.method(name, descriptor);
            if let Some(method) = method {
                break method;
            }

            target_class = target_class
                .super_class()
                .wrap_err_with(|| eyre!("method not found: {name}{descriptor}"))?;
        };

        match kind {
            InvokeKind::Static => {
                if method.access_flags.contains(MethodAccessFlags::NATIVE) {
                    match name.as_str() {
                        "registerNatives" => {
                            // TODO
                        }
                        "print" => {
                            let arg = self
                                .operand_stack
                                .pop()
                                .wrap_err("missing argument to print")?;

                            self.print_jvm_value(&arg)?;
                        }
                        "currentTimeMillis" => self.operand_stack.push(JvmValue::Long(
                            self.vm
                                .time
                                .system_time()
                                .duration_since(SystemTime::UNIX_EPOCH)?
                                .as_millis()
                                .try_into()?,
                        )),
                        _ => unimplemented!("{name}{descriptor}"),
                    }
                } else {
                    let args = method
                        .descriptor
                        .params
                        .iter()
                        .map(|_| self.operand_stack.pop().unwrap())
                        .map(|op| match op {
                            JvmValue::Int(v) => JvmValue::Int(v),
                            op => todo!("{op:?}"),
                        });

                    if let Some(ret) =
                        CallFrame::new(self.class, method, args, self.vm)?.execute()?
                    {
                        self.operand_stack.push(ret);
                    }
                }
            }
            InvokeKind::Special => {
                let nargs = method.descriptor.params.len() + 1; // args + objectref
                let args_start = self.operand_stack.len() - nargs;

                let args = &self.operand_stack[args_start..];
                let args = args.iter().cloned();

                let ret_value = CallFrame::new(target_class, method, args, self.vm)?.execute()?;

                self.operand_stack
                    .truncate(self.operand_stack.len() - nargs);

                if let Some(ret) = ret_value {
                    self.operand_stack.push(ret);
                }
            }
            InvokeKind::Virtual => {
                // TODO: Handle signature polymorphic methods (https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.invokevirtual)

                let nargs = method.descriptor.params.len() + 1; // args + objectref
                let args_start = self.operand_stack.len() - nargs;

                let args = &self.operand_stack[args_start..];

                // TODO: Resolve interface methods

                let (selected_class, selected_method) = if method
                    .access_flags
                    .contains(MethodAccessFlags::PRIVATE)
                {
                    (target_class, method)
                } else {
                    let objectref = args[0].try_as_reference_ref().copied().unwrap();
                    let header = objectref as *mut RefTypeHeader;

                    let mut object_class: &'a Class<'a> = unsafe {
                        match header.as_ref().unwrap() {
                            RefTypeHeader::Object(header) => mem::transmute(header.class.as_ref()),
                            RefTypeHeader::Array(_) => todo!(),
                        }
                    };

                    loop {
                        let method = object_class.method(name, descriptor);
                        if let Some(method) = method {
                            break (object_class, method);
                        }

                        object_class = object_class
                            .super_class()
                            .wrap_err_with(|| eyre!("method not found: {name}{descriptor}"))?;
                    }
                };

                let args = args.iter().cloned();

                let ret_value =
                    CallFrame::new(selected_class, selected_method, args, self.vm)?.execute()?;

                self.operand_stack
                    .truncate(self.operand_stack.len() - nargs);

                if let Some(ret) = ret_value {
                    self.operand_stack.push(ret);
                }
            }
            _ => {
                todo!("{}::{name}({descriptor}) ({kind:?})", target_class.name())
            }
        }

        Ok(())
    }

    fn print_jvm_value(&mut self, value: &JvmValue) -> eyre::Result<()> {
        match value {
            JvmValue::StringConst(v) => write!(self.vm.stdout, "{v}")?,
            JvmValue::Byte(v) => write!(self.vm.stdout, "{v}")?,
            JvmValue::Int(v) => write!(self.vm.stdout, "{v}")?,
            JvmValue::Long(v) => write!(self.vm.stdout, "{v}")?,
            JvmValue::Reference(ptr) => {
                let header = unsafe { (*ptr as *mut RefTypeHeader).as_mut() };

                match header {
                    None => {
                        write!(self.vm.stdout, "null")?;
                    }
                    Some(header) => match header {
                        RefTypeHeader::Array(array) => match array.atype {
                            ArrayType::Int => {
                                let elements = unsafe { header.array_data::<i32>()? };
                                write!(self.vm.stdout, "{elements:?}")?
                            }
                            t => todo!("{t:?}"),
                        },
                        RefTypeHeader::Object(object) => {
                            let class = unsafe { object.class.as_ref() };
                            let fields = unsafe { header.object_data() }?;

                            write!(self.vm.stdout, "{} {{", class.name())?;

                            for (i, field) in class.fields().iter().enumerate() {
                                let name = field.name;
                                let value = &fields[i];

                                write!(self.vm.stdout, "{name}: ")?;

                                self.print_jvm_value(value)?;

                                if i < fields.len() - 1 {
                                    write!(self.vm.stdout, ", ")?;
                                }
                            }

                            write!(self.vm.stdout, "}}")?;
                        }
                    },
                };
            }
            arg => todo!("{arg:?}"),
        }

        Ok(())
    }
}

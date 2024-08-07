use std::cell::UnsafeCell;
use std::fmt::Debug;
use std::io::{self, Cursor};
use std::num::NonZeroU8;

use bumpalo::collections::Vec;
use bumpalo::{vec, Bump};
use byteorder::{BigEndian, ReadBytesExt};
use color_eyre::eyre::{self, bail, eyre, Context, ContextCompat};
use hashbrown::HashMap;

use crate::call_frame::JvmValue;
use crate::class_file::constant_pool::ConstantPool;
use crate::class_file::{ClassFile, FieldAccessFlags, MethodAccessFlags};
use crate::descriptor::{
    parse_field_descriptor, parse_method_descriptor, BaseType, FieldDescriptor, FieldType,
    MethodDescriptor,
};
use crate::instructions::{
    ArrayLoadStoreType, ArrayType, Condition, EqCondition, Instruction, IntegerType, InvokeKind,
    NumberType, OrdCondition, ReturnType,
};
use crate::opcodes::OpCode;

#[derive(Debug)]
pub struct Class<'a> {
    name: &'a str,
    class_file: &'a ClassFile<'a>,
    super_class: Option<&'a Class<'a>>,
    methods: HashMap<MethodId<'a>, Method<'a>>,
    static_fields: HashMap<(&'a str, &'a str), UnsafeCell<JvmValue<'a>>>,
    fields: std::vec::Vec<Field<'a>>,
    field_ordinals: HashMap<(&'a str, &'a str), usize>,
}

#[derive(Debug)]
pub struct Method<'a> {
    pub descriptor: MethodDescriptor<'a>,
    pub access_flags: MethodAccessFlags,
    pub body: Option<MethodBody<'a>>,
}

#[derive(Debug)]
pub struct MethodBody<'a> {
    pub locals: usize,
    pub stack_size: usize,
    pub code: Vec<'a, Instruction>,
}

#[derive(Clone, Debug)]
pub struct Field<'a> {
    pub name: &'a str,
    pub descriptor: FieldDescriptor<'a>,
    pub access_flags: FieldAccessFlags,
}

impl<'a> Class<'a> {
    pub fn new(
        arena: &'a Bump,
        class_file: &'a ClassFile,
        class_loader: &mut dyn FnMut(&str) -> eyre::Result<&'a Class<'a>>,
    ) -> eyre::Result<Class<'a>> {
        let this_class = class_file.constant_pool[class_file.this_class]
            .try_as_class_ref()
            .unwrap();

        let super_class = if class_file.super_class == 0 {
            None
        } else {
            class_file.constant_pool[class_file.super_class]
                .try_as_class_ref()
                .map(|class| {
                    let name = class_file.constant_pool[class.name_index]
                        .try_as_utf_8_ref()
                        .unwrap();
                    class_loader(name)
                })
                .transpose()?
        };

        let name = class_file.constant_pool[this_class.name_index]
            .try_as_utf_8_ref()
            .unwrap();

        let mut fields = std::vec![];
        let mut field_ordinals = HashMap::new();

        // If the class has a super class, we copy its fields into the child class.
        if let Some(super_class) = super_class {
            fields.extend(super_class.fields.iter().cloned());
            field_ordinals.extend(super_class.field_ordinals.iter());
        }

        for field in &class_file.fields {
            if field.access_flags.contains(FieldAccessFlags::STATIC) {
                continue;
            }

            let name = class_file.constant_pool[field.name_index]
                .try_as_utf_8_ref()
                .unwrap();

            let descriptor_str = class_file.constant_pool[field.descriptor_index]
                .try_as_utf_8_ref()
                .unwrap();

            let descriptor = parse_field_descriptor(descriptor_str)?;

            fields.push(Field {
                name,
                descriptor,
                access_flags: field.access_flags.clone(),
            });

            field_ordinals.insert(
                (name.as_str(), descriptor_str.as_str()),
                field_ordinals.len(),
            );
        }

        Ok(Class {
            name,
            class_file,
            super_class,
            methods: {
                let mut methods = HashMap::new();
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
                            descriptor: parse_method_descriptor(descriptor).wrap_err_with(
                                || eyre!("invalid method descriptor: {descriptor}"),
                            )?,
                            access_flags: method.access_flags,
                            body: method
                                .attributes
                                .iter()
                                .find_map(|attr| attr.try_as_code_ref())
                                .map(|attr| -> eyre::Result<MethodBody> {
                                    Ok(MethodBody {
                                        locals: attr.max_locals as usize,
                                        stack_size: attr.max_stack as usize,
                                        code: decode_instructions(arena, attr.code.as_slice())?,
                                    })
                                })
                                .transpose()?,
                        },
                    );
                }
                methods
            },
            static_fields: class_file
                .fields
                .iter()
                .filter(|field| field.access_flags.contains(FieldAccessFlags::STATIC))
                .map(|field| {
                    let name = class_file.constant_pool[field.name_index]
                        .try_as_utf_8_ref()
                        .unwrap();

                    let descriptor_str = class_file.constant_pool[field.descriptor_index]
                        .try_as_utf_8_ref()
                        .unwrap();

                    let descriptor = parse_field_descriptor(descriptor_str)?;

                    let value = UnsafeCell::new(match descriptor.field_type {
                        FieldType::Base(t) => match t {
                            BaseType::Byte => JvmValue::Byte(0),
                            BaseType::Char => JvmValue::Char(0),
                            BaseType::Double => JvmValue::Double(0.0),
                            BaseType::Float => JvmValue::Float(0.0),
                            BaseType::Int => JvmValue::Int(0),
                            BaseType::Long => JvmValue::Long(0),
                            BaseType::Short => JvmValue::Short(0),
                            BaseType::Boolean => JvmValue::Boolean(false),
                            BaseType::Object(_) => JvmValue::Reference(0),
                        },
                        FieldType::Array(_, _) => JvmValue::Reference(0),
                    });

                    Ok(((name.as_str(), descriptor_str.as_str()), value))
                })
                .collect::<eyre::Result<_>>()?,
            fields,
            field_ordinals,
        })
    }

    pub fn index(&self) -> u16 {
        self.class_file.this_class
    }

    pub fn name(&self) -> &'a str {
        self.name
    }

    pub fn super_class(&self) -> Option<&'a Class<'a>> {
        self.super_class
    }

    pub fn method<'b: 'a>(&'a self, name: &'b str, descriptor: &'b str) -> Option<&'a Method<'a>> {
        self.methods.get(&MethodId { name, descriptor })
    }

    pub fn constant_pool(&self) -> &'a ConstantPool {
        &self.class_file.constant_pool
    }

    pub fn static_field(
        &self,
        name: &'a str,
        descriptor: &'a str,
    ) -> Option<&UnsafeCell<JvmValue<'a>>> {
        self.static_fields.get(&(name, descriptor))
    }

    pub fn fields(&self) -> &[Field<'a>] {
        &self.fields
    }

    pub fn field_ordinal(&self, name: &'a str, descriptor: &'a str) -> Option<usize> {
        self.field_ordinals.get(&(name, descriptor)).copied()
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

pub fn decode_instructions<'a>(
    arena: &'a Bump,
    bytes: &[u8],
) -> eyre::Result<Vec<'a, Instruction>> {
    let mut instructions = vec![in arena];
    let mut cursor = Cursor::new(&bytes);

    let mut address_map = std::vec![];
    let mut index_map = std::vec![0; bytes.len()];
    let mut i = 0;

    while let Ok(opcode) = cursor.read_u8() {
        address_map.push(cursor.position() as usize - 1);
        index_map[cursor.position() as usize - 1] = i;
        i += 1;

        let opcode =
            OpCode::from_repr(opcode).wrap_err_with(|| eyre!("unknown opcode: {opcode}"))?;

        let instruction = match opcode {
            OpCode::nop => Instruction::nop,
            OpCode::aconst_null => Instruction::aconst_null,
            OpCode::iconst_m1 => Instruction::iconst(-1),
            OpCode::iconst_0 => Instruction::iconst(0),
            OpCode::iconst_1 => Instruction::iconst(1),
            OpCode::iconst_2 => Instruction::iconst(2),
            OpCode::iconst_3 => Instruction::iconst(3),
            OpCode::iconst_4 => Instruction::iconst(4),
            OpCode::iconst_5 => Instruction::iconst(5),
            OpCode::lconst_0 => Instruction::lconst(0),
            OpCode::lconst_1 => Instruction::lconst(1),
            OpCode::fconst_0 => Instruction::fconst(0),
            OpCode::fconst_1 => Instruction::fconst(1),
            OpCode::fconst_2 => Instruction::fconst(2),
            OpCode::dconst_0 => Instruction::dconst(0),
            OpCode::dconst_1 => Instruction::dconst(1),
            OpCode::bipush => Instruction::bipush(cursor.read_i8()?),
            OpCode::sipush => Instruction::sipush(cursor.read_i16_be()?),
            OpCode::ldc => Instruction::ldc(cursor.read_u8()? as u16),
            OpCode::ldc_w => Instruction::ldc(cursor.read_u16_be()?),
            OpCode::ldc2_w => Instruction::ldc2(cursor.read_u16_be()?),
            OpCode::iload => Instruction::iload(cursor.read_u8()?),
            OpCode::lload => Instruction::lload(cursor.read_u8()?),
            OpCode::fload => Instruction::fload(cursor.read_u8()?),
            OpCode::dload => Instruction::dload(cursor.read_u8()?),
            OpCode::aload => Instruction::aload(cursor.read_u8()?),
            OpCode::iload_0 => Instruction::iload(0),
            OpCode::iload_1 => Instruction::iload(1),
            OpCode::iload_2 => Instruction::iload(2),
            OpCode::iload_3 => Instruction::iload(3),
            OpCode::lload_0 => Instruction::lload(0),
            OpCode::lload_1 => Instruction::lload(1),
            OpCode::lload_2 => Instruction::lload(2),
            OpCode::lload_3 => Instruction::lload(3),
            OpCode::fload_0 => Instruction::fload(0),
            OpCode::fload_1 => Instruction::fload(1),
            OpCode::fload_2 => Instruction::fload(2),
            OpCode::fload_3 => Instruction::fload(3),
            OpCode::dload_0 => Instruction::dload(0),
            OpCode::dload_1 => Instruction::dload(1),
            OpCode::dload_2 => Instruction::dload(2),
            OpCode::dload_3 => Instruction::dload(3),
            OpCode::aload_0 => Instruction::aload(0),
            OpCode::aload_1 => Instruction::aload(1),
            OpCode::aload_2 => Instruction::aload(2),
            OpCode::aload_3 => Instruction::aload(3),
            OpCode::iaload => Instruction::arraystore(ArrayLoadStoreType::Int),
            OpCode::laload => Instruction::arraystore(ArrayLoadStoreType::Long),
            OpCode::faload => Instruction::arraystore(ArrayLoadStoreType::Float),
            OpCode::daload => Instruction::arraystore(ArrayLoadStoreType::Double),
            OpCode::aaload => Instruction::arraystore(ArrayLoadStoreType::Reference),
            OpCode::baload => Instruction::arraystore(ArrayLoadStoreType::Byte),
            OpCode::caload => Instruction::arraystore(ArrayLoadStoreType::Char),
            OpCode::saload => Instruction::arraystore(ArrayLoadStoreType::Short),
            OpCode::istore => Instruction::istore(cursor.read_u8()?),
            OpCode::lstore => Instruction::lstore(cursor.read_u8()?),
            OpCode::fstore => Instruction::fstore(cursor.read_u8()?),
            OpCode::dstore => Instruction::dstore(cursor.read_u8()?),
            OpCode::astore => Instruction::astore(cursor.read_u8()?),
            OpCode::istore_0 => Instruction::istore(0),
            OpCode::istore_1 => Instruction::istore(1),
            OpCode::istore_2 => Instruction::istore(2),
            OpCode::istore_3 => Instruction::istore(3),
            OpCode::lstore_0 => Instruction::lstore(0),
            OpCode::lstore_1 => Instruction::lstore(1),
            OpCode::lstore_2 => Instruction::lstore(2),
            OpCode::lstore_3 => Instruction::lstore(3),
            OpCode::fstore_0 => Instruction::fstore(0),
            OpCode::fstore_1 => Instruction::fstore(1),
            OpCode::fstore_2 => Instruction::fstore(2),
            OpCode::fstore_3 => Instruction::fstore(3),
            OpCode::dstore_0 => Instruction::dstore(0),
            OpCode::dstore_1 => Instruction::dstore(1),
            OpCode::dstore_2 => Instruction::dstore(2),
            OpCode::dstore_3 => Instruction::dstore(3),
            OpCode::astore_0 => Instruction::astore(0),
            OpCode::astore_1 => Instruction::astore(1),
            OpCode::astore_2 => Instruction::astore(2),
            OpCode::astore_3 => Instruction::astore(3),
            OpCode::iastore => Instruction::arraystore(ArrayLoadStoreType::Int),
            OpCode::lastore => Instruction::arraystore(ArrayLoadStoreType::Long),
            OpCode::fastore => Instruction::arraystore(ArrayLoadStoreType::Float),
            OpCode::dastore => Instruction::arraystore(ArrayLoadStoreType::Double),
            OpCode::aastore => Instruction::arraystore(ArrayLoadStoreType::Reference),
            OpCode::bastore => Instruction::arraystore(ArrayLoadStoreType::Byte),
            OpCode::castore => Instruction::arraystore(ArrayLoadStoreType::Char),
            OpCode::sastore => Instruction::arraystore(ArrayLoadStoreType::Short),
            OpCode::pop => Instruction::pop,
            OpCode::pop2 => Instruction::pop2,
            OpCode::dup => Instruction::dup,
            OpCode::dup_x1 => Instruction::dup_x1,
            OpCode::dup_x2 => Instruction::dup_x2,
            OpCode::dup2 => Instruction::dup2,
            OpCode::dup2_x1 => Instruction::dup2_x1,
            OpCode::dup2_x2 => Instruction::dup2_x2,
            OpCode::swap => Instruction::swap,
            OpCode::iadd => Instruction::add(NumberType::Int),
            OpCode::ladd => Instruction::add(NumberType::Long),
            OpCode::fadd => Instruction::add(NumberType::Float),
            OpCode::dadd => Instruction::add(NumberType::Double),
            OpCode::isub => Instruction::sub(NumberType::Int),
            OpCode::lsub => Instruction::sub(NumberType::Long),
            OpCode::fsub => Instruction::sub(NumberType::Float),
            OpCode::dsub => Instruction::sub(NumberType::Double),
            OpCode::imul => Instruction::mul(NumberType::Int),
            OpCode::lmul => Instruction::mul(NumberType::Long),
            OpCode::fmul => Instruction::mul(NumberType::Float),
            OpCode::dmul => Instruction::mul(NumberType::Double),
            OpCode::idiv => Instruction::div(NumberType::Int),
            OpCode::ldiv => Instruction::div(NumberType::Long),
            OpCode::fdiv => Instruction::div(NumberType::Float),
            OpCode::ddiv => Instruction::div(NumberType::Double),
            OpCode::irem => Instruction::rem(NumberType::Int),
            OpCode::lrem => Instruction::rem(NumberType::Long),
            OpCode::frem => Instruction::rem(NumberType::Float),
            OpCode::drem => Instruction::rem(NumberType::Double),
            OpCode::ineg => Instruction::neg(NumberType::Int),
            OpCode::lneg => Instruction::neg(NumberType::Long),
            OpCode::fneg => Instruction::neg(NumberType::Float),
            OpCode::dneg => Instruction::neg(NumberType::Double),
            OpCode::ishl => Instruction::shl(IntegerType::Int),
            OpCode::lshl => Instruction::shl(IntegerType::Long),
            OpCode::ishr => Instruction::shr(IntegerType::Int),
            OpCode::lshr => Instruction::shr(IntegerType::Long),
            OpCode::iushr => Instruction::ushr(IntegerType::Int),
            OpCode::lushr => Instruction::ushr(IntegerType::Long),
            OpCode::iand => Instruction::and(IntegerType::Int),
            OpCode::land => Instruction::and(IntegerType::Long),
            OpCode::ior => Instruction::or(IntegerType::Int),
            OpCode::lor => Instruction::or(IntegerType::Long),
            OpCode::ixor => Instruction::xor(IntegerType::Int),
            OpCode::lxor => Instruction::xor(IntegerType::Long),
            OpCode::iinc => Instruction::inc(cursor.read_u8()?, cursor.read_i8()?),
            OpCode::i2l => Instruction::i2l,
            OpCode::i2f => Instruction::i2f,
            OpCode::i2d => Instruction::i2d,
            OpCode::l2i => Instruction::l2i,
            OpCode::l2f => Instruction::l2f,
            OpCode::l2d => Instruction::l2d,
            OpCode::f2i => Instruction::f2i,
            OpCode::f2l => Instruction::f2l,
            OpCode::f2d => Instruction::f2d,
            OpCode::d2i => Instruction::d2i,
            OpCode::d2l => Instruction::d2l,
            OpCode::d2f => Instruction::d2f,
            OpCode::i2b => Instruction::i2b,
            OpCode::i2c => Instruction::i2c,
            OpCode::i2s => Instruction::i2s,
            OpCode::lcmp => Instruction::lcmp,
            OpCode::fcmpl => Instruction::fcmp(OrdCondition::Lt),
            OpCode::fcmpg => Instruction::fcmp(OrdCondition::Gt),
            OpCode::dcmpl => Instruction::dcmp(OrdCondition::Lt),
            OpCode::dcmpg => Instruction::dcmp(OrdCondition::Gt),
            OpCode::ifeq => Instruction::r#if(Condition::Eq, cursor.read_i16_be()?),
            OpCode::ifne => Instruction::r#if(Condition::Ne, cursor.read_i16_be()?),
            OpCode::iflt => Instruction::r#if(Condition::Lt, cursor.read_i16_be()?),
            OpCode::ifge => Instruction::r#if(Condition::Ge, cursor.read_i16_be()?),
            OpCode::ifgt => Instruction::r#if(Condition::Gt, cursor.read_i16_be()?),
            OpCode::ifle => Instruction::r#if(Condition::Le, cursor.read_i16_be()?),
            OpCode::if_icmpeq => Instruction::if_icmp(Condition::Eq, cursor.read_i16_be()?),
            OpCode::if_icmpne => Instruction::if_icmp(Condition::Ne, cursor.read_i16_be()?),
            OpCode::if_icmplt => Instruction::if_icmp(Condition::Lt, cursor.read_i16_be()?),
            OpCode::if_icmpge => Instruction::if_icmp(Condition::Ge, cursor.read_i16_be()?),
            OpCode::if_icmpgt => Instruction::if_icmp(Condition::Gt, cursor.read_i16_be()?),
            OpCode::if_icmple => Instruction::if_icmp(Condition::Le, cursor.read_i16_be()?),
            OpCode::if_acmpeq => Instruction::if_acmp(EqCondition::Eq, cursor.read_i16_be()?),
            OpCode::if_acmpne => Instruction::if_acmp(EqCondition::Ne, cursor.read_i16_be()?),
            OpCode::goto => Instruction::goto(cursor.read_i16_be()? as i32),
            OpCode::jsr => Instruction::jsr(cursor.read_i16_be()? as i32),
            OpCode::ret => Instruction::ret(cursor.read_u8()?),
            OpCode::tableswitch => {
                cursor.align_to(4);
                let _default = cursor.read_i32_be()?;
                let low = cursor.read_i32_be()?;
                let high = cursor.read_i32_be()?;
                let count = high - low + 1;
                cursor.set_position(cursor.position() + count as u64 * 4);
                Instruction::tableswitch {}
            }
            OpCode::lookupswitch => {
                cursor.align_to(4);
                let _default = cursor.read_i32_be()?;
                let npairs = cursor.read_i32_be()?;
                cursor.set_position(cursor.position() + npairs as u64 * 8);
                Instruction::lookupswitch {}
            }
            OpCode::ireturn => Instruction::r#return(ReturnType::Int),
            OpCode::lreturn => Instruction::r#return(ReturnType::Long),
            OpCode::freturn => Instruction::r#return(ReturnType::Float),
            OpCode::dreturn => Instruction::r#return(ReturnType::Double),
            OpCode::areturn => Instruction::r#return(ReturnType::Reference),
            OpCode::r#return => Instruction::r#return(ReturnType::Void),
            OpCode::getfield => Instruction::getfield(cursor.read_u16_be()?),
            OpCode::putfield => Instruction::putfield(cursor.read_u16_be()?),
            OpCode::getstatic => Instruction::getstatic(cursor.read_u16_be()?),
            OpCode::putstatic => Instruction::putstatic(cursor.read_u16_be()?),
            OpCode::invokevirtual => {
                Instruction::invoke(InvokeKind::Virtual, cursor.read_u16_be()?)
            }
            OpCode::invokespecial => {
                Instruction::invoke(InvokeKind::Special, cursor.read_u16_be()?)
            }
            OpCode::invokestatic => Instruction::invoke(InvokeKind::Static, cursor.read_u16_be()?),
            OpCode::invokeinterface => {
                let index = cursor.read_u16_be()?;
                let count = NonZeroU8::new(cursor.read_u8()?)
                    .wrap_err("invokeinterface count must not be 0")?;
                let zero = cursor.read_u8()?;
                if zero != 0 {
                    bail!("invalid bytes found in invokeinterface instruction: 0x{zero:0x}");
                }
                Instruction::invoke(InvokeKind::Interface { count }, index)
            }
            OpCode::invokedynamic => {
                let index = cursor.read_u16_be()?;
                let zero = cursor.read_u16_be()?;
                if zero != 0 {
                    bail!("invalid bytes found in invokedynamic instruction: 0x{zero:0x}");
                }
                Instruction::invoke(InvokeKind::Dynamic, index)
            }
            OpCode::new => Instruction::new(cursor.read_u16_be()?),
            OpCode::newarray => Instruction::newarray(
                ArrayType::from_repr(cursor.read_u8()?).wrap_err("invalid array type")?,
            ),
            OpCode::anewarray => Instruction::anewarray(cursor.read_u16_be()?),
            OpCode::arraylength => Instruction::arraylength,
            OpCode::athrow => Instruction::athrow,
            OpCode::checkcast => Instruction::checkcast(cursor.read_u16_be()?),
            OpCode::instanceof => Instruction::instanceof(cursor.read_u16_be()?),
            OpCode::monitorenter => Instruction::monitorenter,
            OpCode::monitorexit => Instruction::monitorexit,
            OpCode::wide => todo!(),
            OpCode::multianewarray => {
                Instruction::multianewarray(cursor.read_u16_be()?, cursor.read_u8()?)
            }
            OpCode::ifnull => Instruction::ifnull(cursor.read_i16_be()?),
            OpCode::ifnonnull => Instruction::ifnonnull(cursor.read_i16_be()?),
            OpCode::goto_w => Instruction::goto(cursor.read_i32_be()?),
            OpCode::jsr_w => Instruction::jsr(cursor.read_i32_be()?),
            OpCode::breakpoint | OpCode::impdep1 | OpCode::impdep2 => {
                bail!("unexpected opcode: {opcode:?}")
            }
        };
        instructions.push(instruction);
    }

    // Branch values represent byte address offsets of the instruction to jump to, relative to the current instruction.
    // When instructions are decoded these addresses are no longer valid, so this step updates them to represent index
    // offsets instead.
    for (i, instruction) in instructions.iter_mut().enumerate() {
        macro_rules! address_to_index {
            ($branch:expr, $t:ty) => {{
                (index_map[address_map[i].checked_add_signed($branch as isize).unwrap()] as isize
                    - i as isize) as $t
            }};
        }

        match instruction {
            Instruction::r#if { branch, .. } => *branch = address_to_index!(*branch, i16),
            Instruction::if_icmp { branch, .. } => *branch = address_to_index!(*branch, i16),
            Instruction::if_acmp { branch, .. } => *branch = address_to_index!(*branch, i16),
            Instruction::goto { branch, .. } => *branch = address_to_index!(*branch, i32),
            Instruction::jsr { branch, .. } => *branch = address_to_index!(*branch, i32),
            Instruction::ifnull { branch, .. } => *branch = address_to_index!(*branch, i16),
            Instruction::ifnonnull { branch, .. } => *branch = address_to_index!(*branch, i16),
            _ => {}
        }
    }

    Ok(instructions)
}

trait EndianReadExt {
    fn read_u16_be(&mut self) -> io::Result<u16>;
    fn read_i16_be(&mut self) -> io::Result<i16>;
    fn read_i32_be(&mut self) -> io::Result<i32>;
}

impl<R: io::Read> EndianReadExt for R {
    fn read_u16_be(&mut self) -> io::Result<u16> {
        self.read_u16::<BigEndian>()
    }

    fn read_i16_be(&mut self) -> io::Result<i16> {
        self.read_i16::<BigEndian>()
    }

    fn read_i32_be(&mut self) -> io::Result<i32> {
        self.read_i32::<BigEndian>()
    }
}

trait Align {
    fn align_to(&mut self, align: u64);
}

impl<T> Align for Cursor<T>
where
    Self: io::Seek,
{
    fn align_to(&mut self, align: u64) {
        let pos = self.position();
        let offset = pos % align;
        if offset != 0 {
            self.set_position(pos + align - offset);
        }
    }
}

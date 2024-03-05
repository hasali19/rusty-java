use std::io::{self, Cursor};
use std::num::NonZeroU8;

use bumpalo::collections::{CollectIn, String, Vec};
use bumpalo::{vec, Bump};
use byteorder::{BigEndian, ReadBytesExt};
use color_eyre::eyre::{self, bail, eyre, Context, ContextCompat};

use crate::class_file::constant_pool::{self, ConstantInfo, ConstantPool};
use crate::class_file::{
    AttributeInfo, BootstrapMethod, BootstrapMethodsAttribute, ClassAccessFlags, ClassFile,
    CodeAttribute, CustomAttribute, ExceptionTableEntry, FieldAccessFlags, FieldInfo, InnerClass,
    InnerClassAccessFlags, InnerClassesAttribute, LineNumberTableAttribute, LineNumberTableEntry,
    MethodAccessFlags, MethodInfo, SourceFileAttribute,
};
use crate::instructions::{
    ArrayLoadStoreType, Condition, EqCondition, Instruction, IntegerType, InvokeKind, NumberType,
    OrdCondition, ReturnType,
};
use crate::opcodes::OpCode;

pub struct ClassReader<'a, R> {
    reader: R,
    arena: &'a Bump,
}

impl<'a, R: io::Read> ClassReader<'a, R> {
    pub fn new(arena: &'a Bump, reader: R) -> ClassReader<'a, R> {
        ClassReader { reader, arena }
    }

    pub fn read_class_file<'b>(&'b mut self) -> eyre::Result<ClassFile<'a>> {
        let magic = self.read_u32()?;
        if magic != 0xcafebabe {
            bail!("invalid magic bytes: 0x{magic:0x}");
        }

        let minor_version = self.read_u16()?;
        let major_version = self.read_u16()?;
        let constant_pool = self.read_constant_pool()?;
        let access_flags = ClassAccessFlags::from_bits_truncate(self.read_u16()?);
        let this_class = self.read_u16()?;
        let super_class = self.read_u16()?;
        let interfaces = self.read_interfaces()?;
        let fields = self.read_fields(&constant_pool)?;
        let methods = self.read_methods(&constant_pool)?;
        let attributes = self.read_attributes(&constant_pool)?;

        Ok(ClassFile {
            minor_version,
            major_version,
            constant_pool,
            access_flags,
            this_class,
            super_class,
            interfaces,
            fields,
            methods,
            attributes,
        })
    }

    fn read_constant_pool<'s>(&'s mut self) -> eyre::Result<ConstantPool<'a>> {
        let constant_pool_count = self.read_u16()?;
        let mut constant_pool = Vec::new_in(self.arena);
        let mut i = 1;
        while i < constant_pool_count {
            let tag = self.read_u8()?;
            let constant = match tag {
                1 => ConstantInfo::Utf8(self.read_utf8()?),
                3 => ConstantInfo::Integer(self.read_u32()? as i32),
                4 => ConstantInfo::Float(f32::from_bits(self.read_u32()?)),
                5 => ConstantInfo::Long(self.read_u64()? as i64),
                6 => ConstantInfo::Double(f64::from_bits(self.read_u64()?)),
                7 => ConstantInfo::Class(self.read_class_info()?),
                8 => ConstantInfo::String(self.read_string_info()?),
                9 => ConstantInfo::FieldRef(self.read_fieldref_info()?),
                10 => ConstantInfo::MethodRef(self.read_methodref_info()?),
                11 => ConstantInfo::InterfaceMethodRef(self.read_methodref_info()?),
                12 => ConstantInfo::NameAndType(self.read_name_and_type_info()?),
                15 => ConstantInfo::MethodHandle(self.read_method_handle_info()?),
                16 => ConstantInfo::MethodType(self.read_method_type_info()?),
                17 => ConstantInfo::Dynamic(self.read_dynamic_info()?),
                18 => ConstantInfo::InvokeDynamic(self.read_invoke_dynamic_info()?),
                19 => ConstantInfo::Module(self.read_module_info()?),
                20 => ConstantInfo::Package(self.read_package_info()?),
                _ => bail!("unknown constant pool tag: {tag}"),
            };

            constant_pool.push(constant);

            // Java made the rather poor choice of having longs and doubles take up two indexes
            // in the constant pool, so we have to write this ugly loop to increment by 2 and add
            // a dummy entry to the constant pool.
            if [5, 6].contains(&tag) {
                i += 2;
                constant_pool.push(ConstantInfo::Unused);
            } else {
                i += 1;
            }
        }
        Ok(ConstantPool(constant_pool))
    }

    fn read_utf8<'s>(&'s mut self) -> eyre::Result<String<'a>> {
        let length = self.read_u16()? as usize;
        let mut bytes = bumpalo::vec![in self.arena; 0; length];
        self.reader.read_exact(&mut bytes)?;
        String::from_utf8(bytes).map_err(|e| eyre!("{e}"))
    }

    fn read_class_info(&mut self) -> eyre::Result<constant_pool::Class> {
        Ok(constant_pool::Class {
            name_index: self.read_u16()?,
        })
    }

    fn read_string_info(&mut self) -> eyre::Result<constant_pool::String> {
        Ok(constant_pool::String {
            string_index: self.read_u16()?,
        })
    }

    fn read_fieldref_info(&mut self) -> eyre::Result<constant_pool::FieldRef> {
        Ok(constant_pool::FieldRef {
            class_index: self.read_u16()?,
            name_and_type_index: self.read_u16()?,
        })
    }

    fn read_methodref_info(&mut self) -> eyre::Result<constant_pool::MethodRef> {
        Ok(constant_pool::MethodRef {
            class_index: self.read_u16()?,
            name_and_type_index: self.read_u16()?,
        })
    }

    fn read_name_and_type_info(&mut self) -> eyre::Result<constant_pool::NameAndType> {
        Ok(constant_pool::NameAndType {
            name_index: self.read_u16()?,
            descriptor_index: self.read_u16()?,
        })
    }

    fn read_method_handle_info(&mut self) -> eyre::Result<constant_pool::MethodHandle> {
        Ok(constant_pool::MethodHandle {
            reference_kind: self.read_u8()?,
            reference_index: self.read_u16()?,
        })
    }

    fn read_method_type_info(&mut self) -> eyre::Result<constant_pool::MethodType> {
        Ok(constant_pool::MethodType {
            descriptor_index: self.read_u16()?,
        })
    }

    fn read_dynamic_info(&mut self) -> eyre::Result<constant_pool::Dynamic> {
        Ok(constant_pool::Dynamic {
            bootstrap_method_attr_index: self.read_u16()?,
            name_and_type_index: self.read_u16()?,
        })
    }

    fn read_invoke_dynamic_info(&mut self) -> eyre::Result<constant_pool::InvokeDynamic> {
        Ok(constant_pool::InvokeDynamic {
            bootstrap_method_attr_index: self.read_u16()?,
            name_and_type_index: self.read_u16()?,
        })
    }

    fn read_module_info(&mut self) -> eyre::Result<constant_pool::Module> {
        Ok(constant_pool::Module {
            name_index: self.read_u16()?,
        })
    }

    fn read_package_info(&mut self) -> eyre::Result<constant_pool::Package> {
        Ok(constant_pool::Package {
            name_index: self.read_u16()?,
        })
    }

    fn read_interfaces<'s>(&'s mut self) -> eyre::Result<Vec<'a, u16>> {
        let interfaces_count = self.read_u16()?;
        let arena = self.arena;
        (0..interfaces_count)
            .map(|_| self.read_u16())
            .collect_in::<Result<_, _>>(arena)
            .wrap_err("failed to read interfaces")
    }

    fn read_fields(
        &mut self,
        constant_pool: &ConstantPool,
    ) -> eyre::Result<Vec<'a, FieldInfo<'a>>> {
        let fields_count = self.read_u16()?;
        let arena = self.arena;
        (0..fields_count)
            .map(|_| self.read_field_info(constant_pool))
            .collect_in(arena)
    }

    fn read_field_info(&mut self, constant_pool: &ConstantPool) -> eyre::Result<FieldInfo<'a>> {
        Ok(FieldInfo {
            access_flags: FieldAccessFlags::from_bits_truncate(self.read_u16()?),
            name_index: self.read_u16()?,
            descriptor_index: self.read_u16()?,
            attributes: self.read_attributes(constant_pool)?,
        })
    }

    fn read_methods<'s, 'b>(
        &'s mut self,
        constant_pool: &'b ConstantPool,
    ) -> eyre::Result<Vec<'a, MethodInfo<'a>>> {
        let methods_count = self.read_u16()?;
        let arena = self.arena;
        (0..methods_count)
            .map(|_| self.read_method_info(constant_pool))
            .collect_in(arena)
    }

    fn read_method_info<'s, 'b>(
        &'s mut self,
        constant_pool: &'b ConstantPool,
    ) -> eyre::Result<MethodInfo<'a>> {
        let access_flags = self.read_u16()?;
        let name_index = self.read_u16()?;
        Ok(MethodInfo {
            access_flags: MethodAccessFlags::from_bits_truncate(access_flags),
            name_index,
            descriptor_index: self.read_u16()?,
            attributes: self
                .read_attributes(constant_pool)
                .wrap_err_with(|| eyre!("failed to read attributes for method: {name_index}"))?,
        })
    }

    fn read_attributes<'s, 'b>(
        &'s mut self,
        constant_pool: &'b ConstantPool,
    ) -> eyre::Result<Vec<'a, AttributeInfo<'a>>> {
        let attributes_count = self.read_u16()?;
        let arena = self.arena;
        (0..attributes_count)
            .map(|_| self.read_attribute_info(constant_pool))
            .collect_in(arena)
    }

    fn read_attribute_info<'s, 'b>(
        &'s mut self,
        constant_pool: &'b ConstantPool,
    ) -> eyre::Result<AttributeInfo<'a>> {
        let attribute_name_index = self.read_u16()?;
        let length = self.read_u32()? as usize;

        let Some(ConstantInfo::Utf8(name)) = &constant_pool.get(attribute_name_index) else {
            bail!("invalid attribute name index: {attribute_name_index}")
        };

        let attribute_info = match name.as_str() {
            "Code" => AttributeInfo::Code(self.read_code_attribute(constant_pool)?),
            "LineNumberTable" => {
                AttributeInfo::LineNumberTable(self.read_line_number_table_attribute()?)
            }
            "BootstrapMethods" => {
                AttributeInfo::BootstrapMethods(self.read_bootstrap_methods_attribute()?)
            }
            "InnerClasses" => AttributeInfo::InnerClasses(self.read_inner_classes_attribute()?),
            "SourceFile" => AttributeInfo::SourceFile(self.read_source_file_attribute()?),
            _ => AttributeInfo::Custom(CustomAttribute {
                attribute_name_index,
                info: {
                    let mut bytes = vec![in self.arena; 0; length];
                    self.reader.read_exact(&mut bytes)?;
                    bytes
                },
            }),
        };

        Ok(attribute_info)
    }

    fn read_code_attribute<'s, 'b>(
        &'s mut self,
        constant_pool: &'b ConstantPool,
    ) -> eyre::Result<CodeAttribute<'a>> {
        let arena = self.arena;
        Ok(CodeAttribute {
            max_stack: self.read_u16()?,
            max_locals: self.read_u16()?,
            code: {
                let length = self.read_u32()? as usize;
                let mut bytes = vec![in arena; 0; length];
                self.reader.read_exact(&mut bytes)?;

                let mut instructions = vec![in arena];
                let mut cursor = Cursor::new(&bytes);

                while let Ok(opcode) = cursor.read_u8() {
                    let opcode = OpCode::from_repr(opcode)
                        .wrap_err_with(|| eyre!("unknown opcode: {opcode}"))?;

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
                        OpCode::ifeq => Instruction::r#if(Condition::Eq, cursor.read_u16_be()?),
                        OpCode::ifne => Instruction::r#if(Condition::Ne, cursor.read_u16_be()?),
                        OpCode::iflt => Instruction::r#if(Condition::Lt, cursor.read_u16_be()?),
                        OpCode::ifge => Instruction::r#if(Condition::Ge, cursor.read_u16_be()?),
                        OpCode::ifgt => Instruction::r#if(Condition::Gt, cursor.read_u16_be()?),
                        OpCode::ifle => Instruction::r#if(Condition::Le, cursor.read_u16_be()?),
                        OpCode::if_icmpeq => {
                            Instruction::if_icmp(Condition::Eq, cursor.read_u16_be()?)
                        }
                        OpCode::if_icmpne => {
                            Instruction::if_icmp(Condition::Ne, cursor.read_u16_be()?)
                        }
                        OpCode::if_icmplt => {
                            Instruction::if_icmp(Condition::Lt, cursor.read_u16_be()?)
                        }
                        OpCode::if_icmpge => {
                            Instruction::if_icmp(Condition::Ge, cursor.read_u16_be()?)
                        }
                        OpCode::if_icmpgt => {
                            Instruction::if_icmp(Condition::Gt, cursor.read_u16_be()?)
                        }
                        OpCode::if_icmple => {
                            Instruction::if_icmp(Condition::Le, cursor.read_u16_be()?)
                        }
                        OpCode::if_acmpeq => {
                            Instruction::if_acmp(EqCondition::Eq, cursor.read_u16_be()?)
                        }
                        OpCode::if_acmpne => {
                            Instruction::if_acmp(EqCondition::Ne, cursor.read_u16_be()?)
                        }
                        OpCode::goto => Instruction::goto(cursor.read_u16_be()? as u32),
                        OpCode::jsr => Instruction::jsr(cursor.read_u16_be()? as u32),
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
                        OpCode::invokestatic => {
                            Instruction::invoke(InvokeKind::Static, cursor.read_u16_be()?)
                        }
                        OpCode::invokeinterface => {
                            let index = cursor.read_u16_be()?;
                            let count = NonZeroU8::new(cursor.read_u8()?)
                                .wrap_err("invokeinterface count must not be 0")?;
                            let zero = cursor.read_u8()?;
                            if zero != 0 {
                                bail!(
                                    "invalid bytes found in invokeinterface instruction: 0x{zero:0x}"
                                );
                            }
                            Instruction::invoke(InvokeKind::Interface { count }, index)
                        }
                        OpCode::invokedynamic => {
                            let index = cursor.read_u16_be()?;
                            let zero = cursor.read_u16_be()?;
                            if zero != 0 {
                                bail!(
                                    "invalid bytes found in invokedynamic instruction: 0x{zero:0x}"
                                );
                            }
                            Instruction::invoke(InvokeKind::Dynamic, index)
                        }
                        OpCode::new => Instruction::new(cursor.read_u16_be()?),
                        OpCode::newarray => Instruction::newarray(cursor.read_u8()?),
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
                        OpCode::ifnull => Instruction::ifnull(cursor.read_u16_be()?),
                        OpCode::ifnonnull => Instruction::ifnonnull(cursor.read_u16_be()?),
                        OpCode::goto_w => Instruction::goto(cursor.read_u32_be()?),
                        OpCode::jsr_w => Instruction::jsr(cursor.read_u32_be()?),
                        OpCode::breakpoint | OpCode::impdep1 | OpCode::impdep2 => {
                            bail!("unexpected opcode: {opcode:?}")
                        }
                    };
                    instructions.push(instruction);
                }

                instructions
            },
            exception_table: {
                let length = self.read_u16()? as usize;
                (0..length)
                    .map(|_| -> eyre::Result<ExceptionTableEntry> {
                        Ok(ExceptionTableEntry {
                            start_pc: self.read_u16()?,
                            end_pc: self.read_u16()?,
                            handler_pc: self.read_u16()?,
                            catch_type: self.read_u16()?,
                        })
                    })
                    .collect_in::<Result<_, _>>(arena)?
            },
            attributes: self.read_attributes(constant_pool)?,
        })
    }

    fn read_line_number_table_attribute<'s>(
        &'s mut self,
    ) -> eyre::Result<LineNumberTableAttribute<'a>> {
        let arena = self.arena;
        Ok(LineNumberTableAttribute {
            line_number_table: {
                let length = self.read_u16()? as usize;
                (0..length)
                    .map(|_| -> eyre::Result<LineNumberTableEntry> {
                        Ok(LineNumberTableEntry {
                            start_pc: self.read_u16()?,
                            line_number: self.read_u16()?,
                        })
                    })
                    .collect_in::<Result<_, _>>(arena)?
            },
        })
    }

    fn read_bootstrap_methods_attribute<'s>(
        &'s mut self,
    ) -> eyre::Result<BootstrapMethodsAttribute<'a>> {
        let arena = self.arena;
        Ok(BootstrapMethodsAttribute {
            bootstrap_methods: {
                let length = self.read_u16()? as usize;
                (0..length)
                    .map(|_| -> eyre::Result<BootstrapMethod> {
                        Ok(BootstrapMethod {
                            bootstrap_method_ref: self.read_u16()?,
                            bootstrap_arguments: {
                                let length = self.read_u16()? as usize;
                                (0..length)
                                    .map(|_| self.read_u16())
                                    .collect_in::<Result<_, _>>(arena)?
                            },
                        })
                    })
                    .collect_in::<Result<_, _>>(arena)?
            },
        })
    }

    fn read_inner_classes_attribute<'s>(&'s mut self) -> eyre::Result<InnerClassesAttribute<'a>> {
        let arena = self.arena;
        Ok(InnerClassesAttribute {
            classes: {
                let length = self.read_u16()? as usize;
                (0..length)
                    .map(|_| -> eyre::Result<InnerClass> {
                        Ok(InnerClass {
                            inner_class_info_index: self.read_u16()?,
                            outer_class_info_index: self.read_u16()?,
                            inner_name_index: self.read_u16()?,
                            inner_class_access_flags: InnerClassAccessFlags::from_bits_truncate(
                                self.read_u16()?,
                            ),
                        })
                    })
                    .collect_in::<Result<_, _>>(arena)?
            },
        })
    }

    fn read_source_file_attribute(&mut self) -> eyre::Result<SourceFileAttribute> {
        Ok(SourceFileAttribute {
            sourcefile_index: self.read_u16()?,
        })
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        self.reader.read_u8()
    }

    fn read_u16(&mut self) -> io::Result<u16> {
        self.reader.read_u16::<BigEndian>()
    }

    fn read_u32(&mut self) -> io::Result<u32> {
        self.reader.read_u32::<BigEndian>()
    }

    fn read_u64(&mut self) -> io::Result<u64> {
        self.reader.read_u64::<BigEndian>()
    }
}

trait EndianReadExt {
    fn read_u16_be(&mut self) -> io::Result<u16>;
    fn read_i16_be(&mut self) -> io::Result<i16>;
    fn read_u32_be(&mut self) -> io::Result<u32>;
    fn read_i32_be(&mut self) -> io::Result<i32>;
}

impl<R: io::Read> EndianReadExt for R {
    fn read_u16_be(&mut self) -> io::Result<u16> {
        self.read_u16::<BigEndian>()
    }

    fn read_i16_be(&mut self) -> io::Result<i16> {
        self.read_i16::<BigEndian>()
    }

    fn read_u32_be(&mut self) -> io::Result<u32> {
        self.read_u32::<BigEndian>()
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

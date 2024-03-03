use std::io::{self, Cursor};

use byteorder::{BigEndian, ReadBytesExt};
use color_eyre::eyre::{self, bail, Context};

use crate::class_file::constant_pool::{self, ConstantInfo, ConstantPool};
use crate::class_file::{
    AttributeInfo, BootstrapMethod, BootstrapMethodsAttribute, ClassAccessFlags, ClassFile,
    CodeAttribute, CustomAttribute, ExceptionTableEntry, FieldAccessFlags, FieldInfo, InnerClass,
    InnerClassAccessFlags, InnerClassesAttribute, Instruction, LineNumberTableAttribute,
    LineNumberTableEntry, MethodAccessFlags, MethodInfo, SourceFileAttribute,
};

pub struct ClassReader<R>(R);

impl<R: io::Read> ClassReader<R> {
    pub fn new(reader: R) -> ClassReader<R> {
        ClassReader(reader)
    }

    pub fn read_class_file(&mut self) -> eyre::Result<ClassFile> {
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

    fn read_constant_pool(&mut self) -> eyre::Result<ConstantPool> {
        let constant_pool_count = self.read_u16()?;
        let mut constant_pool = vec![];
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

    fn read_utf8(&mut self) -> eyre::Result<String> {
        let length = self.read_u16()? as usize;
        let mut bytes = vec![0; length];
        self.0.read_exact(&mut bytes)?;
        String::from_utf8(bytes).wrap_err("failed to read utf8 from constant pool")
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

    fn read_interfaces(&mut self) -> eyre::Result<Vec<u16>> {
        let interfaces_count = self.read_u16()?;
        (0..interfaces_count)
            .map(|_| self.read_u16())
            .collect::<Result<_, _>>()
            .wrap_err("failed to read interfaces")
    }

    fn read_fields(&mut self, constant_pool: &ConstantPool) -> eyre::Result<Vec<FieldInfo>> {
        let fields_count = self.read_u16()?;
        (0..fields_count)
            .map(|_| self.read_field_info(constant_pool))
            .collect()
    }

    fn read_field_info(&mut self, constant_pool: &ConstantPool) -> eyre::Result<FieldInfo> {
        Ok(FieldInfo {
            access_flags: FieldAccessFlags::from_bits_truncate(self.read_u16()?),
            name_index: self.read_u16()?,
            descriptor_index: self.read_u16()?,
            attributes: self.read_attributes(constant_pool)?,
        })
    }

    fn read_methods(&mut self, constant_pool: &ConstantPool) -> eyre::Result<Vec<MethodInfo>> {
        let methods_count = self.read_u16()?;
        (0..methods_count)
            .map(|_| self.read_method_info(constant_pool))
            .collect()
    }

    fn read_method_info(&mut self, constant_pool: &ConstantPool) -> eyre::Result<MethodInfo> {
        Ok(MethodInfo {
            access_flags: MethodAccessFlags::from_bits_truncate(self.read_u16()?),
            name_index: self.read_u16()?,
            descriptor_index: self.read_u16()?,
            attributes: self.read_attributes(constant_pool)?,
        })
    }

    fn read_attributes(
        &mut self,
        constant_pool: &ConstantPool,
    ) -> eyre::Result<Vec<AttributeInfo>> {
        let attributes_count = self.read_u16()?;
        (0..attributes_count)
            .map(|_| self.read_attribute_info(constant_pool))
            .collect()
    }

    fn read_attribute_info(&mut self, constant_pool: &ConstantPool) -> eyre::Result<AttributeInfo> {
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
                    let mut bytes = vec![0; length];
                    self.0.read_exact(&mut bytes)?;
                    bytes
                },
            }),
        };

        Ok(attribute_info)
    }

    fn read_code_attribute(&mut self, constant_pool: &ConstantPool) -> eyre::Result<CodeAttribute> {
        Ok(CodeAttribute {
            max_stack: self.read_u16()?,
            max_locals: self.read_u16()?,
            code: {
                let length = self.read_u32()? as usize;
                let mut bytes = vec![0; length];
                self.0.read_exact(&mut bytes)?;

                let mut instructions = vec![];
                let mut cursor = Cursor::new(&bytes);

                while let Ok(opcode) = cursor.read_u8() {
                    let instruction = match opcode {
                        2 => Instruction::iconst { value: -1 },
                        3 => Instruction::iconst { value: 0 },
                        4 => Instruction::iconst { value: 1 },
                        5 => Instruction::iconst { value: 2 },
                        6 => Instruction::iconst { value: 3 },
                        7 => Instruction::iconst { value: 4 },
                        8 => Instruction::iconst { value: 5 },
                        18 => Instruction::ldc {
                            index: cursor.read_u8()? as u16,
                        },
                        19 => Instruction::ldc {
                            index: cursor.read_u16_be()?,
                        },
                        20 => Instruction::ldc2 {
                            index: cursor.read_u16_be()?,
                        },
                        21 => Instruction::iload {
                            index: cursor.read_u8()?,
                        },
                        25 => Instruction::aload {
                            index: cursor.read_u8()?,
                        },
                        26 => Instruction::iload { index: 0 },
                        27 => Instruction::iload { index: 1 },
                        28 => Instruction::iload { index: 2 },
                        29 => Instruction::iload { index: 3 },
                        42 => Instruction::aload { index: 0 },
                        43 => Instruction::aload { index: 1 },
                        44 => Instruction::aload { index: 2 },
                        45 => Instruction::aload { index: 3 },
                        54 => Instruction::istore {
                            index: cursor.read_u8()?,
                        },
                        59 => Instruction::istore { index: 0 },
                        60 => Instruction::istore { index: 1 },
                        61 => Instruction::istore { index: 2 },
                        62 => Instruction::istore { index: 3 },
                        177 => Instruction::retvoid,
                        183 => Instruction::invokespecial {
                            index: cursor.read_u16_be()?,
                        },
                        184 => Instruction::invokestatic {
                            index: cursor.read_u16_be()?,
                        },
                        186 => {
                            let index = cursor.read_u16_be()?;
                            let zero = cursor.read_u16_be()?;
                            if zero != 0 {
                                bail!(
                                    "invalid bytes found in invokedynamic instruction: 0x{zero:0x}"
                                );
                            }
                            Instruction::invokedynamic { index }
                        }
                        _ => bail!("unknown opcode: {opcode}"),
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
                    .collect::<Result<_, _>>()?
            },
            attributes: self.read_attributes(constant_pool)?,
        })
    }

    fn read_line_number_table_attribute(&mut self) -> eyre::Result<LineNumberTableAttribute> {
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
                    .collect::<Result<_, _>>()?
            },
        })
    }

    fn read_bootstrap_methods_attribute(&mut self) -> eyre::Result<BootstrapMethodsAttribute> {
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
                                    .collect::<Result<_, _>>()?
                            },
                        })
                    })
                    .collect::<Result<_, _>>()?
            },
        })
    }

    fn read_inner_classes_attribute(&mut self) -> eyre::Result<InnerClassesAttribute> {
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
                    .collect::<Result<_, _>>()?
            },
        })
    }

    fn read_source_file_attribute(&mut self) -> eyre::Result<SourceFileAttribute> {
        Ok(SourceFileAttribute {
            sourcefile_index: self.read_u16()?,
        })
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        self.0.read_u8()
    }

    fn read_u16(&mut self) -> io::Result<u16> {
        self.0.read_u16::<BigEndian>()
    }

    fn read_u32(&mut self) -> io::Result<u32> {
        self.0.read_u32::<BigEndian>()
    }

    fn read_u64(&mut self) -> io::Result<u64> {
        self.0.read_u64::<BigEndian>()
    }
}

trait EndianReadExt {
    fn read_u16_be(&mut self) -> io::Result<u16>;
}

impl<R: io::Read> EndianReadExt for R {
    fn read_u16_be(&mut self) -> io::Result<u16> {
        self.read_u16::<BigEndian>()
    }
}

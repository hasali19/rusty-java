use std::io;

use bumpalo::collections::{CollectIn, String, Vec};
use bumpalo::{vec, Bump};
use byteorder::{BigEndian, ReadBytesExt};
use color_eyre::eyre::{self, bail, eyre, Context};

use crate::class_file::constant_pool::{self, ConstantInfo, ConstantPool};
use crate::class_file::{
    AttributeInfo, BootstrapMethod, BootstrapMethodsAttribute, ClassAccessFlags, ClassFile,
    CodeAttribute, CustomAttribute, ExceptionTableEntry, FieldAccessFlags, FieldInfo, InnerClass,
    InnerClassAccessFlags, InnerClassesAttribute, LineNumberTableAttribute, LineNumberTableEntry,
    MethodAccessFlags, MethodInfo, SourceFileAttribute,
};

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
                bytes
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

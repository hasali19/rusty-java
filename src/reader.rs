use std::io;

use bitflags::bitflags;
use byteorder::{BigEndian, ReadBytesExt};
use color_eyre::eyre::{self, bail, Context, ContextCompat};
use constant_pool::{ConstantInfo, ConstantPool};

#[derive(Debug)]
pub struct ClassFile {
    pub minor_version: u16,
    pub major_version: u16,
    pub constant_pool: ConstantPool,
    pub access_flags: u16,
    pub this_class: u16,
    pub super_class: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub attributes: Vec<AttributeInfo>,
}

mod constant_pool {
    use std::ops::Index;

    #[derive(Debug)]
    pub struct ConstantPool(pub(crate) Vec<ConstantInfo>);

    impl ConstantPool {
        pub fn get(&self, index: u16) -> Option<&ConstantInfo> {
            self.0.get(index.checked_sub(1)? as usize)
        }
    }

    impl Index<u16> for ConstantPool {
        type Output = ConstantInfo;

        fn index(&self, index: u16) -> &Self::Output {
            &self.0[index as usize - 1]
        }
    }

    #[derive(Debug)]
    pub enum ConstantInfo {
        Utf8(std::string::String),
        Class(Class),
        String(String),
        MethodRef(MethodRef),
        NameAndType(NameAndType),
        MethodHandle,
        InvokeDynamic(InvokeDynamic),
    }

    #[derive(Debug)]
    pub struct Class {
        pub name_index: u16,
    }

    #[derive(Debug)]
    pub struct String {
        pub string_index: u16,
    }

    #[derive(Debug)]
    pub struct MethodRef {
        pub class_index: u16,
        pub name_and_type_index: u16,
    }

    #[derive(Debug)]
    pub struct NameAndType {
        pub name_index: u16,
        pub descriptor_index: u16,
    }

    #[derive(Debug)]
    pub struct InvokeDynamic {
        pub bootstrap_method_attr_index: u16,
        pub name_and_type_index: u16,
    }
}

#[derive(Debug)]
pub struct FieldInfo {
    pub access_flags: u16,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<AttributeInfo>,
}

#[derive(Debug)]
pub struct MethodInfo {
    pub access_flags: MethodAccessFlags,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<AttributeInfo>,
}

bitflags! {
    #[derive(Debug)]
    pub struct MethodAccessFlags: u16 {
        const PUBLIC = 0x0001;
        const PRIVATE = 0x0002;
        const PROTECTED = 0x0004;
        const STATIC = 0x0008;
        const FINAL = 0x0010;
        const SYNCHRONIZED = 0x0020;
        const BRIDGE = 0x0040;
        const VARARGS = 0x0080;
        const NATIVE = 0x0100;
        const ABSTRACT = 0x0400;
        const STRICT = 0x0800;
        const SYNTHETIC = 0x1000;
    }
}

#[derive(Debug)]
pub enum AttributeInfo {
    Code(CodeAttributeInfo),
    LineNumberTable(LineNumberTableAttributeInfo),
    Custom(CustomAttributeInfo),
}

#[derive(Debug)]
pub struct CodeAttributeInfo {
    pub max_stack: u16,
    pub max_locals: u16,
    pub code: Vec<u8>,
    pub exception_table: Vec<ExceptionTableEntry>,
    pub attributes: Vec<AttributeInfo>,
}

#[derive(Debug)]
pub struct ExceptionTableEntry {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    pub catch_type: u16,
}

#[derive(Debug)]
pub struct LineNumberTableAttributeInfo {
    pub line_number_table: Vec<LineNumberTableEntry>,
}

#[derive(Debug)]
pub struct LineNumberTableEntry {
    pub start_pc: u16,
    pub line_number: u16,
}

#[derive(Debug)]
pub struct CustomAttributeInfo {
    pub attribute_name_index: u16,
    pub info: Vec<u8>,
}

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
        let access_flags = self.read_u16()?;
        let this_class = self.read_u16()?;
        let super_class = self.read_u16()?;
        let interfaces = self.read_interfaces()?;
        let fields = self.read_fields()?;
        let methods = self.read_methods(&constant_pool)?;

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
            attributes: vec![],
        })
    }

    fn read_constant_pool(&mut self) -> eyre::Result<ConstantPool> {
        let constant_pool_count = self.read_u16()?;
        let mut constant_pool = vec![];
        for _ in 0..constant_pool_count - 1 {
            let tag = self.read_u8()?;
            let constant = match tag {
                1 => ConstantInfo::Utf8(self.read_utf8()?),
                7 => ConstantInfo::Class(self.read_class_info()?),
                8 => ConstantInfo::String(self.read_string_info()?),
                10 => ConstantInfo::MethodRef(self.read_methodref_info()?),
                12 => ConstantInfo::NameAndType(self.read_name_and_type_info()?),
                15 => {
                    // TODO: Read fields
                    self.skip(3)?;
                    ConstantInfo::MethodHandle
                }
                18 => ConstantInfo::InvokeDynamic(self.read_invoke_dynamic_info()?),
                _ => bail!("unknown constant pool tag: {tag}"),
            };
            constant_pool.push(constant);
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

    fn read_invoke_dynamic_info(&mut self) -> eyre::Result<constant_pool::InvokeDynamic> {
        Ok(constant_pool::InvokeDynamic {
            bootstrap_method_attr_index: self.read_u16()?,
            name_and_type_index: self.read_u16()?,
        })
    }

    fn read_interfaces(&mut self) -> eyre::Result<Vec<u16>> {
        let interfaces_count = self.read_u16()?;
        (0..interfaces_count)
            .map(|_| self.read_u16())
            .collect::<Result<_, _>>()
            .wrap_err("failed to read interfaces")
    }

    fn read_fields(&mut self) -> eyre::Result<Vec<FieldInfo>> {
        let fields_count = self.read_u16()?;
        (0..fields_count).map(|_| self.read_field_info()).collect()
    }

    fn read_field_info(&mut self) -> eyre::Result<FieldInfo> {
        todo!()
    }

    fn read_methods(&mut self, constant_pool: &ConstantPool) -> eyre::Result<Vec<MethodInfo>> {
        let methods_count = self.read_u16()?;
        (0..methods_count)
            .map(|_| self.read_method_info(constant_pool))
            .collect()
    }

    fn read_method_info(&mut self, constant_pool: &ConstantPool) -> eyre::Result<MethodInfo> {
        Ok(MethodInfo {
            access_flags: MethodAccessFlags::from_bits(self.read_u16()?)
                .wrap_err("unexpected bits in method access flags")?,
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
            "LineNumberTable" => AttributeInfo::LineNumberTable(self.read_line_number_table()?),
            _ => AttributeInfo::Custom(CustomAttributeInfo {
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

    fn read_code_attribute(
        &mut self,
        constant_pool: &ConstantPool,
    ) -> eyre::Result<CodeAttributeInfo> {
        Ok(CodeAttributeInfo {
            max_stack: self.read_u16()?,
            max_locals: self.read_u16()?,
            code: {
                let length = self.read_u32()? as usize;
                let mut bytes = vec![0; length];
                self.0.read_exact(&mut bytes)?;
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
                    .collect::<Result<_, _>>()?
            },
            attributes: self.read_attributes(constant_pool)?,
        })
    }

    fn read_line_number_table(&mut self) -> eyre::Result<LineNumberTableAttributeInfo> {
        Ok(LineNumberTableAttributeInfo {
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

    fn read_u8(&mut self) -> io::Result<u8> {
        self.0.read_u8()
    }

    fn read_u16(&mut self) -> io::Result<u16> {
        self.0.read_u16::<BigEndian>()
    }

    fn read_u32(&mut self) -> io::Result<u32> {
        self.0.read_u32::<BigEndian>()
    }

    fn skip(&mut self, n: usize) -> io::Result<()> {
        for _ in 0..n {
            self.read_u8()?;
        }
        Ok(())
    }
}

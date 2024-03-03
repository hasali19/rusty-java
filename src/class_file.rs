use bitflags::bitflags;

use self::constant_pool::ConstantPool;

#[derive(Debug)]
pub struct ClassFile {
    pub minor_version: u16,
    pub major_version: u16,
    pub constant_pool: ConstantPool,
    pub access_flags: ClassAccessFlags,
    pub this_class: u16,
    pub super_class: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub attributes: Vec<AttributeInfo>,
}

pub mod constant_pool {
    use std::ops::Index;

    use strum::EnumTryAs;

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

    #[derive(Debug, EnumTryAs)]
    pub enum ConstantInfo {
        Unused,
        Utf8(std::string::String),
        Integer(i32),
        Float(f32),
        Long(i64),
        Double(f64),
        Class(Class),
        String(String),
        FieldRef(FieldRef),
        MethodRef(MethodRef),
        InterfaceMethodRef(MethodRef),
        NameAndType(NameAndType),
        MethodHandle(MethodHandle),
        MethodType(MethodType),
        Dynamic(Dynamic),
        InvokeDynamic(InvokeDynamic),
        Module(Module),
        Package(Package),
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
    pub struct FieldRef {
        pub class_index: u16,
        pub name_and_type_index: u16,
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
    pub struct MethodHandle {
        pub reference_kind: u8,
        pub reference_index: u16,
    }

    #[derive(Debug)]
    pub struct MethodType {
        pub descriptor_index: u16,
    }

    #[derive(Debug)]
    pub struct Dynamic {
        pub bootstrap_method_attr_index: u16,
        pub name_and_type_index: u16,
    }

    #[derive(Debug)]
    pub struct InvokeDynamic {
        pub bootstrap_method_attr_index: u16,
        pub name_and_type_index: u16,
    }

    #[derive(Debug)]
    pub struct Module {
        pub name_index: u16,
    }

    #[derive(Debug)]
    pub struct Package {
        pub name_index: u16,
    }
}

bitflags! {
    #[derive(Debug)]
    pub struct ClassAccessFlags: u16 {
        const PUBLIC = 0x0001;
        const FINAL = 0x0010;
        const SUPER = 0x0020;
        const INTERFACE = 0x0200;
        const ABSTRACT = 0x0400;
        const SYNTHETIC = 0x1000;
        const ANNOTATION = 0x2000;
        const ENUM = 0x4000;
        const MODULE = 0x8000;
    }
}

#[derive(Debug)]
pub struct FieldInfo {
    pub access_flags: FieldAccessFlags,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<AttributeInfo>,
}

bitflags! {
    #[derive(Debug)]
    pub struct FieldAccessFlags: u16 {
        const PUBLIC = 0x0001;
        const PRIVATE = 0x0002;
        const PROTECTED = 0x0004;
        const STATIC = 0x0008;
        const FINAL = 0x0010;
        const VOLATILE = 0x0040;
        const TRANSIENT = 0x0080;
        const SYNTHETIC = 0x1000;
        const ENUM = 0x4000;
    }
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
    Code(CodeAttribute),
    LineNumberTable(LineNumberTableAttribute),
    BootstrapMethods(BootstrapMethodsAttribute),
    InnerClasses(InnerClassesAttribute),
    SourceFile(SourceFileAttribute),
    Custom(CustomAttribute),
}

#[derive(Debug)]
pub struct CodeAttribute {
    pub max_stack: u16,
    pub max_locals: u16,
    pub code: Vec<Instruction>,
    pub exception_table: Vec<ExceptionTableEntry>,
    pub attributes: Vec<AttributeInfo>,
}

#[allow(non_camel_case_types)]
#[derive(Debug)]
pub enum Instruction {
    aload { index: u8 },
    invokespecial { index: u16 },
    retvoid,
    iconst { value: i8 },
    istore { index: u8 },
    iload { index: u8 },
    invokedynamic { index: u16 },
    invokestatic { index: u16 },
    ldc { index: u16 },
    ldc2 { index: u16 },
}

#[derive(Debug)]
pub struct ExceptionTableEntry {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    pub catch_type: u16,
}

#[derive(Debug)]
pub struct LineNumberTableAttribute {
    pub line_number_table: Vec<LineNumberTableEntry>,
}

#[derive(Debug)]
pub struct LineNumberTableEntry {
    pub start_pc: u16,
    pub line_number: u16,
}

#[derive(Debug)]
pub struct BootstrapMethodsAttribute {
    pub bootstrap_methods: Vec<BootstrapMethod>,
}

#[derive(Debug)]
pub struct BootstrapMethod {
    pub bootstrap_method_ref: u16,
    pub bootstrap_arguments: Vec<u16>,
}

#[derive(Debug)]
pub struct InnerClassesAttribute {
    pub classes: Vec<InnerClass>,
}

#[derive(Debug)]
pub struct InnerClass {
    pub inner_class_info_index: u16,
    pub outer_class_info_index: u16,
    pub inner_name_index: u16,
    pub inner_class_access_flags: InnerClassAccessFlags,
}

bitflags! {
    #[derive(Debug)]
    pub struct InnerClassAccessFlags: u16 {
        const PUBLIC = 0x0001;
        const PRIVATE = 0x0002;
        const PROTECTED = 0x0004;
        const STATIC = 0x0008;
        const FINAL = 0x0010;
        const INTERFACE = 0x0200;
        const ABSTRACT = 0x0400;
        const SYNTHETIC = 0x1000;
        const ANNOTATION = 0x2000;
        const ENUM = 0x4000;
    }
}

#[derive(Debug)]
pub struct SourceFileAttribute {
    pub sourcefile_index: u16,
}

#[derive(Debug)]
pub struct CustomAttribute {
    pub attribute_name_index: u16,
    pub info: Vec<u8>,
}

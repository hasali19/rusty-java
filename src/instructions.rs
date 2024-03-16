use std::num::NonZeroU8;

use strum::FromRepr;

#[allow(non_camel_case_types)]
#[derive(Debug)]
pub enum Instruction {
    // Constants
    nop,
    aconst_null,
    r#const { data_type: NumberType, value: i8 },
    bipush { value: i8 },
    sipush { value: i16 },
    ldc { index: u16 },
    ldc2 { index: u16 },
    // Loads
    load { data_type: LoadStoreType, index: u8 },
    arrayload { data_type: ArrayLoadStoreType },
    // Stores
    store { data_type: LoadStoreType, index: u8 },
    arraystore { data_type: ArrayLoadStoreType },
    // Stack
    pop,
    pop2,
    dup,
    dup_x1,
    dup_x2,
    dup2,
    dup2_x1,
    dup2_x2,
    swap,
    // Math
    add { data_type: NumberType },
    sub { data_type: NumberType },
    mul { data_type: NumberType },
    div { data_type: NumberType },
    rem { data_type: NumberType },
    neg { data_type: NumberType },
    shl { data_type: IntegerType },
    shr { data_type: IntegerType },
    ushr { data_type: IntegerType },
    and { data_type: IntegerType },
    or { data_type: IntegerType },
    xor { data_type: IntegerType },
    inc { index: u8, value: i8 },
    // Conversions
    i2l,
    i2f,
    i2d,
    l2i,
    l2f,
    l2d,
    f2i,
    f2l,
    f2d,
    d2i,
    d2l,
    d2f,
    i2b,
    i2c,
    i2s,
    // Comparisons
    lcmp,
    fcmp { condition: OrdCondition },
    dcmp { condition: OrdCondition },
    r#if { condition: Condition, branch: i16 },
    if_icmp { condition: Condition, branch: i16 },
    if_acmp { condition: EqCondition, branch: i16 },
    // References
    getstatic { index: u16 },
    putstatic { index: u16 },
    getfield { index: u16 },
    putfield { index: u16 },
    invoke { kind: InvokeKind, index: u16 },
    new { index: u16 },
    newarray { atype: ArrayType },
    anewarray { index: u16 },
    arraylength,
    athrow,
    checkcast { index: u16 },
    instanceof { index: u16 },
    monitorenter,
    monitorexit,
    // Control
    goto { branch: i32 },
    jsr { branch: i32 },
    ret { index: u8 },
    tableswitch {/* TODO */},
    lookupswitch {},
    r#return { data_type: ReturnType },
    // Extended
    // wide,
    multianewarray { index: u16, dimensions: u8 },
    ifnull { branch: i16 },
    ifnonnull { branch: i16 },
    // Reserved
    breakpoint,
    impdep1,
    impdep2,
}

#[derive(Debug)]
pub enum NumberType {
    Int,
    Long,
    Float,
    Double,
}

#[derive(Debug)]
pub enum IntegerType {
    Int,
    Long,
}

#[derive(Debug)]
pub enum LoadStoreType {
    Int,
    Long,
    Float,
    Double,
    Reference,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ArrayLoadStoreType {
    Int,
    Long,
    Float,
    Double,
    Reference,
    Byte,
    Char,
    Short,
}

#[derive(Debug)]
pub enum Condition {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug)]
pub enum EqCondition {
    Eq,
    Ne,
}

#[derive(Debug)]
pub enum OrdCondition {
    Lt,
    Gt,
}

#[derive(Debug)]
pub enum IfCmpType {
    Int,
    Reference,
}

#[derive(Clone, Copy, Debug)]
pub enum InvokeKind {
    Virtual,
    Special,
    Static,
    Interface { count: NonZeroU8 },
    Dynamic,
}

#[derive(Debug)]
pub enum ReturnType {
    Void,
    Int,
    Long,
    Float,
    Double,
    Reference,
}

#[derive(Clone, Copy, Debug, FromRepr)]
#[repr(u8)]
pub enum ArrayType {
    Boolean = 4,
    Char = 5,
    Float = 6,
    Double = 7,
    Byte = 8,
    Short = 9,
    Int = 10,
    Long = 11,
}

impl Instruction {
    pub fn iconst(value: i8) -> Instruction {
        Instruction::r#const {
            data_type: NumberType::Int,
            value,
        }
    }

    pub fn lconst(value: i8) -> Instruction {
        Instruction::r#const {
            data_type: NumberType::Long,
            value,
        }
    }

    pub fn fconst(value: i8) -> Instruction {
        Instruction::r#const {
            data_type: NumberType::Float,
            value,
        }
    }

    pub fn dconst(value: i8) -> Instruction {
        Instruction::r#const {
            data_type: NumberType::Double,
            value,
        }
    }

    pub fn bipush(value: i8) -> Instruction {
        Instruction::bipush { value }
    }

    pub fn sipush(value: i16) -> Instruction {
        Instruction::sipush { value }
    }

    pub fn ldc(index: u16) -> Instruction {
        Instruction::ldc { index }
    }

    pub fn ldc2(index: u16) -> Instruction {
        Instruction::ldc2 { index }
    }

    pub fn iload(index: u8) -> Instruction {
        Instruction::load {
            data_type: LoadStoreType::Int,
            index,
        }
    }

    pub fn lload(index: u8) -> Instruction {
        Instruction::load {
            data_type: LoadStoreType::Long,
            index,
        }
    }

    pub fn fload(index: u8) -> Instruction {
        Instruction::load {
            data_type: LoadStoreType::Float,
            index,
        }
    }

    pub fn dload(index: u8) -> Instruction {
        Instruction::load {
            data_type: LoadStoreType::Double,
            index,
        }
    }

    pub fn aload(index: u8) -> Instruction {
        Instruction::load {
            data_type: LoadStoreType::Reference,
            index,
        }
    }

    pub fn arrayload(data_type: ArrayLoadStoreType) -> Instruction {
        Instruction::arrayload { data_type }
    }

    pub fn istore(index: u8) -> Instruction {
        Instruction::store {
            data_type: LoadStoreType::Int,
            index,
        }
    }

    pub fn lstore(index: u8) -> Instruction {
        Instruction::store {
            data_type: LoadStoreType::Long,
            index,
        }
    }

    pub fn fstore(index: u8) -> Instruction {
        Instruction::store {
            data_type: LoadStoreType::Float,
            index,
        }
    }

    pub fn dstore(index: u8) -> Instruction {
        Instruction::store {
            data_type: LoadStoreType::Double,
            index,
        }
    }

    pub fn astore(index: u8) -> Instruction {
        Instruction::store {
            data_type: LoadStoreType::Reference,
            index,
        }
    }

    pub fn arraystore(data_type: ArrayLoadStoreType) -> Instruction {
        Instruction::arraystore { data_type }
    }

    pub fn add(data_type: NumberType) -> Instruction {
        Instruction::add { data_type }
    }

    pub fn sub(data_type: NumberType) -> Instruction {
        Instruction::sub { data_type }
    }

    pub fn mul(data_type: NumberType) -> Instruction {
        Instruction::mul { data_type }
    }

    pub fn div(data_type: NumberType) -> Instruction {
        Instruction::div { data_type }
    }

    pub fn rem(data_type: NumberType) -> Instruction {
        Instruction::rem { data_type }
    }

    pub fn neg(data_type: NumberType) -> Instruction {
        Instruction::neg { data_type }
    }

    pub fn shl(data_type: IntegerType) -> Instruction {
        Instruction::shl { data_type }
    }

    pub fn shr(data_type: IntegerType) -> Instruction {
        Instruction::shr { data_type }
    }

    pub fn ushr(data_type: IntegerType) -> Instruction {
        Instruction::ushr { data_type }
    }

    pub fn and(data_type: IntegerType) -> Instruction {
        Instruction::and { data_type }
    }

    pub fn or(data_type: IntegerType) -> Instruction {
        Instruction::or { data_type }
    }

    pub fn xor(data_type: IntegerType) -> Instruction {
        Instruction::xor { data_type }
    }

    pub fn inc(index: u8, value: i8) -> Instruction {
        Instruction::inc { index, value }
    }

    pub fn fcmp(condition: OrdCondition) -> Instruction {
        Instruction::fcmp { condition }
    }

    pub fn dcmp(condition: OrdCondition) -> Instruction {
        Instruction::dcmp { condition }
    }

    pub fn r#if(condition: Condition, branch: i16) -> Instruction {
        Instruction::r#if { condition, branch }
    }

    pub fn if_icmp(condition: Condition, branch: i16) -> Instruction {
        Instruction::if_icmp { condition, branch }
    }

    pub fn if_acmp(condition: EqCondition, branch: i16) -> Instruction {
        Instruction::if_acmp { condition, branch }
    }

    pub fn goto(branch: i32) -> Instruction {
        Instruction::goto { branch }
    }

    pub fn jsr(branch: i32) -> Instruction {
        Instruction::jsr { branch }
    }

    pub fn ret(index: u8) -> Instruction {
        Instruction::ret { index }
    }

    pub fn r#return(data_type: ReturnType) -> Instruction {
        Instruction::r#return { data_type }
    }

    pub fn multianewarray(index: u16, dimensions: u8) -> Instruction {
        Instruction::multianewarray { index, dimensions }
    }

    pub fn getfield(index: u16) -> Instruction {
        Instruction::getfield { index }
    }

    pub fn putfield(index: u16) -> Instruction {
        Instruction::putfield { index }
    }

    pub fn getstatic(index: u16) -> Instruction {
        Instruction::getstatic { index }
    }

    pub fn putstatic(index: u16) -> Instruction {
        Instruction::putstatic { index }
    }

    pub fn invoke(kind: InvokeKind, index: u16) -> Instruction {
        Instruction::invoke { kind, index }
    }

    pub fn new(index: u16) -> Instruction {
        Instruction::new { index }
    }

    pub fn newarray(atype: ArrayType) -> Instruction {
        Instruction::newarray { atype }
    }

    pub fn anewarray(index: u16) -> Instruction {
        Instruction::anewarray { index }
    }

    pub fn checkcast(index: u16) -> Instruction {
        Instruction::checkcast { index }
    }

    pub fn instanceof(index: u16) -> Instruction {
        Instruction::instanceof { index }
    }

    pub fn ifnull(branch: i16) -> Instruction {
        Instruction::ifnull { branch }
    }

    pub fn ifnonnull(branch: i16) -> Instruction {
        Instruction::ifnull { branch }
    }
}

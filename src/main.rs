use std::fs::File;
use std::io::BufReader;

use clap::Parser;
use color_eyre::eyre::{self, bail, eyre, Context, ContextCompat};
use rusty_java::class::Class;
use rusty_java::class_file::constant_pool::{self, ConstantInfo};
use rusty_java::class_file::{AttributeInfo, Instruction};
use rusty_java::reader::ClassReader;

#[derive(clap::Parser)]
struct Args {
    class_file: String,
    #[clap(long)]
    dump: bool,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let args = Args::parse();

    let class_file = ClassReader::new(BufReader::new(File::open(&args.class_file)?))
        .read_class_file()
        .wrap_err_with(|| eyre!("failed to read class file at '{}'", args.class_file))?;

    let class = Class::new(&class_file)?;

    if args.dump {
        println!("{class:#?}");
    } else {
        execute_method(&class, "main").wrap_err("failed to execute main method")?;
    }

    Ok(())
}

fn execute_method(class: &Class, method_name: &str) -> eyre::Result<()> {
    let method = class
        .method(method_name)
        .wrap_err_with(|| eyre!("method not found"))?;

    let code_attr = method
        .attributes
        .iter()
        .find_map(|attr| {
            let AttributeInfo::Code(attr) = attr else {
                return None;
            };
            Some(attr)
        })
        .wrap_err("missing code attribute")?;

    let code = &code_attr.code;

    enum Operand<'a> {
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

    #[derive(Clone, Copy, Debug)]
    enum Local {
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

    let mut pc = 0;
    let mut locals = vec![Local::None; code_attr.max_locals as usize];
    let mut operand_stack = Vec::with_capacity(code_attr.max_stack as usize);

    loop {
        let instruction = &code[pc];
        match instruction {
            Instruction::aload { index } => todo!(),
            Instruction::invokespecial { index } => todo!(),
            Instruction::retvoid => {
                // TODO: synchronized methods
                break;
            }
            Instruction::iconst { value } => {
                operand_stack.push(Operand::Byte(*value));
                pc += 1;
            }
            Instruction::istore { index } => {
                let operand = operand_stack
                    .pop()
                    .wrap_err("no operand provided to istore")?;

                locals[*index as usize] = match operand {
                    Operand::Byte(v) => Local::Byte(v),
                    Operand::StringConst(_) => todo!(),
                    Operand::Int(_) => todo!(),
                    Operand::Short(_) => todo!(),
                    Operand::Long(_) => todo!(),
                    Operand::Char(_) => todo!(),
                    Operand::Float(_) => todo!(),
                    Operand::Double(_) => todo!(),
                    Operand::Boolean(_) => todo!(),
                    Operand::ReturnAddress(_) => todo!(),
                };

                pc += 1;
            }
            Instruction::iload { index } => {
                let val = match locals[*index as usize] {
                    Local::None => 0,
                    Local::Int(v) => v,
                    Local::Byte(v) => v as i32,
                    local => bail!("iload called with invalid local: {local:?}"),
                };

                operand_stack.push(Operand::Int(val));

                pc += 1;
            }
            Instruction::invokedynamic { index } => {
                let invoke_dynamic = &class.constant_pool()[*index]
                    .try_as_invoke_dynamic_ref()
                    .wrap_err("invalid operand for invokedynamic")?;

                let name_and_type = class.constant_pool()[invoke_dynamic.name_and_type_index]
                    .try_as_name_and_type_ref()
                    .wrap_err("expected name_and_type")?;

                let name = class.constant_pool()[name_and_type.name_index]
                    .try_as_utf_8_ref()
                    .wrap_err("expected utf8")?;

                panic!("exec {name}");
            }
            Instruction::invokestatic { index } => {
                let invoke_dynamic = &class.constant_pool()[*index]
                    .try_as_method_ref_ref()
                    .wrap_err("expected methodref")?;

                let name_and_type = class.constant_pool()[invoke_dynamic.name_and_type_index]
                    .try_as_name_and_type_ref()
                    .wrap_err("expected name_and_type")?;

                let name = class.constant_pool()[name_and_type.name_index]
                    .try_as_utf_8_ref()
                    .wrap_err("expected utf8")?;

                if name == "print" {
                    let arg = operand_stack.pop().wrap_err("missing argument to print")?;
                    match arg {
                        Operand::Byte(v) => print!("{v}"),
                        Operand::StringConst(v) => print!("{v}"),
                        Operand::Int(v) => print!("{v}"),
                        Operand::Short(_) => todo!(),
                        Operand::Long(_) => todo!(),
                        Operand::Char(_) => todo!(),
                        Operand::Float(_) => todo!(),
                        Operand::Double(_) => todo!(),
                        Operand::Boolean(_) => todo!(),
                        Operand::ReturnAddress(_) => todo!(),
                    }
                    pc += 1;
                } else {
                    todo!("exec {name}");
                }
            }
            Instruction::ldc { index } => {
                match &class.constant_pool()[*index] {
                    ConstantInfo::Utf8(_) => todo!(),
                    ConstantInfo::Class(_) => todo!(),
                    ConstantInfo::String(constant_pool::String { string_index }) => operand_stack
                        .push(Operand::StringConst(
                            class.constant_pool()[*string_index]
                                .try_as_utf_8_ref()
                                .wrap_err("expected utf8")?,
                        )),
                    ConstantInfo::MethodRef(_) => todo!(),
                    ConstantInfo::NameAndType(_) => todo!(),
                    ConstantInfo::MethodHandle => todo!(),
                    ConstantInfo::InvokeDynamic(_) => todo!(),
                };
                pc += 1;
            }
            Instruction::ldc2 { index } => todo!(),
        }
    }

    Ok(())
}

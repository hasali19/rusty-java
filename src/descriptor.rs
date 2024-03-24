use color_eyre::eyre::{self, eyre};
use winnow::combinator::{alt, delimited, dispatch, empty, fail, repeat, terminated};
use winnow::token::{any, take_till, take_while};
use winnow::{PResult, Parser};

#[derive(Debug)]
pub enum BaseType<'a> {
    Byte,
    Char,
    Double,
    Float,
    Int,
    Long,
    Short,
    Boolean,
    Object(&'a str),
}

#[derive(Debug)]
pub enum FieldType<'a> {
    Base(BaseType<'a>),
    Array(u8, BaseType<'a>),
}

#[derive(Debug)]
pub struct FieldDescriptor<'a> {
    pub field_type: FieldType<'a>,
}

#[derive(Debug)]
pub struct MethodDescriptor<'a> {
    pub params: Vec<FieldType<'a>>,
    pub return_type: Option<FieldType<'a>>,
}

pub fn parse_method_descriptor(descriptor: &str) -> eyre::Result<MethodDescriptor> {
    let (params, return_type) = (parse_params_types, parse_return_type)
        .parse(descriptor)
        .map_err(|e| eyre!("{e}"))?;

    Ok(MethodDescriptor {
        params,
        return_type,
    })
}

pub fn parse_field_descriptor(descriptor: &str) -> eyre::Result<FieldDescriptor> {
    let field_type = parse_field_type
        .parse(descriptor)
        .map_err(|e| eyre!("{e}"))?;

    Ok(FieldDescriptor { field_type })
}

fn parse_base_type<'s>(input: &mut &'s str) -> PResult<BaseType<'s>> {
    dispatch! { any;
        'L' => terminated(take_till(.., ';').map(BaseType::Object), ';'),
        'B' => empty.map(|_| BaseType::Byte),
        'C' => empty.map(|_| BaseType::Char),
        'D' => empty.map(|_| BaseType::Double),
        'F' => empty.map(|_| BaseType::Float),
        'I' => empty.map(|_| BaseType::Int),
        'J' => empty.map(|_| BaseType::Long),
        'S' => empty.map(|_| BaseType::Short),
        'Z' => empty.map(|_| BaseType::Boolean),
        _ => fail,
    }
    .parse_next(input)
}

fn parse_array_type<'s>(input: &mut &'s str) -> PResult<(u8, BaseType<'s>)> {
    let parse_array_depth = take_while(1.., '[').map(|v: &str| v.len() as u8);
    (parse_array_depth, parse_base_type).parse_next(input)
}

fn parse_field_type<'s>(input: &mut &'s str) -> PResult<FieldType<'s>> {
    alt((
        parse_base_type.map(FieldType::Base),
        parse_array_type.map(|(n, ty)| FieldType::Array(n, ty)),
    ))
    .parse_next(input)
}

fn parse_params_types<'s>(input: &mut &'s str) -> PResult<Vec<FieldType<'s>>> {
    delimited("(", repeat(.., parse_field_type), ")").parse_next(input)
}

fn parse_return_type<'s>(input: &mut &'s str) -> PResult<Option<FieldType<'s>>> {
    alt(("V".map(|_| None), parse_field_type.map(Some))).parse_next(input)
}

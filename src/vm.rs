use std::fs::File;
use std::io::BufReader;
use std::iter;

use bumpalo::Bump;
use color_eyre::eyre::{self, eyre, Context};

use crate::call_frame::CallFrame;
use crate::class::{Class, Method};
use crate::class_file::MethodAccessFlags;
use crate::reader::ClassReader;

pub struct Vm<'a> {
    arena: &'a Bump,
}

impl<'a> Vm<'a> {
    pub fn new(arena: &'a Bump) -> Vm<'a> {
        Vm { arena }
    }

    pub fn load_class_file(&mut self, name: &str) -> eyre::Result<&'a Class<'a>> {
        let class_file = self.arena.alloc(
            ClassReader::new(self.arena, BufReader::new(File::open(name)?))
                .read_class_file()
                .wrap_err_with(|| eyre!("failed to read class file at '{}'", name))?,
        );

        let class = self.arena.alloc(Class::new(self.arena, class_file)?);

        if let Some(clinit) = class.method("<clinit>", "()V")
            && clinit.access_flags.contains(MethodAccessFlags::STATIC)
        {
            self.call_method(class, clinit)?;
        }

        Ok(class)
    }

    pub fn call_method(&mut self, class: &Class, method: &Method) -> eyre::Result<()> {
        CallFrame::new(class, method, iter::empty())?.execute()?;
        Ok(())
    }
}

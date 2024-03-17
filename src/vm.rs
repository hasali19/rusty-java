use std::fs::File;
use std::io::{self, BufReader};
use std::iter;
use std::path::Path;

use bumpalo::Bump;
use color_eyre::eyre::{self, eyre, Context};

use crate::call_frame::CallFrame;
use crate::class::{Class, Method};
use crate::class_file::MethodAccessFlags;
use crate::reader::ClassReader;

pub struct Vm<'a> {
    arena: &'a Bump,
    pub(crate) stdout: &'a mut dyn io::Write,
    pub(crate) heap: Bump,
}

impl<'a> Vm<'a> {
    pub fn new(arena: &'a Bump, stdout: &'a mut dyn io::Write) -> Vm<'a> {
        Vm {
            arena,
            stdout,
            heap: Bump::new(),
        }
    }

    pub fn load_class_file(&mut self, name: &str) -> eyre::Result<&'a Class<'a>> {
        let path = Path::new(name).with_extension("class");
        let class_file = self.arena.alloc(
            ClassReader::new(self.arena, BufReader::new(File::open(path)?))
                .read_class_file()
                .wrap_err_with(|| eyre!("failed to read class file '{}'", name))?,
        );

        let class = self.arena.alloc(Class::new(self.arena, class_file)?);

        if let Some(clinit) = class.method("<clinit>", "()V")
            && clinit.access_flags.contains(MethodAccessFlags::STATIC)
        {
            self.call_method(class, clinit)?;
        }

        Ok(class)
    }

    pub fn call_method(&mut self, class: &'a Class, method: &'a Method) -> eyre::Result<()> {
        CallFrame::new(class, method, iter::empty(), self)?.execute()?;
        Ok(())
    }
}

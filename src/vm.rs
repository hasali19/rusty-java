use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Cursor};
use std::iter;
use std::path::Path;
use std::time::SystemTime;

use bumpalo::Bump;
use color_eyre::eyre::{self, eyre, Context};

use crate::call_frame::CallFrame;
use crate::class::{Class, Method};
use crate::class_file::MethodAccessFlags;
use crate::reader::ClassReader;

pub trait TimeProvider {
    fn system_time(&self) -> SystemTime;
}

struct DefaultTimeProvider;

impl TimeProvider for DefaultTimeProvider {
    fn system_time(&self) -> SystemTime {
        SystemTime::now()
    }
}

pub struct Vm<'a> {
    arena: &'a Bump,
    classes: HashMap<&'a str, &'a Class<'a>>,
    pub(crate) stdout: &'a mut dyn io::Write,
    pub(crate) heap: Bump,
    pub(crate) time: Box<dyn TimeProvider>,
}

impl<'a> Vm<'a> {
    pub fn new(arena: &'a Bump, stdout: &'a mut dyn io::Write) -> Vm<'a> {
        Vm {
            arena,
            classes: HashMap::new(),
            stdout,
            heap: Bump::new(),
            time: Box::new(DefaultTimeProvider),
        }
    }

    pub fn with_time_provider(mut self, time_provider: Box<dyn TimeProvider>) -> Self {
        self.time = time_provider;
        self
    }

    pub fn load_class_file(&mut self, name: &str) -> eyre::Result<&'a Class<'a>> {
        let class_name = name.strip_suffix(".class").unwrap_or(name);

        if let Some(class) = self.classes.get(class_name) {
            return Ok(class);
        }

        let path = Path::new(name).with_extension("class");

        let reader: Box<dyn io::Read> = if path.exists() {
            Box::new(BufReader::new(
                File::open(&path).wrap_err_with(|| eyre!("failed to open {path:?}"))?,
            ))
        } else {
            Box::new(Cursor::new(
                jdk_tools::extract_jrt_class(class_name)
                    .wrap_err_with(|| eyre!("class not found: {class_name}"))?,
            ))
        };

        let class_file = self.arena.alloc(
            ClassReader::new(self.arena, reader)
                .read_class_file()
                .wrap_err_with(|| eyre!("failed to read class file '{}'", name))?,
        );

        let class = self.arena.alloc(Class::new(self.arena, class_file)?);

        if let Some(clinit) = class.method("<clinit>", "()V")
            && clinit.access_flags.contains(MethodAccessFlags::STATIC)
        {
            self.call_method(class, clinit)?;
        }

        self.classes.insert(class.name(), class);

        Ok(class)
    }

    pub fn call_method(
        &mut self,
        class: &'a Class<'a>,
        method: &'a Method<'a>,
    ) -> eyre::Result<()> {
        CallFrame::new(class, method, iter::empty(), self)?.execute()?;
        Ok(())
    }
}

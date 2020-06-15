#![allow(dead_code)]
//       ^^^^^^^^^ - prevents reporting false warnings

use void::Void;
use std::io;
use std::path::PathBuf;
use debcrafter::{PackageInstance, Package};

pub fn paragraph<W: io::Write>(mut dest: W, text: &str) -> io::Result<()> {
    let mut write_dot = false;
    for line in text.split('\n') {
        if line.len() > 0 {
            if write_dot {
                writeln!(dest, " .")?;
                write_dot = false;
            }
            writeln!(dest, " {}", line)?;
        } else {
            write_dot = true;
        }
    }
    Ok(())
}

pub trait WriteHeader {
    fn write_header<W: io::Write>(self, writer: W) -> io::Result<()>;
}

impl<T: FnOnce(&mut dyn io::Write) -> io::Result<()>> WriteHeader for T {
    fn write_header<W: io::Write>(self, mut writer: W) -> io::Result<()> {
        self(&mut writer)
    }
}

impl<'a> WriteHeader for &'a str {
    fn write_header<W: io::Write>(self, mut writer: W) -> io::Result<()> {
        write!(writer, "{}", self)
    }
}

impl WriteHeader for String {
    fn write_header<W: io::Write>(self, mut writer: W) -> io::Result<()> {
        write!(writer, "{}", self)
    }
}

impl WriteHeader for Void {
    fn write_header<W: io::Write>(self, _writer: W) -> io::Result<()> {
        match self {}
    }
}

pub struct LazyCreateBuilder<H = Void> where H: WriteHeader {
    path: PathBuf,
    header: Option<H>,
    append: bool,
}

impl<H: WriteHeader> LazyCreateBuilder<H> {
    pub fn new<P: Into<PathBuf>>(path: P, append: bool) -> Self {
        LazyCreateBuilder {
            path: path.into(),
            header: None,
            append,
        }
    }

    pub fn set_header<H2: WriteHeader>(self, header: H2) -> LazyCreateBuilder<H2> {
        LazyCreateBuilder {
            path: self.path,
            header: Some(header),
            append: self.append,
        }
    }

    pub fn finalize(self) -> LazyCreate<H> {
        LazyCreate {
            state: LazyCreateState::Empty(self.path, self.header, self.append),
        }
    }
}

enum LazyCreateState<H: WriteHeader> {
    Empty(PathBuf, Option<H>, bool),
    Created(io::BufWriter<std::fs::File>),
}

pub struct LazyCreate<H = Void> where H: WriteHeader {
    state: LazyCreateState<H>,
}

impl<H: WriteHeader> LazyCreateState<H> {
    fn create(&mut self) -> io::Result<&mut impl io::Write> {
        match self {
            LazyCreateState::Empty(path, header, append) => {
                let file = std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(*append)
                    .open(path)?;

                let mut file = io::BufWriter::new(file);
                if let Some(header) = header.take() {
                    header.write_header(&mut file)?;
                }
                *self = LazyCreateState::Created(file);
                if let LazyCreateState::Created(file) = self {
                    Ok(file)
                } else {
                    unreachable!();
                }
            },
            LazyCreateState::Created(file) => Ok(file),
        }
    }
}

impl<H: WriteHeader> LazyCreate<H> {
    pub fn created(&mut self) -> Option<&mut impl io::Write> {
        if let LazyCreateState::Created(file) = &mut self.state {
            Some(file)
        } else {
            None
        }
    }

    pub fn separator<T: std::fmt::Display>(&mut self, separator: T) -> io::Result<()> {
        use std::io::Write;

        if let Some(file) = self.created() {
            write!(file, "{}", separator)
        } else {
            Ok(())
        }
    }
}

impl<H: WriteHeader> io::Write for LazyCreate<H> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.state.create()?.write(data)
    }

    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.state.create()?.write_all(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let LazyCreateState::Created(file) = &mut self.state {
            file.flush()
        } else {
            Ok(())
        }
    }
}

pub enum GenFileName<'a> {
    Extension(&'a str),
    Raw(&'a str)
}

/// There are only two arguments: source and destingation
pub fn get_args() -> (PathBuf, PathBuf, bool) {
    let mut args = std::env::args_os();
    args.next().expect("Not even zeroth argument given");
    let source = args.next().expect("Source not specified.");
    let dest = args.next().expect("Dest not specified.");
    let append = match args.next() {
        Some(ref arg) if *arg == *"--append" => true,
        Some(arg) => panic!("Unknown argument {:?}", arg),
        None => false,
    };

    (source.into(), dest.into(), append)
}

pub fn generate<F: FnMut(&PackageInstance, LazyCreateBuilder) -> io::Result<()>>(gen_file: GenFileName, deps: debcrafter::FileDeps, mut f: F) {
    let (source, dest, append) = get_args();
    let pkg = Package::load(&source);
    let includes = pkg.load_includes(source.parent().unwrap_or(".".as_ref()), deps);

    if pkg.variants.len() == 0 {
        let instance = pkg.instantiate(None, Some(&includes)).expect("Invalid variant");
        let mut dest = PathBuf::from(dest);
        match gen_file {
            GenFileName::Extension(extension) => {
                dest.push(&*instance.name);
                dest.set_extension(extension);
            },
            GenFileName::Raw(file_name) => {
                dest.push(file_name);
            },
        }
        f(&instance, LazyCreateBuilder { path: dest, header: None, append, }).expect("Failed to write dest file");
    } else {
        let dest = PathBuf::from(dest);
        for variant in &pkg.variants {
            let instance = pkg.instantiate(Some(variant), Some(&includes)).expect("Invalid variant");
            let dest = match gen_file {
                GenFileName::Extension(extension) => {
                    let mut dest = dest.join(&*instance.name);
                    dest.set_extension(extension);
                    dest
                },
                GenFileName::Raw(file_name) => {
                    dest.join(file_name)
                },
            };

            f(&instance, LazyCreateBuilder { path: dest, header: None, append, }).expect("Failed to write dest file");
        }
    }
}

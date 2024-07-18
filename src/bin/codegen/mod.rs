//#![allow(dead_code)]
//       ^^^^^^^^^ - prevents reporting false warnings

use void::Void;
use std::io;
use std::path::PathBuf;

pub mod bash;

pub fn paragraph<W: io::Write>(mut dest: W, text: &str) -> io::Result<()> {
    let mut write_dot = false;
    for line in text.split('\n') {
        if !line.is_empty() {
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
                    .truncate(!*append)
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

pub struct LazyWriter<'a, W> {
    writer: W,
    head: &'a [u8],
    tail: &'a [u8],
    was_written: bool,
}

impl<'a, W: io::Write> LazyWriter<'a, W> {
    pub fn new(writer: W, head: &'a [u8], tail: &'a [u8]) -> Self {
        LazyWriter {
            writer,
            head,
            tail,
            was_written: false,
        }
    }

    pub fn finish(mut self) -> io::Result<()> {
        if self.was_written {
            assert!(self.head.is_empty());
            if !self.tail.is_empty() {
                self.writer.write_all(self.tail)
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }
}

impl<'a, W: io::Write> io::Write for LazyWriter<'a, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.head.is_empty() {
            let written = self.writer.write(self.head)?;
            self.head = &self.head[written..];
            self.was_written = true;
        }
        let amount = self.writer.write(bytes)?;
        self.was_written |= amount > 0;
        Ok(amount)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

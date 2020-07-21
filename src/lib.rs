#![feature(type_alias_impl_trait)]

use std::{
    borrow::Cow,
    ffi::{
        CStr,
        NulError,
    },
    fmt::{
        self,
        Write as FmtWrite,
    },
    io::{
        self,
        Write as IoWrite,
    },
    iter,
    ptr,
};

use libc::{
    c_char,
    c_int,
    time_t,
};

pub mod ansi;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const HELP_MESSAGE: &str = "HELP: Ctrl-S = save | Ctrl-Q = quit | Ctrl-F = find";

#[link(name = "c")]
extern "C" {
    fn wcwidth(c: libc::wchar_t) -> c_int;
}

#[derive(Debug)]
pub enum Error {
    IncorrectInvocation,
    NulError(NulError),
    IoError(io::Error),
    FmtError(fmt::Error),
}

impl From<NulError> for Error {
    fn from(err: NulError) -> Self {
        Error::NulError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::IoError(err)
    }
}

impl From<fmt::Error> for Error {
    fn from(err: fmt::Error) -> Self {
        Error::FmtError(err)
    }
}

pub type KiroResult<T> = Result<T, Error>;

#[repr(C)]
pub struct Row {
    idx: c_int,
    size: c_int,
    chars: *mut c_char,
}

#[repr(C)]
pub struct Editor {
    cx: c_int,
    cy: c_int,
    rowoff: c_int,
    coloff: c_int,
    screenrows: c_int,
    screencols: c_int,
    numrows: c_int,
    rawmode: c_int,
    row: *mut Row,
    dirty: c_int,
    filename: *mut c_char,
    statusmsg: [c_char; 80],
    statusmsg_time: time_t,
}

impl Default for Editor {
    fn default() -> Editor {
        Editor {
            cx: 0,
            cy: 0,
            rowoff: 0,
            coloff: 0,
            screenrows: 0,
            screencols: 0,
            numrows: 0,
            rawmode: 0,
            row: ptr::null_mut(),
            dirty: 0,
            filename: ptr::null_mut(),
            statusmsg: [0; 80],
            statusmsg_time: unsafe { libc::time(ptr::null_mut()) },
        }
    }
}

impl Editor {
    pub fn draw(&self) -> KiroResult<()> {
        let mut output = String::new();
        write!(output, "{}{}", ansi::HIDE_CURSOR, ansi::GOTO_TOP_LEFT)?;

        let mut write_line = |s| write!(output, "{}{}{}", s, ansi::CLEAR_REST_OF_LINE, ansi::EOL);

        let empty = self.is_empty();
        let lines = select(empty, self.render_welcome_message(), self.render_buffer())
            .chain(iter::repeat(Self::empty_line()))
            .take(self.screenrows as _);

        for line in lines {
            write_line(line)?;
        }

        write!(
            output,
            "{}{}{}",
            self.render_status_message(),
            self.goto_current_cursor_position(),
            ansi::SHOW_CURSOR,
        )?;
        print!("{}", output);
        std::io::stdout().flush()?;
        Ok(())
    }

    fn render_welcome_message(&self) -> impl Iterator<Item = Cow<str>> {
        let msg = format!("キロ editor -- version {}", VERSION);
        let render_width = render_width(&msg).unwrap_or_else(|| {
            panic!(
                "Could not calculate render width of {:?} -- is the locale set up correctly?",
                msg
            )
        });
        let greeting = format!(
            "~{:^width$}",
            msg,
            width = self.screencols as usize - (render_width - msg.chars().count()) - 1,
        );
        iter::repeat(Self::empty_line())
            .take(self.screenrows as usize / 3)
            .chain(iter::once(greeting.into()))
    }

    fn render_buffer(&self) -> impl Iterator<Item = Cow<str>> {
        self.screen_lines().map(move |line| {
            crop_to(&line, self.coloff as _, self.screencols as _)
                .to_string()
                .into()
        })
    }

    fn render_status_message(&self) -> String {
        let lstatus = format!(
            "{} - {} lines {}",
            unsafe { CStr::from_ptr(self.filename).to_string_lossy() },
            self.numrows,
            if self.dirty != 0 { "(modified)" } else { "" },
        );
        let rstatus = format!("{}/{}", self.rowoff + self.cy + 1, self.numrows,);
        let padding: String = iter::repeat(' ')
            .take(
                self.screencols as usize
                    // TODO: Correctly handle failing `render_width`
                    - render_width(&lstatus).unwrap_or_else(|| lstatus.len())
                    - render_width(&rstatus).unwrap_or_else(|| rstatus.len()),
            )
            .collect();
        format!(
            "{}{}{}{}{}{}{}{}{}",
            ansi::REVERSE,
            lstatus,
            padding,
            rstatus,
            ansi::CLEAR_REST_OF_LINE,
            ansi::EOL,
            ansi::RESET,
            unsafe { CStr::from_ptr(self.statusmsg.as_ptr()).to_string_lossy() },
            ansi::CLEAR_REST_OF_LINE,
        )
    }

    fn screen_lines(&self) -> impl Iterator<Item = Cow<str>> {
        (0..std::cmp::min(self.screenrows, self.numrows - self.rowoff)).map(move |y| {
            let offset = self.rowoff + y;
            let row = unsafe { self.row.offset(offset as isize) };
            unsafe { CStr::from_ptr((*row).chars).to_string_lossy() }
        })
    }

    fn is_empty(&self) -> bool {
        self.numrows == 0
    }

    fn goto_current_cursor_position(&self) -> String {
        // TODO: Tabs and multibyte/double width characters
        ansi::goto_position(self.cx as usize + 1, self.cy as usize + 1)
    }

    fn empty_line() -> Cow<'static, str> {
        "~".into()
    }
}

fn crop_to(s: &str, start: usize, width: usize) -> &str {
    let mut indices = s.chars().scan((0, 0), |(pos, byte_idx), c| {
        let result = Some((*pos, *byte_idx));
        *pos += char_width(c).unwrap();
        *byte_idx += c.len_utf8();
        result
    });
    let start = indices.find(|&(pos, _)| pos >= start).map(|(_, i)| i);
    let end = start.and_then(|start| {
        indices
            .find(|&(pos, _)| pos >= start + width)
            .map(|(_, i)| i)
    });
    match (start, end) {
        (Some(start), Some(end)) => &s[start..end],
        (Some(start), _) => &s[start..],
        (_, Some(end)) => &s[..end],
        _ => "",
    }
}

fn char_width(c: char) -> Option<usize> {
    if c == '\t' {
        // TODO: Move to 4-space tabs
        return Some(8);
    }
    let len = unsafe { wcwidth(c as _) };
    if len < 0 {
        None
    }
    else {
        Some(len as usize)
    }
}

fn render_width(s: &str) -> Option<usize> {
    s.chars()
        .map(char_width)
        .fold(Some(0), |acc, maybe_len| Some(acc? + maybe_len?))
}

struct When<It: Iterator> {
    iter: It,
}

impl<It: Iterator> Iterator for When<It> {
    type Item = It::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

trait IteratorWhen: Iterator {
    type Iter: Iterator;
    fn when(self, b: bool) -> When<Self::Iter>;
}

impl<I> IteratorWhen for I
where
    I: Iterator,
{
    type Iter = impl Iterator<Item = I::Item>;

    fn when(self, b: bool) -> When<Self::Iter> {
        When {
            iter: self.filter(move |_| b),
        }
    }
}

fn select<It1, It2>(b: bool, if_true: It1, if_false: It2) -> impl Iterator<Item = It1::Item>
where
    It1: Iterator,
    It2: Iterator<Item = It1::Item>,
{
    if_true.when(b).chain(if_false.when(!b))
}

#[no_mangle]
pub extern "C" fn editorRefreshScreen() {
    extern "C" {
        static E: Editor;
    }
    let e = unsafe { &E };
    std::panic::catch_unwind(|| e.draw().unwrap()).unwrap_or_else(|err| {
        println!("{:?}", err);
        std::process::exit(1);
    });
}

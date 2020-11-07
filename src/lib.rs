#![feature(assoc_char_funcs)]
#![feature(type_alias_impl_trait)]

use std::{
    borrow::Cow,
    ffi::{
        CStr,
        NulError,
        OsStr,
    },
    fmt::{
        self,
        Write as FmtWrite,
    },
    fs::{
        rename,
        File,
    },
    io::{
        self,
        Write as IoWrite,
    },
    iter,
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr,
    time::{
        Duration,
        Instant,
    },
};

use libc::{
    c_char,
    c_int,
};

pub mod ansi;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const HELP_MESSAGE: &str = "HELP: Ctrl-S = save | Ctrl-Q = quit";

pub const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

#[link(name = "c")]
extern "C" {
    fn wcwidth(c: libc::wchar_t) -> c_int;
}

#[link(name = "kilo", kind = "static")]
extern "C" {
    // FIXME: This warning is a bug, see https://github.com/rust-lang/rust/pull/72700 and
    // https://github.com/rust-lang/rust/pull/74448
    #[allow(improper_ctypes)]
    static mut E: Editor;
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
#[allow(non_camel_case_types)]
pub enum KEY_ACTION {
    KEY_NULL = 0,    // NULL
    CTRL_C = 3,      // Ctrl-c
    CTRL_D = 4,      // Ctrl-d
    CTRL_H = 8,      // Ctrl-h
    TAB = 9,         // Tab
    CTRL_L = 12,     // Ctrl+l
    ENTER = 13,      // Enter
    CTRL_Q = 17,     // Ctrl-q
    CTRL_S = 19,     // Ctrl-s
    CTRL_U = 21,     // Ctrl-u
    ESC = 27,        // Escape
    BACKSPACE = 127, // Backspace
    // The following are just soft codes, not really reported by the
    // terminal directly.
    ARROW_LEFT = 1000,
    ARROW_RIGHT,
    ARROW_UP,
    ARROW_DOWN,
    DEL_KEY,
    HOME_KEY,
    END_KEY,
    PAGE_UP,
    PAGE_DOWN,
}

type Rows = Vec<String>;

#[repr(C)]
pub struct Editor {
    cx: usize,
    cy: usize,
    rowoff: usize,
    coloff: usize,
    screenrows: usize,
    screencols: usize,
    numrows: usize,
    rawmode: usize,
    rows: Box<Rows>,
    dirty: usize,
    filename: *mut c_char,
    status: Box<Status>,
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
            rows: Box::new(Vec::new()),
            dirty: 0,
            filename: ptr::null_mut(),
            status: Box::new(Status::default()),
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
            self.rows.len(),
            if self.dirty != 0 { "(modified)" } else { "" },
        );
        let rstatus = format!("{}/{}", self.rowoff + self.cy + 1, self.rows.len(),);
        let padding: String = iter::repeat(' ')
            .take(
                self.screencols as usize
                    // TODO: Correctly handle failing `render_width`
                    - render_width(&lstatus).unwrap_or_else(|| lstatus.len())
                    - render_width(&rstatus).unwrap_or_else(|| rstatus.len()),
            )
            .collect();
        let statusmsg = if self.status.time.elapsed() <= STATUS_TIMEOUT {
            &self.status.message
        }
        else {
            ""
        };
        format!(
            "{}{}{}{}{}{}{}{}{}",
            ansi::REVERSE,
            lstatus,
            padding,
            rstatus,
            ansi::CLEAR_REST_OF_LINE,
            ansi::EOL,
            ansi::RESET,
            statusmsg,
            ansi::CLEAR_REST_OF_LINE,
        )
    }

    fn screen_lines(&self) -> impl Iterator<Item = Cow<str>> {
        (0..std::cmp::min(self.screenrows, self.rows.len() - self.rowoff)).map(move |y| {
            let offset = self.rowoff + y;
            (&self.rows[offset as usize]).into()
        })
    }

    fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    fn goto_current_cursor_position(&self) -> String {
        // TODO: Tabs and multibyte/double width characters
        ansi::goto_position(self.cx as usize + 1, self.cy as usize + 1)
    }

    fn insert_line(&mut self, idx: usize, line: String) {
        self.rows.insert(idx, line);
        self.dirty += 1;
    }

    fn append_line(&mut self, line: impl Into<String>) {
        self.rows.push(line.into());
    }

    fn insert_char(&mut self, c: char) {
        let filerow = self.filerow();
        let filecol = self.filecol();
        for _ in self.rows.len()..=filerow {
            self.append_line("");
        }
        let row = &mut self.rows[filerow];
        for _ in row.len()..filecol {
            row.push(' ');
        }
        row.insert(filecol, std::char::from_u32(c as _).expect("invalid char"));
        if self.cx == self.screencols - 1 {
            self.coloff += 1;
        }
        else {
            self.cx += 1;
        }
        self.dirty += 1;
    }

    fn insert_newline(&mut self) {
        let filecol = self.filecol();
        let filerow = self.filerow();
        if let Some(row) = self.rows.get_mut(filerow) {
            let cursor_position = filecol.min(row.len());
            let end = row[cursor_position..].into();
            row.replace_range(cursor_position.., "");
            self.insert_line(filerow + 1, end);
        }
        else {
            self.append_line("");
        }
        if self.cy == self.screenrows - 1 {
            self.rowoff += 1;
        }
        else {
            self.cy += 1;
        }
        self.cx = 0;
        self.coloff = 0;
    }

    fn delete_character(&mut self) {
        let filerow = self.filerow();
        let filecol = self.filecol();
        if filerow == 0 && filecol == 0 {
            return;
        }
        if let Some(row) = self.rows.get_mut(filerow) {
            if filecol != 0 {
                row.remove(filecol - 1);
                if self.cx == 0 && self.coloff != 0 {
                    self.coloff -= 1;
                }
                else {
                    self.cx -= 1;
                }
            }
            else {
                let row = self.rows.remove(filerow);
                let filecol = self.rows[filerow - 1].len();
                self.rows[filerow - 1].push_str(&row);
                if self.cy == 0 {
                    self.rowoff -= 1;
                }
                else {
                    self.cy -= 1;
                }
                self.cx = filecol;
                if self.cx >= self.screencols {
                    let shift = self.screencols - self.cx + 1;
                    self.cx -= shift;
                    self.coloff += shift;
                }
            }
        }
        self.dirty += 1;
    }

    fn move_cursor(&mut self, key: KEY_ACTION) {
        let filerow = self.rowoff + self.cy;
        let filecol = self.coloff + self.cx;

        match key {
            KEY_ACTION::ARROW_LEFT =>
                if self.cx == 0 {
                    if self.coloff != 0 {
                        self.coloff -= 1;
                    }
                    else if filerow > 0 {
                        self.cy -= 1;
                        self.cx = self.rows[(filerow - 1) as usize].len() as _;
                        if self.cx > self.screencols - 1 {
                            self.coloff = self.cx - self.screencols + 1;
                            self.cx = self.screencols - 1;
                        }
                    }
                }
                else {
                    self.cx -= 1;
                },
            KEY_ACTION::ARROW_RIGHT => {
                if filerow < self.rows.len() && filecol < self.rows[filerow].len() {
                    if self.cx == self.screencols - 1 {
                        self.coloff += 1;
                    }
                    else {
                        self.cx += 1;
                    }
                }
                else if filerow < self.rows.len() && filecol == self.rows[filerow].len() {
                    self.cx = 0;
                    self.coloff = 0;
                    if self.cy == self.screenrows - 1 {
                        self.rowoff += 1;
                    }
                    else {
                        self.cy += 1;
                    }
                }
            }
            KEY_ACTION::ARROW_UP =>
                if self.cy == 0 {
                    if self.rowoff != 0 {
                        self.rowoff -= 1;
                    }
                }
                else {
                    self.cy -= 1;
                },
            KEY_ACTION::ARROW_DOWN =>
                if filerow < self.rows.len() {
                    if self.cy == self.screenrows - 1 {
                        self.rowoff += 1;
                    }
                    else {
                        self.cy += 1;
                    }
                },
            _ => unreachable!(),
        }
        let filerow = self.rowoff + self.cy;
        let filecol = self.coloff + self.cx;
        let rowlen = self.rows.get(filerow).map_or(0, String::len);
        if filecol > rowlen {
            self.coloff = std::cmp::min(self.coloff, rowlen);
            self.cx = rowlen - self.coloff;
        }
    }

    fn save(&mut self) -> KiroResult<u64> {
        if self.filename.is_null() {
            panic!("null filename");
        }
        let path = std::fs::canonicalize(OsStr::from_bytes(
            &unsafe { CStr::from_ptr(self.filename as _) }.to_bytes(),
        ))?;
        let file_name = {
            let mut file_name = path.file_name().unwrap().to_os_string();
            file_name.push("~kirosave");
            file_name
        };

        let temp_file_path = {
            let mut path = path.clone();
            path.set_file_name(file_name);
            path
        };
        let bytes_written = {
            let mut file = File::create(temp_file_path.clone())?;
            for row in &*self.rows {
                writeln!(file, "{}", row)?;
            }
            file.metadata()?.len()
        };
        rename(temp_file_path, path)?;

        self.dirty = 0;
        Ok(bytes_written)
    }

    pub fn set_status(&mut self, message: String) {
        self.status = Box::new(Status::new(message));
    }

    fn filerow(&self) -> usize {
        self.rowoff + self.cy
    }

    fn filecol(&self) -> usize {
        self.coloff + self.cx
    }

    fn filename(&self) -> impl AsRef<Path> {
        OsStr::from_bytes(unsafe { CStr::from_ptr(self.filename) }.to_bytes())
    }

    fn empty_line() -> Cow<'static, str> {
        "~".into()
    }
}

struct Status {
    message: String,
    time: Instant,
}

impl Status {
    fn new(message: String) -> Status {
        Status {
            message,
            time: Instant::now(),
        }
    }
}

impl Default for Status {
    fn default() -> Status {
        Status::new("".into())
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

fn instance() -> &'static mut Editor {
    unsafe { &mut E }
}

#[no_mangle]
pub extern "C" fn editorRefreshScreen() {
    instance().draw().unwrap()
}

#[no_mangle]
pub extern "C" fn editorClearStatusMessage() {
    instance().set_status(String::new());
}

/// # Safety
///
/// `error` must be a null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn editorSetStatusMessageSearch(query: *const c_char) {
    if query.is_null() {
        return;
    }
    instance().set_status(format!(
        "Search: {} (Use Esc/Arrows/Return)",
        CStr::from_ptr(query).to_string_lossy()
    ));
}

#[no_mangle]
pub extern "C" fn editorSetStatusMessageQuit(count: c_int) {
    instance().set_status(format!(
        "WARNING!!! File has unsaved changes. Press Ctrl-Q {} more times to quit.",
        count
    ));
}

/// # Safety
///
/// `line` must be a null-terminated string. It is safe to pass `NULL` for
/// `line`, but this crashes the editor.
#[no_mangle]
pub unsafe extern "C" fn editorInsertRow(
    _numrows: c_int,
    line: *const c_char,
    _len: libc::ssize_t,
) {
    assert!(!line.is_null());
    instance().append_line(CStr::from_ptr(line).to_string_lossy());
}

#[no_mangle]
pub extern "C" fn editorInsertNewline() {
    instance().insert_newline();
}

#[no_mangle]
pub extern "C" fn editorSave() {
    match instance().save() {
        Ok(bytes_written) =>
            instance().set_status(format!("{} bytes written to disk", bytes_written)),
        Err(err) => {
            instance().set_status(format!(
                "Could not write to file `{}`: {:?}",
                instance().filename().as_ref().display(),
                err
            ));
        }
    };
}

#[no_mangle]
pub extern "C" fn editorDelChar() {
    instance().delete_character();
}

#[no_mangle]
pub extern "C" fn editorMoveCursor(key: KEY_ACTION) {
    instance().move_cursor(key);
}

#[no_mangle]
pub extern "C" fn editorInsertChar(c: c_int) {
    instance().insert_char(char::from_u32(c as _).expect("invalid char"));
}

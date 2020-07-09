use std::ffi::{CString, NulError};
use std::io;

use libc::{c_char, c_int};

use kiro::Editor;

#[link(name = "kilo", kind = "static")]
extern "C" {
    fn editorSelectSyntaxHighlight(filename: *mut c_char);
    fn editorOpen(filename: *mut c_char);
    fn enableRawMode(fd: c_int);
    fn editorSetStatusMessage(msg: *const c_char);
    fn editorRefreshScreen();
    fn editorProcessKeypress(fd: c_int);
    fn updateWindowSize();
    fn handleSigWinCh(_: c_int);

    static mut E: Editor;
}

#[derive(Debug)]
enum KiroErr {
    IncorrectInvocation,
    NulError(NulError),
    IoError(io::Error),
}

impl From<NulError> for KiroErr {
    fn from(err: NulError) -> Self {
        KiroErr::NulError(err)
    }
}

type KiroResult<T> = Result<T, KiroErr>;

fn main() -> KiroResult<()> {
    let mut filename = std::env::args()
        .nth(1)
        .ok_or(KiroErr::IncorrectInvocation)?;

    unsafe {
        E = Editor::default();
        updateWindowSize();
        let result = libc::signal(libc::SIGWINCH, handleSigWinCh as _);
        if result == libc::SIG_ERR {
            return Err(KiroErr::IoError(io::Error::last_os_error()));
        }
        editorSelectSyntaxHighlight(filename.as_mut_ptr() as _);
        editorOpen(filename.as_mut_ptr() as _);
        enableRawMode(libc::STDIN_FILENO);
        editorSetStatusMessage(
            CString::new("HELP: Ctrl-S = save | Ctrl-Q = quit | Ctrl-F = find")?.as_ptr() as _,
        );
        loop {
            editorRefreshScreen();
            editorProcessKeypress(libc::STDIN_FILENO);
        }
    }
}

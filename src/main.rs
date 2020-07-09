use std::ffi::{CString, NulError};

#[link(name = "kilo", kind = "static")]
extern "C" {
    fn initEditor();
    fn editorSelectSyntaxHighlight(filename: *mut libc::c_char);
    fn editorOpen(filename: *mut libc::c_char);
    fn enableRawMode(fd: libc::c_int);
    fn editorSetStatusMessage(msg: *const libc::c_char);
    fn editorRefreshScreen();
    fn editorProcessKeypress(fd: libc::c_int);
}

#[derive(Debug)]
enum KiroErr {
    IncorrectInvocation,
    NulError(NulError),
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
        initEditor();
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

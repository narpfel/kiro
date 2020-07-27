use std::{
    ffi::CString,
    io,
};

use libc::{
    c_char,
    c_int,
};

use kiro::{
    Editor,
    KiroResult,
};

#[link(name = "kilo", kind = "static")]
extern "C" {
    fn editorOpen(filename: *mut c_char);
    fn enableRawMode(fd: c_int);
    fn editorProcessKeypress(fd: c_int);
    fn updateWindowSize();
    fn handleSigWinCh(_: c_int);

    // FIXME: This warning is a bug, see https://github.com/rust-lang/rust/pull/72700 and
    // https://github.com/rust-lang/rust/pull/74448
    #[allow(improper_ctypes)]
    static mut E: Editor;
}

extern "C" fn restore_primary_buffer() {
    println!("{}", kiro::ansi::PRIMARY_BUFFER);
}

fn main() -> KiroResult<()> {
    let mut filename = std::env::args()
        .nth(1)
        .ok_or(kiro::Error::IncorrectInvocation)?;

    println!("{}", kiro::ansi::ALTERNATIVE_BUFFER);
    unsafe {
        let locale = CString::new("")?;
        libc::setlocale(libc::LC_CTYPE, locale.as_ptr() as _);
        E = Editor::default();
        updateWindowSize();
        let result = libc::signal(libc::SIGWINCH, handleSigWinCh as _);
        if result == libc::SIG_ERR {
            return Err(kiro::Error::IoError(io::Error::last_os_error()));
        }
        libc::atexit(restore_primary_buffer);
        editorOpen(filename.as_mut_ptr() as _);
        enableRawMode(libc::STDIN_FILENO);
        E.set_status(kiro::HELP_MESSAGE.into());
        loop {
            E.draw()?;
            editorProcessKeypress(libc::STDIN_FILENO);
        }
    }
}

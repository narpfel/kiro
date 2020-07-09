use libc::{c_char, c_int, c_uchar, time_t};

#[repr(C)]
pub struct Syntax {
    filematch: *mut *mut c_char,
    keywords: *mut *mut c_char,
    singleline_comment_start: [c_char; 2],
    multiline_comment_start: [c_char; 3],
    multiline_comment_end: [c_char; 3],
    flags: c_int,
}

#[repr(C)]
pub struct Row {
    idx: c_int,
    size: c_int,
    rsize: c_int,
    chars: *mut c_char,
    render: *mut c_char,
    hl: *mut c_uchar,
    hl_oc: c_int,
}

#[repr(C)]
pub struct Colour {
    r: c_int,
    g: c_int,
    b: c_int,
}

#[repr(C)]
pub struct Config {
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
    syntax: *mut Syntax,
}
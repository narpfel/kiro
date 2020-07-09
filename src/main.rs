#[link(name = "kilo", kind = "static")]
extern "C" {
    fn cmain(argc: libc::c_int, argv: *mut *mut libc::c_char) -> libc::c_int;
}

fn main() {
    let mut args: Vec<_> = std::env::args().collect();
    let mut c_args: Vec<_> = args.iter_mut().map(|s| s.as_mut_ptr()).collect();
    let result = unsafe { cmain(c_args.len() as _, c_args.as_mut_ptr() as _) };
    std::process::exit(result);
}

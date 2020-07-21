pub const HIDE_CURSOR: &str = "\x1B[?25l";
pub const SHOW_CURSOR: &str = "\x1B[?25h";
pub const GOTO_TOP_LEFT: &str = "\x1B[H";
pub const CLEAR_REST_OF_LINE: &str = "\x1B[0K";
pub const ALTERNATIVE_BUFFER: &str = "\x1B[?1049h";
pub const PRIMARY_BUFFER: &str = "\x1B[?1049l";
pub const REVERSE: &str = "\x1B[7m";
pub const RESET: &str = "\x1B[0m";
pub const EOL: &str = "\r\n";

pub fn goto_position(x: usize, y: usize) -> String {
    format!("\x1B[{y};{x}H", x = x, y = y)
}

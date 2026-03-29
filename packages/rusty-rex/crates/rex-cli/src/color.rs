/// ANSI color helpers. Only emit codes when stdout is a terminal.

use std::sync::OnceLock;

static USE_COLOR: OnceLock<bool> = OnceLock::new();

fn use_color() -> bool {
    *USE_COLOR.get_or_init(|| {
        // Check NO_COLOR env var (https://no-color.org)
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        // Check if stderr is a terminal (we print colored output to stderr)
        unsafe { isatty(2) != 0 }
    })
}

unsafe extern "C" {
    fn isatty(fd: i32) -> i32;
}

pub fn isatty_fd(fd: i32) -> bool {
    unsafe { isatty(fd) != 0 }
}

fn ansi(code: &str, s: &str) -> String {
    if use_color() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn red(s: &str) -> String { ansi("31", s) }
pub fn green(s: &str) -> String { ansi("32", s) }
pub fn yellow(s: &str) -> String { ansi("33", s) }
pub fn cyan(s: &str) -> String { ansi("36", s) }
pub fn magenta(s: &str) -> String { ansi("35", s) }
pub fn dim(s: &str) -> String { ansi("2", s) }

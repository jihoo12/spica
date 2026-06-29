mod editor;
mod script;

use libc::{ioctl, winsize, ECHO, ICANON, STDIN_FILENO, STDOUT_FILENO, TCSAFLUSH, TIOCGWINSZ, tcgetattr, tcsetattr, termios};
use std::io::{self, Read, Write};
use std::mem;

use editor::{EditorConfig, Mode};

// --- Terminal Raw Mode Handling ---
struct RawMode {
    orig_termios: termios,
}

impl RawMode {
    fn enable() -> Self {
        unsafe {
            let mut raw: termios = mem::zeroed();
            if tcgetattr(STDIN_FILENO, &mut raw) == -1 {
                panic!("tcgetattr failed");
            }
            let orig_termios = raw;
            raw.c_lflag &= !(ECHO | ICANON);
            if tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw) == -1 {
                panic!("tcsetattr failed");
            }
            RawMode { orig_termios }
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        unsafe {
            tcsetattr(STDIN_FILENO, TCSAFLUSH, &self.orig_termios);
        }
    }
}

// --- Helper Functions ---
fn get_terminal_size() -> (u16, u16) {
    unsafe {
        let mut ws: winsize = std::mem::zeroed();
        if ioctl(STDOUT_FILENO, TIOCGWINSZ, &mut ws) == -1 {
            return (80, 24);
        }
        (ws.ws_col, ws.ws_row)
    }
}

fn draw_screen(config: &EditorConfig) {
    let visible_rows = (config.screen_rows - 1) as usize;
    let visible_cols = config.screen_cols as usize;

    for y in 0..visible_rows {
        let file_row_idx = y + config.row_offset;
        print!("\x1b[K");

        if file_row_idx < config.buffer.rows.len() {
            let row_content = &config.buffer.rows[file_row_idx].content;

            if row_content.len() > config.col_offset {
                let mut line = row_content[config.col_offset..].to_string();
                line.truncate(visible_cols);
                print!("{}\r\n", line);
            } else {
                print!("\r\n");
            }
        } else {
            print!("~\r\n");
        }
    }
}

fn draw_status_bar(config: &EditorConfig) {
    print!("\x1b[{};1H\x1b[K", config.screen_rows);
    if config.mode == Mode::Command {
        print!(":{}", config.command_buffer);
    } else {
        let mode_str = match config.mode {
            Mode::Normal => "-- NORMAL --",
            Mode::Insert => "-- INSERT --",
            _ => "",
        };
        let status = format!("{} | Pos: {},{} | {}", mode_str, config.cx, config.cy, config.status_msg);
        print!("\x1b[7m{:width$}\x1b[m", status, width = config.screen_cols as usize);
    }
}

fn refresh_screen(config: &mut EditorConfig) {
    config.scroll();

    print!("\x1b[?25l\x1b[H");
    draw_screen(config);
    draw_status_bar(config);

    let screen_y = config.cy - config.row_offset as u16;
    let screen_x = config.cx - config.col_offset as u16;

    print!("\x1b[{};{}H\x1b[?25h", screen_y + 1, screen_x + 1);
    io::stdout().flush().unwrap();
}

fn main() {
    let _raw_mode = RawMode::enable();
    let term_size = get_terminal_size();
    let mut config = EditorConfig::new(term_size);

    // Initialize pi-lisp scripting
    script::init(&mut config);

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let filename = args[1].clone();
        if config.buffer.open(&filename).is_ok() {
            config.filename = Some(filename.clone());
            config.status_msg = format!("Opened: {}", filename);
        } else {
            config.filename = Some(filename.clone());
            config.status_msg = format!("New file: {}", filename);
        }
    }

    print!("\x1b[2J");

    loop {
        refresh_screen(&mut config);

        let mut buf = [0; 1];
        if io::stdin().read(&mut buf).is_ok() {
            let c = buf[0] as char;

            if !config.handle_keypress(c) {
                print!("\x1b[2J\x1b[H");
                io::stdout().flush().unwrap();
                break;
            }
        }
    }
}

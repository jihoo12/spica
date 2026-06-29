use std::io::Write;
use std::fs::File;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
}

pub struct Row {
    pub content: String,
}

impl Row {
    pub fn new(s: String) -> Self {
        Row { content: s }
    }
    pub fn insert_char(&mut self, at: usize, c: char) {
        if at >= self.content.len() {
            self.content.push(c);
        } else {
            self.content.insert(at, c);
        }
    }
    pub fn delete_char(&mut self, at: usize) {
        if at < self.content.len() {
            self.content.remove(at);
        }
    }
}

pub struct EditorBuffer {
    pub rows: Vec<Row>,
}

impl EditorBuffer {
    pub fn new() -> Self {
        EditorBuffer {
            rows: vec![Row::new(String::new())],
        }
    }
    pub fn rows_to_string(&self) -> String {
        self.rows.iter()
            .map(|r| r.content.as_str())
            .collect::<Vec<&str>>()
            .join("\n")
    }
    pub fn open(&mut self, filename: &str) -> std::io::Result<()> {
        let content = std::fs::read_to_string(filename)?;
        self.rows.clear();
        for line in content.lines() {
            self.rows.push(Row::new(line.to_string()));
        }
        if self.rows.is_empty() {
            self.rows.push(Row::new(String::new()));
        }
        Ok(())
    }
}

pub struct EditorConfig {
    pub cx: u16,
    pub cy: u16,
    pub screen_cols: u16,
    pub screen_rows: u16,
    pub row_offset: usize,
    pub col_offset: usize,
    pub mode: Mode,
    pub buffer: EditorBuffer,
    pub command_buffer: String,
    pub status_msg: String,
    pub filename: Option<String>,
}

impl EditorConfig {
    pub fn new(term_size: (u16, u16)) -> Self {
        let (cols, rows) = term_size;
        EditorConfig {
            cx: 0,
            cy: 0,
            screen_cols: cols,
            screen_rows: rows,
            row_offset: 0,
            col_offset: 0,
            mode: Mode::Normal,
            buffer: EditorBuffer::new(),
            command_buffer: String::new(),
            status_msg: String::from("WELCOME! :q to quit"),
            filename: None,
        }
    }

    pub fn move_cursor(&mut self, key: char) {
        let row_count = self.buffer.rows.len();
        match key {
            'h' => if self.cx > 0 { self.cx -= 1 },
            'j' => if (self.cy as usize) < row_count - 1 { self.cy += 1 },
            'k' => if self.cy > 0 { self.cy -= 1 },
            'l' => {
                let cur_row_len = self.buffer.rows[self.cy as usize].content.len() as u16;
                if self.cx < cur_row_len { self.cx += 1; }
            }
            _ => {}
        }
        let cur_row_len = self.buffer.rows[self.cy as usize].content.len() as u16;
        if self.cx > cur_row_len { self.cx = cur_row_len; }
    }

    pub fn insert_char(&mut self, c: char) {
        self.buffer.rows[self.cy as usize].insert_char(self.cx as usize, c);
        self.cx += 1;
    }

    pub fn delete_char(&mut self) {
        if self.cx == 0 && self.cy == 0 { return; }
        if self.cx > 0 {
            self.buffer.rows[self.cy as usize].delete_char(self.cx as usize - 1);
            self.cx -= 1;
        } else {
            let current_row_content = self.buffer.rows.remove(self.cy as usize).content;
            self.cy -= 1;
            let prev_row = &mut self.buffer.rows[self.cy as usize];
            self.cx = prev_row.content.len() as u16;
            prev_row.content.push_str(&current_row_content);
        }
    }

    pub fn handle_keypress(&mut self, key: char) -> bool {
        match self.mode {
            Mode::Normal => match key {
                'i' => self.mode = Mode::Insert,
                ':' => {
                    self.mode = Mode::Command;
                    self.command_buffer.clear();
                }
                'h' | 'j' | 'k' | 'l' => self.move_cursor(key),
                _ => {
                    if crate::script::dispatch_key("normal", key) {
                        return true;
                    }
                }
            },
            Mode::Insert => match key {
                '\x1b' => self.mode = Mode::Normal,
                '\r' | '\n' => {
                    let remaining = self.buffer.rows[self.cy as usize].content.split_off(self.cx as usize);
                    self.buffer.rows.insert(self.cy as usize + 1, Row::new(remaining));
                    self.cy += 1;
                    self.cx = 0;
                }
                '\x7f' | '\x08' => self.delete_char(),
                c if !c.is_control() => self.insert_char(c),
                _ => {
                    crate::script::dispatch_key("insert", key);
                }
            },
            Mode::Command => match key {
                '\x1b' => self.mode = Mode::Normal,
                '\r' | '\n' => return self.execute_command(),
                '\x7f' | '\x08' => { self.command_buffer.pop(); }
                c if !c.is_control() => self.command_buffer.push(c),
                _ => {
                    crate::script::dispatch_key("command", key);
                }
            },
        }
        true
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        crate::script::trigger_hook("before-save");
        let path = match &self.filename {
            Some(name) => name.clone(),
            None => {
                self.status_msg = "No file name! Use :w <filename> (TBD)".into();
                crate::script::trigger_hook("after-save");
                return Ok(());
            }
        };
        let content = self.buffer.rows_to_string();
        let mut file = File::create(&path)?;
        file.write_all(content.as_bytes())?;
        self.status_msg = format!("Saved to {}", path);
        crate::script::trigger_hook("after-save");
        Ok(())
    }

    pub fn execute_command(&mut self) -> bool {
        let cmd = self.command_buffer.as_str();
        let mut should_continue = true;
        match cmd {
            "w" => match self.save() {
                Ok(_) => self.status_msg = "Saved".into(),
                Err(e) => self.status_msg = format!("Error: {}", e),
            },
            "q" => should_continue = false,
            "wq" => {
                let _ = self.save();
                should_continue = false;
            },
            _ if cmd.starts_with("pi ") => {
                let src = cmd.trim_start_matches("pi ");
                match crate::script::eval_string(src) {
                    Ok(result) => self.status_msg = format!("=> {}", result),
                    Err(e) => self.status_msg = format!("pi error: {}", e),
                }
            }
            _ => {
                // Try user-defined commands from the plugin system
                if !crate::script::dispatch_command(cmd) {
                    self.status_msg = format!("Unknown: {}", cmd);
                }
            }
        }
        self.mode = Mode::Normal;
        self.command_buffer.clear();
        should_continue
    }
    pub fn scroll(&mut self) {
        let visible_rows = (self.screen_rows - 1) as usize;
        let visible_cols = self.screen_cols as usize;

        if (self.cy as usize) < self.row_offset {
            self.row_offset = self.cy as usize;
        }
        if (self.cy as usize) >= self.row_offset + visible_rows {
            self.row_offset = (self.cy as usize) - visible_rows + 1;
        }
        if (self.cx as usize) < self.col_offset {
            self.col_offset = self.cx as usize;
        }
        if (self.cx as usize) >= self.col_offset + visible_cols {
            self.col_offset = (self.cx as usize) - visible_cols + 1;
        }
    }
}

use std::io::Write;
use std::fs::File;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
}

#[derive(Clone, PartialEq)]
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

#[derive(Clone, PartialEq)]
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
    // Undo
    pub undo_stack: Vec<(EditorBuffer, u16, u16)>,
    pub insert_start_state: Option<(EditorBuffer, u16, u16)>,
    // Yank
    pub yank_buffer: Option<String>,
    pub pending_operator: Option<char>,
    // Line numbers
    pub show_line_numbers: bool,
    // Search
    pub search_query: String,
    pub search_results: Vec<(usize, usize)>,
    pub search_idx: usize,
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
            undo_stack: Vec::new(),
            insert_start_state: None,
            yank_buffer: None,
            pending_operator: None,
            show_line_numbers: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_idx: 0,
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

    pub fn delete_char_at(&mut self) {
        let row_len = self.buffer.rows[self.cy as usize].content.len();
        if row_len == 0 || (self.cx as usize) >= row_len { return; }
        self.save_undo_state();
        self.buffer.rows[self.cy as usize].delete_char(self.cx as usize);
        self.status_msg = "Deleted char".into();
    }

    pub fn delete_line(&mut self) {
        self.save_undo_state();
        let row_count = self.buffer.rows.len();
        if row_count <= 1 {
            self.buffer.rows[0].content.clear();
            self.cx = 0;
            self.status_msg = "Cleared last line".into();
            return;
        }
        let yanked = self.buffer.rows[self.cy as usize].content.clone();
        self.yank_buffer = Some(yanked);
        self.buffer.rows.remove(self.cy as usize);
        if self.cy as usize >= self.buffer.rows.len() {
            self.cy -= 1;
        }
        let cur_row_len = self.buffer.rows[self.cy as usize].content.len() as u16;
        if self.cx > cur_row_len {
            self.cx = cur_row_len;
        }
        self.status_msg = "Deleted line".into();
    }

    pub fn yank_line(&mut self) {
        self.yank_buffer = Some(self.buffer.rows[self.cy as usize].content.clone());
        self.status_msg = "Yanked line".into();
    }

    pub fn paste_below(&mut self) {
        let content = match &self.yank_buffer {
            Some(c) => c.clone(),
            None => { self.status_msg = "Nothing to paste".into(); return; }
        };
        self.save_undo_state();
        self.buffer.rows.insert(self.cy as usize + 1, Row::new(content));
        self.cy += 1;
        self.cx = 0;
        self.status_msg = "Pasted".into();
    }

    pub fn paste_above(&mut self) {
        let content = match &self.yank_buffer {
            Some(c) => c.clone(),
            None => { self.status_msg = "Nothing to paste".into(); return; }
        };
        self.save_undo_state();
        self.buffer.rows.insert(self.cy as usize, Row::new(content));
        self.cx = 0;
        self.status_msg = "Pasted above".into();
    }

    pub fn undo(&mut self) {
        if let Some((buf, cx, cy)) = self.undo_stack.pop() {
            self.buffer = buf;
            self.cx = cx;
            self.cy = cy;
            self.status_msg = "Undone".into();
        } else {
            self.status_msg = "Nothing to undo".into();
        }
    }

    pub fn search(&mut self, query: &str) {
        self.search_query = query.to_string();
        self.search_results.clear();
        if query.is_empty() {
            self.status_msg = "Empty search".into();
            return;
        }
        for (i, row) in self.buffer.rows.iter().enumerate() {
            let mut start = 0;
            while let Some(pos) = row.content[start..].find(query) {
                self.search_results.push((i, start + pos));
                start += pos + 1;
            }
        }
        if self.search_results.is_empty() {
            self.status_msg = format!("No matches: {}", query);
        } else {
            self.search_idx = 0;
            let (row, col) = self.search_results[0];
            self.cy = row as u16;
            self.cx = col as u16;
            self.status_msg = format!("Search: {} ({}/{})", query, 1, self.search_results.len());
        }
    }

    pub fn search_next(&mut self) {
        if self.search_results.is_empty() {
            self.status_msg = "No search results".into();
            return;
        }
        self.search_idx = (self.search_idx + 1) % self.search_results.len();
        let (row, col) = self.search_results[self.search_idx];
        self.cy = row as u16;
        self.cx = col as u16;
        self.status_msg = format!("({}/{})", self.search_idx + 1, self.search_results.len());
    }

    pub fn search_prev(&mut self) {
        if self.search_results.is_empty() {
            self.status_msg = "No search results".into();
            return;
        }
        if self.search_idx == 0 {
            self.search_idx = self.search_results.len() - 1;
        } else {
            self.search_idx -= 1;
        }
        let (row, col) = self.search_results[self.search_idx];
        self.cy = row as u16;
        self.cx = col as u16;
        self.status_msg = format!("({}/{})", self.search_idx + 1, self.search_results.len());
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

    fn save_undo_state(&mut self) {
        self.undo_stack.push((self.buffer.clone(), self.cx, self.cy));
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }

    pub fn handle_keypress(&mut self, key: char) -> bool {
        match self.mode {
            Mode::Normal => match key {
                'i' => {
                    self.insert_start_state = Some((self.buffer.clone(), self.cx, self.cy));
                    self.mode = Mode::Insert;
                }
                ':' => {
                    self.mode = Mode::Command;
                    self.command_buffer.clear();
                }
                '/' => {
                    self.mode = Mode::Command;
                    self.command_buffer = "/".to_string();
                }
                'h' | 'j' | 'k' | 'l' => {
                    self.pending_operator = None;
                    self.move_cursor(key);
                }
                'x' => {
                    self.pending_operator = None;
                    self.delete_char_at();
                }
                'd' => {
                    if self.pending_operator == Some('d') {
                        self.pending_operator = None;
                        self.delete_line();
                    } else {
                        self.pending_operator = Some('d');
                    }
                }
                'y' => {
                    if self.pending_operator == Some('y') {
                        self.pending_operator = None;
                        self.yank_line();
                    } else {
                        self.pending_operator = Some('y');
                    }
                }
                'p' => {
                    self.pending_operator = None;
                    self.paste_below();
                }
                'P' => {
                    self.pending_operator = None;
                    self.paste_above();
                }
                'u' => {
                    self.pending_operator = None;
                    self.undo();
                }
                'n' => {
                    self.pending_operator = None;
                    self.search_next();
                }
                'N' => {
                    self.pending_operator = None;
                    self.search_prev();
                }
                _ => {
                    self.pending_operator = None;
                    if crate::script::dispatch_key("normal", key) {
                        return true;
                    }
                }
            },
            Mode::Insert => match key {
                '\x1b' => {
                    if let Some((buf, cx, cy)) = self.insert_start_state.take() {
                        if self.buffer != buf {
                            self.undo_stack.push((buf, cx, cy));
                            if self.undo_stack.len() > 100 {
                                self.undo_stack.remove(0);
                            }
                        }
                    }
                    self.mode = Mode::Normal;
                }
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
                self.status_msg = "No filename! Use :w <filename>".into();
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "no filename"));
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
        let cmd = self.command_buffer.clone();
        let mut should_continue = true;
        match cmd.as_str() {
            "w" => match self.save() {
                Ok(_) => self.status_msg = "Saved".into(),
                Err(e) => self.status_msg = format!("Error: {}", e),
            },
            "q" => should_continue = false,
            "q!" => should_continue = false,
            "wq" => match self.save() {
                Ok(_) => should_continue = false,
                Err(e) => self.status_msg = format!("Error: {}", e),
            },
            _ if cmd.starts_with("wq ") => {
                let filename = cmd[3..].trim().to_string();
                self.filename = Some(filename);
                match self.save() {
                    Ok(_) => should_continue = false,
                    Err(e) => self.status_msg = format!("Error: {}", e),
                }
            },
            _ if cmd.starts_with("w ") => {
                let filename = cmd[2..].trim().to_string();
                self.filename = Some(filename);
                match self.save() {
                    Ok(_) => {},
                    Err(e) => self.status_msg = format!("Error: {}", e),
                }
            },
            _ if cmd.starts_with("pi ") => {
                let src = cmd.trim_start_matches("pi ");
                match crate::script::eval_string(src) {
                    Ok(result) => self.status_msg = format!("=> {}", result),
                    Err(e) => self.status_msg = format!("pi error: {}", e),
                }
            }
            "help" => {
                self.status_msg = "Commands: w q q! wq pi <code> ln /<search> help".into();
            }
            "ln" => {
                self.show_line_numbers = !self.show_line_numbers;
                self.status_msg = if self.show_line_numbers { "Line numbers: on" } else { "Line numbers: off" }.into();
            }
            _ if cmd.starts_with('/') => {
                let query = &cmd[1..];
                self.search(query);
            }
            _ => {
                // Try user-defined commands from the plugin system
                if !crate::script::dispatch_command(&cmd) {
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
        let ln_width = if self.show_line_numbers {
            (self.buffer.rows.len().to_string().len()).max(3) + 1
        } else {
            0
        };
        let visible_cols = (self.screen_cols as usize).saturating_sub(ln_width).max(1);

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

## Rust Text editor
A minimalist, modal text editor built from scratch in Rust. This project demonstrates low-level terminal manipulation using libc to create a "Vim-like" editing experience directly in the console.

## 🚀 Features
- Modal Editing: Supports NORMAL, INSERT, and COMMAND modes.

- Raw Mode Handling: Manages terminal states manually using termios for precise input control.

- Dynamic Viewport:

- Vertical Scrolling: Handles files longer than the terminal screen.

- Horizontal Scrolling: Handles long lines that exceed the terminal width.

- File I/O: Ability to open existing files via command-line arguments and save changes using commands.

- Status Bar: Real-time feedback on current mode, cursor position, and system messages.

## 🛠 Architecture
The editor is built on three core pillars:
- Terminal Raw Mode: Uses libc to disable ICANON (canonical mode) and ECHO flags. This allows the program to read byte-by-byte input without waiting for the user to press Enter.

- Editor State (EditorConfig): Centralizes the cursor position ($cx, cy$), the text buffer, scrolling offsets, and the current mode.

- The Render Loop:

    - Process Input: Captures keypresses and updates the state.

    - Update Viewport: Calculates scrolling offsets based on cursor movement.

    - Draw: Uses ANSI escape sequences to clear the screen and redraw the buffer.

## ⌨️ Controls & Modes
Normal Mode (Default)
Used for navigation and entering commands.

- i: Switch to Insert Mode.

- :: Switch to Command Mode.

- h, j, k, l: Move cursor (Left, Down, Up, Right).

Insert Mode
Used for typing text.

- Esc: Return to Normal Mode.

- Backspace: Delete characters.

- Enter: Break lines.

Command Mode
Triggered by :, used for file operations.

- w: Save the current buffer.

- q: Quit the editor.

- wq: Save and quit.

- Esc: Cancel command.

📥 Installation

Prerequisites

- Rust: Ensure you have cargo and rustc installed.

- Environment: Linux or macOS (uses libc for Unix-based terminal control).
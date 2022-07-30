use clap::{App, Arg};
use std::cmp::{max, min};
use std::ffi::OsStr;
use std::fs;
use std::io::{stdin, stdout, Write};
use std::path;
use termion::clear;
use termion::cursor;
use termion::event::{Event, Key};
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cursor {
    row: usize,
    column: usize,
}

struct EditerState {
    buffer: Vec<Vec<char>>,
    cursor: Cursor,
    row_offset: usize,
    path: Option<path::PathBuf>,
}

impl Default for EditerState {
    fn default() -> Self {
        Self {
            buffer: vec![Vec::new()],
            cursor: Cursor { row: 0, column: 0 },
            row_offset: 0,
            path: None,
        }
    }
}

impl EditerState {
    fn open(&mut self, path: &path::Path) {
        self.buffer = fs::read_to_string(path)
            .ok()
            .map(|s| {
                let buffer: Vec<Vec<char>> = s
                    .lines()
                    .map(|line| line.trim_end().chars().collect())
                    .collect();
                if buffer.is_empty() {
                    vec![Vec::new()]
                } else {
                    buffer
                }
            })
            .unwrap_or_else(|| vec![Vec::new()]);

        self.path = Some(path.into());
        self.cursor = Cursor { row: 0, column: 0 };
        self.row_offset = 0;
    }

    fn terminal_size() -> (usize, usize) {
        let (rows, cols) = termion::terminal_size().unwrap();
        (rows as usize, cols as usize)
    }

    fn draw<T: Write>(&self, out: &mut T) {
        let (rows, cols) = Self::terminal_size();

        write!(out, "{}", clear::All);
        write!(out, "{}", cursor::Goto(1, 1));

        // 画面上の行、列
        let mut row = 0;
        let mut col = 0;

        let mut display_cursor: Option<(usize, usize)> = None;

        'outer: for i in self.row_offset..self.buffer.len() {
            for j in 0..=self.buffer[i].len() {
                if self.cursor == (Cursor { row: i, column: j }) {
                    // 画面上のカーソルの位置がわかった
                    display_cursor = Some((row, col));
                }

                if let Some(c) = self.buffer[i].get(j) {
                    let width = c.width().unwrap_or(0);
                    if col + width >= cols {
                        row += 1;
                        col = 0;
                        if row >= rows {
                            break 'outer;
                        } else {
                            write!(out, "\r\n");
                        }
                    }
                    write!(out, "{}", c);
                    col += width;
                }
            }
            row += 1;
            col = 0;
            if row >= rows {
                break;
            } else {
                // 最後の行の最後では改行すると1行ずれてしまうのでこのようなコードになっている
                write!(out, "\r\n");
            }
        }

        if let Some((r, c)) = display_cursor {
            write!(out, "{}", cursor::Goto(c as u16 + 1, r as u16 + 1));
        }

        out.flush().unwrap();
    }

    fn scroll(&mut self) {
        let (rows, _) = Self::terminal_size();
        self.row_offset = min(self.row_offset, self.cursor.row);
        if self.cursor.row + 1 >= rows {
            self.row_offset = max(self.row_offset, self.cursor.row + 1 - rows);
        }
    }

    fn cursor_up(&mut self) {
        if self.cursor.row > 0 {
            self.cursor.row -= 1;
            self.cursor.column = min(self.buffer[self.cursor.row].len(), self.cursor.column);
        }
        self.scroll();
    }

    fn cursor_dwon(&mut self) {
        if self.cursor.row + 1 < self.buffer.len() {
            self.cursor.row += 1;
            self.cursor.column = min(self.cursor.column, self.buffer[self.cursor.row].len());
        }
        self.scroll();
    }

    fn cursor_left(&mut self) {
        if self.cursor.column > 0 {
            self.cursor.column -= 1;
        }
        self.scroll();
    }

    fn cursor_right(&mut self) {
        self.cursor.column = min(self.cursor.column + 1, self.buffer[self.cursor.row].len());
        self.scroll();
    }

    fn insert(&mut self, c: char) {
        if c == '\n' {
            let rest: Vec<char> = self.buffer[self.cursor.row]
                .drain(self.cursor.column..)
                .collect();
            self.buffer.insert(self.cursor.row + 1, rest);
            self.cursor.row += 1;
            self.cursor.column = 0;
            self.scroll();
        } else if !c.is_control() {
            self.buffer[self.cursor.row].insert(self.cursor.column, c);
            self.cursor_right();
        }
    }

    fn back_space(&mut self) {
        if self.cursor == (Cursor { row: 0, column: 0 }) {
            return;
        }

        if self.cursor.column == 0 {
            let line = self.buffer.remove(self.cursor.row);
            self.cursor.row -= 1;
            self.cursor.column = self.buffer[self.cursor.row].len();
            self.buffer[self.cursor.row].extend(line.iter());
        } else {
            self.cursor_left();
            self.buffer[self.cursor.row].remove(self.cursor.column);
        }
    }

    fn delete(&mut self) {
        if self.cursor.row == self.buffer.len() - 1
            && self.cursor.column == self.buffer[self.cursor.row].len()
        {
            return;
        }

        if self.cursor.column == self.buffer[self.cursor.row].len() {

            let line = self.buffer.remove(self.cursor.row + 1);
            self.buffer[self.cursor.row].extend(line.iter());
        } else {
            self.buffer[self.cursor.row].remove(self.cursor.column);
        }
    }

    fn save(&self) {
        if let Some(path) = self.path.as_ref() {
            if let Ok(mut file) = fs::File::create(path) {
                for line in &self.buffer {
                    for &c in line {
                        write!(file, "{}", c);
                    }
                    writeln!(file);
                }
            }
        }
    }
}

fn main() {
    // clap
    let matches = App::new("testediter")
        .about("A text editer")
        .bin_name("testediter")
        .arg(Arg::with_name("file"))
        .get_matches();

    let file_path: Option<&OsStr> = matches.value_of_os("file");

    let mut state = EditerState::default();

    if let Some(file_path) = file_path {
        state.open(path::Path::new(file_path));
    }

    let stdin = stdin();
    let mut stdout = AlternateScreen::from(stdout().into_raw_mode().unwrap());

    state.draw(&mut stdout);

    for evt in stdin.events() {
        match evt.unwrap() {
            Event::Key(Key::Ctrl('c')) => {
                return;
            },
            Event::Key(Key::Ctrl('s')) => {
                state.save();
            }
            Event::Key(Key::Up) => {
                state.cursor_up();
            },
            Event::Key(Key::Down) => {
                state.cursor_dwon();
            },
            Event::Key(Key::Left) => {
                state.cursor_left();
            },
            Event::Key(Key::Right) => {
                state.cursor_right();
            },
            Event::Key(Key::Char(c)) => {
                state.insert(c);
            },
            Event::Key(Key::Backspace) => {
                state.back_space();
            },
            Event::Key(Key::Delete) => {
                state.delete();
            },
            _ => {},
        }
        state.draw(&mut stdout);
    }
}

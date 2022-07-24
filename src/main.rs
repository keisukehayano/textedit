use std::io::{stdin, stdout, Write};
use termion::event::{Event, Key};
use termion::input::TermRead;
use termion::raw::IntoRawMode;

fn main() {
    let stdin = stdin();

    let mut stdout = stdout().into_raw_mode().unwrap();

    for event in stdin.events() {
        if event.unwrap() == Event::Key(Key::Ctrl('c')) {
            return;
        }
    }
}

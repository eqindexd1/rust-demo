use std::io::{self, Write};
use std::time::Duration;

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    terminal::{self, Clear, ClearType},
};

fn main() -> io::Result<()> {
    let text = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        eprintln!("usage: rust-demo <text>");
        std::process::exit(1);
    }

    let mut out = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;

    let result = run(&mut out, &text);

    execute!(out, cursor::Show, terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    result
}

fn run(out: &mut impl Write, text: &str) -> io::Result<()> {
    let chars: Vec<char> = text.chars().collect();
    // Start fully off-screen to the right, end fully off-screen to the left.
    let mut offset: i32 = 0;

    loop {
        let (cols, rows) = terminal::size()?;
        let width = cols as i32;
        let span = width + chars.len() as i32;
        let x = width - offset; // column of the first character

        let line: String = (0..width)
            .map(|col| {
                let i = col - x;
                if i >= 0 && (i as usize) < chars.len() {
                    chars[i as usize]
                } else {
                    ' '
                }
            })
            .collect();

        queue!(out, Clear(ClearType::All), cursor::MoveTo(0, rows / 2))?;
        write!(out, "{line}")?;
        out.flush()?;

        offset = (offset + 1) % span;

        if event::poll(Duration::from_millis(60))? {
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(());
                }
            }
        }
    }
}

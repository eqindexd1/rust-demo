use std::io::{self, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal,
};

const FRAME: Duration = Duration::from_millis(30);
/// Columns scrolled per frame at 1x speed.
const BASE_SPEED: f32 = 0.5;
const SPEEDS: [f32; 7] = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0];
const DEFAULT_SPEED: usize = 2;
const TOAST_TIME: Duration = Duration::from_millis(1800);
const GLITCH_GLYPHS: &[char] = &['#', '@', '$', '%', '&', '*', '!', '?', '/', '\\', '<', '>', '=', '+'];

type Rgb = (u8, u8, u8);

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

    execute!(out, ResetColor, cursor::Show, terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    result
}

fn run(out: &mut impl Write, text: &str) -> io::Result<()> {
    let mut app = App::new(text);
    let mut next = Instant::now();

    loop {
        let (cols, rows) = terminal::size()?;
        app.step(cols, rows);
        app.draw(out, cols, rows)?;

        // Handle input until the next frame is due, so key presses don't
        // speed up the animation.
        next += FRAME;
        loop {
            let timeout = next.saturating_duration_since(Instant::now());
            if !event::poll(timeout)? {
                break;
            }
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press && app.handle_key(key) {
                    return Ok(());
                }
        }
        // Don't try to catch up after a stall (e.g. the terminal was suspended).
        next = next.max(Instant::now() - FRAME);
    }
}

// ---------------------------------------------------------------------------
// Looks: palettes, motions and effects
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Palette {
    RainbowWave,
    RainbowCycle,
    Spectrum,
    Fire,
    Ocean,
    Matrix,
    Neon,
    Sparkle,
    Candy,
    Solid(&'static str, Rgb),
}

const PALETTES: &[Palette] = &[
    Palette::RainbowWave,
    Palette::RainbowCycle,
    Palette::Spectrum,
    Palette::Fire,
    Palette::Ocean,
    Palette::Matrix,
    Palette::Neon,
    Palette::Sparkle,
    Palette::Candy,
    Palette::Solid("White", (240, 240, 240)),
    Palette::Solid("Red", (255, 70, 70)),
    Palette::Solid("Orange", (255, 150, 40)),
    Palette::Solid("Yellow", (255, 230, 60)),
    Palette::Solid("Green", (80, 230, 100)),
    Palette::Solid("Cyan", (60, 220, 240)),
    Palette::Solid("Blue", (80, 120, 255)),
    Palette::Solid("Purple", (170, 90, 255)),
    Palette::Solid("Pink", (255, 110, 200)),
];

impl Palette {
    fn name(self) -> &'static str {
        match self {
            Palette::RainbowWave => "Rainbow wave",
            Palette::RainbowCycle => "Rainbow cycle",
            Palette::Spectrum => "Spectrum",
            Palette::Fire => "Fire",
            Palette::Ocean => "Ocean",
            Palette::Matrix => "Matrix",
            Palette::Neon => "Neon pulse",
            Palette::Sparkle => "Sparkle",
            Palette::Candy => "Candy cane",
            Palette::Solid(name, _) => name,
        }
    }

    /// Color of character `i` drawn at screen column `x` on frame `t`.
    fn paint(self, i: usize, x: i32, t: u64) -> Rgb {
        let (fi, tf) = (i as f32, t as f32);
        let i = i as u64;
        match self {
            Palette::RainbowWave => hsv(tf * 4.0 + fi * 18.0, 0.85, 1.0),
            Palette::RainbowCycle => hsv(tf * 3.0, 0.85, 1.0),
            // Colors are pinned to the screen; the text slides through them.
            Palette::Spectrum => hsv(x as f32 * 5.0, 0.8, 1.0),
            Palette::Fire => {
                let n = noise(i, t / 3);
                hsv(n * 50.0, 1.0, 0.7 + 0.3 * n)
            }
            Palette::Ocean => {
                let w = (tf * 0.08 + fi * 0.4).sin() * 0.5 + 0.5;
                hsv(180.0 + w * 45.0, 0.9 - 0.4 * w, 0.6 + 0.4 * w)
            }
            Palette::Matrix => {
                if noise(i, t / 4) > 0.85 {
                    (210, 255, 210)
                } else {
                    (0, 110 + (noise(i, t / 8) * 145.0) as u8, 40)
                }
            }
            Palette::Neon => {
                let p = (tf * 0.12 + fi * 0.15).sin() * 0.5 + 0.5;
                lerp((255, 0, 200), (0, 230, 255), p)
            }
            Palette::Sparkle => {
                if noise(i, t / 2) > 0.8 {
                    hsv(noise(i + 999, t / 2) * 360.0, 0.6, 1.0)
                } else {
                    (150, 150, 175)
                }
            }
            Palette::Candy => {
                if ((i + t / 4) / 2).is_multiple_of(2) {
                    (255, 60, 120)
                } else {
                    (255, 255, 255)
                }
            }
            Palette::Solid(_, c) => c,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Motion {
    Steady,
    Wave,
    Bounce,
    Coaster,
    Glitch,
}

const MOTIONS: &[Motion] = &[Motion::Steady, Motion::Wave, Motion::Bounce, Motion::Coaster, Motion::Glitch];

impl Motion {
    fn name(self) -> &'static str {
        match self {
            Motion::Steady => "Steady",
            Motion::Wave => "Wave",
            Motion::Bounce => "Bounce",
            Motion::Coaster => "Rollercoaster",
            Motion::Glitch => "Glitch",
        }
    }

    /// Vertical offset of character `i` at screen column `x` on frame `t`.
    fn dy(self, i: usize, x: i32, t: u64, amp: f32) -> i32 {
        let (fi, tf) = (i as f32, t as f32);
        let y = match self {
            Motion::Steady => 0.0,
            Motion::Wave => (tf * 0.15 - fi * 0.45).sin() * amp,
            Motion::Bounce => amp - (tf * 0.08 - fi * 0.04).sin().abs() * 2.0 * amp,
            // A fixed track on screen that the text rides along.
            Motion::Coaster => {
                let x = x as f32;
                ((x * 0.07).sin() * 0.7 + (x * 0.019).sin() * 0.3) * amp
            }
            Motion::Glitch => match noise(i as u64, t / 2) {
                n if n > 0.965 => 1.0,
                n if n > 0.93 => -1.0,
                _ => 0.0,
            },
        };
        y.round() as i32
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Effect {
    None,
    Shadow,
    Echo,
    Reflection,
}

const EFFECTS: &[Effect] = &[Effect::None, Effect::Shadow, Effect::Echo, Effect::Reflection];

impl Effect {
    fn name(self) -> &'static str {
        match self {
            Effect::None => "None",
            Effect::Shadow => "Drop shadow",
            Effect::Echo => "Echo trail",
            Effect::Reflection => "Reflection",
        }
    }
}

// ---------------------------------------------------------------------------
// Menu
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Item {
    Color,
    Motion,
    Effect,
    Speed,
    Direction,
    Stars,
    EditText,
    Surprise,
    Help,
    Quit,
}

const MENU: &[Item] = &[
    Item::Color,
    Item::Motion,
    Item::Effect,
    Item::Speed,
    Item::Direction,
    Item::Stars,
    Item::EditText,
    Item::Surprise,
    Item::Help,
    Item::Quit,
];

impl Item {
    fn label(self) -> &'static str {
        match self {
            Item::Color => "Color",
            Item::Motion => "Motion",
            Item::Effect => "Effect",
            Item::Speed => "Speed",
            Item::Direction => "Direction",
            Item::Stars => "Starfield",
            Item::EditText => "Edit text…",
            Item::Surprise => "Surprise me!",
            Item::Help => "Help",
            Item::Quit => "Quit",
        }
    }

    /// Items with a value that ←/→ changes.
    fn adjustable(self) -> bool {
        matches!(
            self,
            Item::Color | Item::Motion | Item::Effect | Item::Speed | Item::Direction | Item::Stars
        )
    }
}

enum Overlay {
    None,
    Menu { sel: usize },
    Prompt { kind: PromptKind, buf: String },
    Help,
}

#[derive(Clone, Copy)]
enum PromptKind {
    Command,
    Text,
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

struct Star {
    x: f32,
    y: u16,
    speed: f32,
}

struct App {
    text: Vec<char>,
    offset: f32,
    tick: u64,
    paused: bool,
    palette: usize,
    motion: usize,
    effect: usize,
    speed: usize,
    reverse: bool,
    stars_on: bool,
    stars: Vec<Star>,
    star_size: (u16, u16),
    overlay: Overlay,
    toast: Option<(String, bool, Instant)>,
    rng: Rng,
}

impl App {
    fn new(text: &str) -> Self {
        App {
            text: text.chars().collect(),
            offset: 0.0,
            tick: 0,
            paused: false,
            palette: 0,
            motion: 0,
            effect: 0,
            speed: DEFAULT_SPEED,
            reverse: false,
            stars_on: false,
            stars: Vec::new(),
            star_size: (0, 0),
            overlay: Overlay::None,
            toast: Some(("Press m for the menu".into(), false, Instant::now())),
            rng: Rng::seeded(),
        }
    }

    fn step(&mut self, cols: u16, rows: u16) {
        self.tick += 1;
        if (cols, rows) != self.star_size {
            self.spawn_stars(cols, rows);
        }
        if self.paused {
            return;
        }

        let dist = BASE_SPEED * SPEEDS[self.speed];
        // Start fully off-screen on one side, end fully off-screen on the other.
        let span = (cols as usize + self.text.len()) as f32;
        self.offset = (self.offset + dist).rem_euclid(span);

        // Stars drift the same way as the text, slower ones further away.
        let dir = if self.reverse { 1.0 } else { -1.0 };
        for s in &mut self.stars {
            s.x = (s.x + dir * dist * s.speed).rem_euclid(cols.max(1) as f32);
        }
    }

    fn spawn_stars(&mut self, cols: u16, rows: u16) {
        self.star_size = (cols, rows);
        let count = cols as usize * rows as usize / 25;
        self.stars = (0..count)
            .map(|_| Star {
                x: self.rng.below(cols as u64) as f32,
                y: self.rng.below(rows as u64) as u16,
                speed: 0.1 + self.rng.unit() * 0.6,
            })
            .collect();
    }

    fn set_text(&mut self, text: &str) {
        self.text = text.chars().collect();
        self.offset = 0.0;
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), false, Instant::now()));
    }

    fn surprise(&mut self) -> String {
        self.palette = self.rng.below(PALETTES.len() as u64) as usize;
        self.motion = self.rng.below(MOTIONS.len() as u64) as usize;
        self.effect = self.rng.below(EFFECTS.len() as u64) as usize;
        self.speed = 1 + self.rng.below(4) as usize;
        self.reverse = self.rng.below(4) == 0;
        self.stars_on = self.rng.below(2) == 0;
        format!(
            "Surprise! {} + {} + {}",
            PALETTES[self.palette].name(),
            MOTIONS[self.motion].name(),
            EFFECTS[self.effect].name()
        )
    }

    fn value(&self, item: Item) -> String {
        match item {
            Item::Color => PALETTES[self.palette].name().into(),
            Item::Motion => MOTIONS[self.motion].name().into(),
            Item::Effect => EFFECTS[self.effect].name().into(),
            Item::Speed => format!("{}x", SPEEDS[self.speed]),
            Item::Direction => if self.reverse { "→ Right" } else { "← Left" }.into(),
            Item::Stars => if self.stars_on { "On" } else { "Off" }.into(),
            _ => String::new(),
        }
    }

    fn adjust(&mut self, item: Item, delta: i32) {
        match item {
            Item::Color => self.palette = cycle(self.palette, PALETTES.len(), delta),
            Item::Motion => self.motion = cycle(self.motion, MOTIONS.len(), delta),
            Item::Effect => self.effect = cycle(self.effect, EFFECTS.len(), delta),
            Item::Speed => {
                self.speed = (self.speed as i32 + delta).clamp(0, SPEEDS.len() as i32 - 1) as usize
            }
            Item::Direction => self.reverse = !self.reverse,
            Item::Stars => self.stars_on = !self.stars_on,
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Input
    // -----------------------------------------------------------------------

    /// Returns true when the app should quit.
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return true;
        }
        match std::mem::replace(&mut self.overlay, Overlay::None) {
            Overlay::None => self.hotkey(key.code),
            Overlay::Menu { sel } => self.menu_key(key.code, sel),
            Overlay::Prompt { kind, buf } => self.prompt_key(key.code, kind, buf),
            Overlay::Help => false, // any key closes help
        }
    }

    fn hotkey(&mut self, code: KeyCode) -> bool {
        let shown = match code {
            KeyCode::Char('m') | KeyCode::Tab | KeyCode::Enter => {
                self.overlay = Overlay::Menu { sel: 0 };
                return false;
            }
            KeyCode::Char('/') | KeyCode::Char(':') => {
                self.overlay = Overlay::Prompt { kind: PromptKind::Command, buf: String::new() };
                return false;
            }
            KeyCode::Char('?') | KeyCode::Char('h') => {
                self.overlay = Overlay::Help;
                return false;
            }
            KeyCode::Char('q') => return true,
            KeyCode::Char(' ') | KeyCode::Char('p') => {
                self.paused = !self.paused;
                if self.paused { "Paused" } else { "Resumed" }.to_string()
            }
            KeyCode::Char('c') | KeyCode::Right => self.adjusted(Item::Color, 1),
            KeyCode::Char('C') | KeyCode::Left => self.adjusted(Item::Color, -1),
            KeyCode::Char('v') => self.adjusted(Item::Motion, 1),
            KeyCode::Char('e') => self.adjusted(Item::Effect, 1),
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Up => self.adjusted(Item::Speed, 1),
            KeyCode::Char('-') | KeyCode::Down => self.adjusted(Item::Speed, -1),
            KeyCode::Char('r') => self.adjusted(Item::Direction, 1),
            KeyCode::Char('s') => self.adjusted(Item::Stars, 1),
            KeyCode::Char('x') => self.surprise(),
            _ => return false,
        };
        self.notify(shown);
        false
    }

    /// Adjust `item` and describe its new value.
    fn adjusted(&mut self, item: Item, delta: i32) -> String {
        self.adjust(item, delta);
        format!("{}: {}", item.label(), self.value(item))
    }

    fn menu_key(&mut self, code: KeyCode, mut sel: usize) -> bool {
        let item = MENU[sel];
        match code {
            KeyCode::Esc | KeyCode::Char('m') | KeyCode::Char('q') | KeyCode::Tab => return false,
            KeyCode::Up | KeyCode::Char('k') => sel = cycle(sel, MENU.len(), -1),
            KeyCode::Down | KeyCode::Char('j') => sel = cycle(sel, MENU.len(), 1),
            KeyCode::Left | KeyCode::Char('h') => self.adjust(item, -1),
            KeyCode::Right | KeyCode::Char('l') => self.adjust(item, 1),
            KeyCode::Enter | KeyCode::Char(' ') => match item {
                Item::EditText => {
                    let buf = self.text.iter().collect();
                    self.overlay = Overlay::Prompt { kind: PromptKind::Text, buf };
                    return false;
                }
                Item::Surprise => {
                    let msg = self.surprise();
                    self.notify(msg);
                }
                Item::Help => {
                    self.overlay = Overlay::Help;
                    return false;
                }
                Item::Quit => return true,
                _ => self.adjust(item, 1),
            },
            _ => {}
        }
        self.overlay = Overlay::Menu { sel };
        false
    }

    fn prompt_key(&mut self, code: KeyCode, kind: PromptKind, mut buf: String) -> bool {
        match code {
            KeyCode::Esc => return false,
            KeyCode::Enter => {
                return match kind {
                    PromptKind::Command => self.run_command(&buf),
                    PromptKind::Text => {
                        if !buf.trim().is_empty() {
                            self.set_text(buf.trim());
                            self.notify("Text updated");
                        }
                        false
                    }
                };
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) => buf.push(c),
            _ => {}
        }
        self.overlay = Overlay::Prompt { kind, buf };
        false
    }

    /// Run a '/' command. Returns true when the app should quit.
    fn run_command(&mut self, line: &str) -> bool {
        let line = line.trim();
        let line = line.strip_prefix('/').unwrap_or(line);
        let (name, arg) = line.split_once(' ').unwrap_or((line, ""));
        let arg = arg.trim();

        let result: Result<String, String> = match name {
            "" => return false,
            "color" | "c" => pick(PALETTES, Palette::name, arg, "color").map(|i| {
                self.palette = i;
                format!("Color: {}", PALETTES[i].name())
            }),
            "motion" | "m" => pick(MOTIONS, Motion::name, arg, "motion").map(|i| {
                self.motion = i;
                format!("Motion: {}", MOTIONS[i].name())
            }),
            "effect" | "e" => pick(EFFECTS, Effect::name, arg, "effect").map(|i| {
                self.effect = i;
                format!("Effect: {}", EFFECTS[i].name())
            }),
            "speed" | "s" => arg
                .trim_end_matches('x')
                .parse::<f32>()
                .map_err(|_| "usage: /speed <multiplier>, e.g. /speed 2".to_string())
                .map(|want| {
                    self.speed = nearest(&SPEEDS, want);
                    format!("Speed: {}x", SPEEDS[self.speed])
                }),
            "text" | "t" if arg.is_empty() => Err("usage: /text <message>".into()),
            "text" | "t" => {
                self.set_text(arg);
                Ok("Text updated".into())
            }
            "reverse" | "r" => Ok(self.adjusted(Item::Direction, 1)),
            "stars" => Ok(self.adjusted(Item::Stars, 1)),
            "pause" | "p" => {
                self.paused = !self.paused;
                Ok(if self.paused { "Paused" } else { "Resumed" }.into())
            }
            "random" | "surprise" | "x" => Ok(self.surprise()),
            "menu" => {
                self.overlay = Overlay::Menu { sel: 0 };
                return false;
            }
            "help" | "h" | "?" => {
                self.overlay = Overlay::Help;
                return false;
            }
            "quit" | "q" | "exit" => return true,
            _ => Err(format!("unknown command '/{name}' (try /help)")),
        };
        self.toast = Some(match result {
            Ok(msg) => (msg, false, Instant::now()),
            Err(e) => (format!("error: {e}"), true, Instant::now()),
        });
        false
    }

    // -----------------------------------------------------------------------
    // Drawing
    // -----------------------------------------------------------------------

    fn draw(&self, out: &mut impl Write, cols: u16, rows: u16) -> io::Result<()> {
        let mut g = Grid::new(cols as usize, rows as usize);
        if self.stars_on {
            self.draw_stars(&mut g);
        }
        self.draw_scroller(&mut g);
        self.draw_ui(&mut g);
        g.flush(out)
    }

    fn draw_stars(&self, g: &mut Grid) {
        for (k, s) in self.stars.iter().enumerate() {
            let twinkle = noise(k as u64, self.tick / 6) * 0.3 + 0.7;
            let v = ((50.0 + s.speed * 280.0) * twinkle).min(255.0) as u8;
            let ch = match s.speed {
                sp if sp < 0.3 => '.',
                sp if sp < 0.55 => '+',
                _ => '*',
            };
            g.put(s.x as i32, s.y as i32, ch, rgb((v, v, v.saturating_add(30))));
        }
    }

    fn draw_scroller(&self, g: &mut Grid) {
        let (cols, rows) = (g.cols as i32, g.rows as i32);
        let len = self.text.len() as i32;
        let t = self.tick;
        let palette = PALETTES[self.palette];
        let motion = MOTIONS[self.motion];
        let effect = EFFECTS[self.effect];
        let amp = (rows as f32 / 4.0).min(4.0);
        let base = rows / 2;
        // The side the text has already passed through.
        let behind = if self.reverse { -1 } else { 1 };

        let mut x0 = if self.reverse { self.offset as i32 - len } else { cols - self.offset as i32 };
        if motion == Motion::Glitch && noise(9999, t / 3) > 0.9 {
            x0 += if noise(9998, t / 3) > 0.5 { 2 } else { -2 };
        }

        let letters = self.text.iter().enumerate().filter(|(_, ch)| **ch != ' ');

        // Effect layer goes underneath the text.
        for (i, &ch) in letters.clone() {
            let x = x0 + i as i32;
            let color = palette.paint(i, x, t);
            match effect {
                Effect::None => {}
                Effect::Shadow => {
                    let y = base + motion.dy(i, x, t, amp);
                    g.put(x + 1, y + 1, ch, rgb(scale(color, 0.3)));
                }
                Effect::Echo => {
                    // Fainter ghosts further behind, following the path the
                    // text took a few frames ago.
                    for k in (1..=3u32).rev() {
                        let xk = x + behind * 3 * k as i32;
                        let y = base + motion.dy(i, xk, t.saturating_sub(5 * k as u64), amp);
                        g.put(xk, y, ch, rgb(scale(color, 0.6f32.powi(k as i32))));
                    }
                }
                Effect::Reflection => {
                    let mirror = base + amp.ceil() as i32 + 1;
                    let y = 2 * mirror - (base + motion.dy(i, x, t, amp));
                    let ripple = ((t as f32 * 0.25 + i as f32 * 0.7).sin() * 0.9).round() as i32;
                    let water = lerp(scale(color, 0.35), (10, 40, 90), 0.3);
                    g.put(x + ripple, y, ch, rgb(water));
                }
            }
        }

        for (i, &ch) in letters {
            let x = x0 + i as i32;
            let y = base + motion.dy(i, x, t, amp);
            let mut color = palette.paint(i, x, t);
            let mut ch = ch;
            if motion == Motion::Glitch && noise(i as u64 + 3, t / 2) > 0.95 {
                ch = GLITCH_GLYPHS[(noise(i as u64 + 5, t) * GLITCH_GLYPHS.len() as f32) as usize];
                color = (255 - color.0, 255 - color.1, 255 - color.2);
            }
            g.put(x, y, ch, rgb(color));
        }
    }

    fn draw_ui(&self, g: &mut Grid) {
        let (cols, rows) = (g.cols as i32, g.rows as i32);
        let bar_bg = Color::Rgb { r: 30, g: 30, b: 48 };

        if let Some((msg, is_err, at)) = &self.toast
            && at.elapsed() < TOAST_TIME {
                let s = format!(" {msg} ");
                let x = (cols - s.chars().count() as i32) / 2;
                let fg = if *is_err { rgb((255, 110, 110)) } else { Color::White };
                g.text(x.max(0), 0, &s, fg, bar_bg);
            }
        if self.paused {
            g.text(cols - 8, 0, " PAUSED ", Color::Black, rgb((255, 200, 60)));
        }

        match &self.overlay {
            Overlay::None => {
                let hint = " m menu  / command  ? help  space pause  q quit ";
                let x = (cols - hint.chars().count() as i32) / 2;
                g.text(x.max(0), rows - 1, hint, rgb((90, 90, 110)), Color::Reset);
            }
            Overlay::Menu { sel } => self.draw_menu(g, *sel),
            Overlay::Prompt { kind, buf } => {
                let label = match kind {
                    PromptKind::Command => "/",
                    PromptKind::Text => "text: ",
                };
                g.fill(0, rows - 1, cols, 1, bar_bg);
                g.text(0, rows - 1, label, rgb((255, 200, 60)), bar_bg);
                // Show the tail of the buffer if it doesn't fit.
                let room = (cols - label.len() as i32 - 1).max(0) as usize;
                let skip = buf.chars().count().saturating_sub(room);
                let shown: String = buf.chars().skip(skip).collect();
                let x = label.len() as i32;
                g.text(x, rows - 1, &shown, Color::White, bar_bg);
                if (self.tick / 10).is_multiple_of(2) {
                    g.text(x + shown.chars().count() as i32, rows - 1, "█", Color::White, bar_bg);
                }
            }
            Overlay::Help => self.draw_help(g),
        }
    }

    fn draw_menu(&self, g: &mut Grid, sel: usize) {
        let (w, h) = (44, MENU.len() as i32 + 4);
        let (x, y) = ((g.cols as i32 - w) / 2, (g.rows as i32 - h) / 2);
        let bg = rgb((18, 18, 30));
        let border = rgb(hsv(self.tick as f32 * 3.0, 0.7, 1.0));
        g.frame(x, y, w, h, border, bg, " ✦ SCROLLER ✦ ");

        for (row, &item) in MENU.iter().enumerate() {
            let yy = y + 1 + row as i32;
            let selected = row == sel;
            let row_bg = if selected { rgb((60, 50, 115)) } else { bg };
            g.fill(x + 1, yy, w - 2, 1, row_bg);

            let marker = if selected { "▶ " } else { "  " };
            let fg = if selected { Color::White } else { rgb((180, 180, 200)) };
            g.text(x + 2, yy, &format!("{marker}{}", item.label()), fg, row_bg);

            if item.adjustable() {
                let value = self.value(item);
                let value = if selected { format!("‹ {value} ›") } else { value };
                let color = match item {
                    // Preview the palette on its own name.
                    Item::Color => rgb(PALETTES[self.palette].paint(0, x, self.tick)),
                    _ => rgb((255, 200, 60)),
                };
                let vx = x + w - 3 - value.chars().count() as i32;
                g.text(vx, yy, &value, color, row_bg);
            }
        }

        let footer = "↑↓ select  ←→ change  ⏎ ok  esc close";
        let fx = x + (w - footer.chars().count() as i32) / 2;
        g.text(fx, y + h - 2, footer, rgb((110, 110, 140)), bg);
    }

    fn draw_help(&self, g: &mut Grid) {
        let names = |list: Vec<&str>| list.join(", ");
        let lines = [
            "Keys".to_string(),
            "  m / Tab      open the menu".into(),
            "  /            type a command".into(),
            "  c / C  ← →   next / previous color".into(),
            "  v  e         next motion / effect".into(),
            "  + -  ↑ ↓     faster / slower".into(),
            "  r  s         reverse / toggle stars".into(),
            "  x            surprise me".into(),
            "  space        pause".into(),
            "  q  Ctrl-C    quit".into(),
            String::new(),
            "Commands".into(),
            "  /color <name>    /motion <name>    /effect <name>".into(),
            "  /speed <mult>    /text <message>   /reverse".into(),
            "  /stars  /pause  /random  /menu  /help  /quit".into(),
            String::new(),
            format!("Colors:  {}", names(PALETTES[..9].iter().map(|p| p.name()).collect())),
            format!("         {}", names(PALETTES[9..].iter().map(|p| p.name()).collect())),
            format!("Motions: {}", names(MOTIONS.iter().map(|m| m.name()).collect())),
            format!("Effects: {}", names(EFFECTS.iter().map(|e| e.name()).collect())),
        ];
        let inner = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as i32;
        let (w, h) = (inner + 4, lines.len() as i32 + 4);
        let (x, y) = ((g.cols as i32 - w) / 2, (g.rows as i32 - h) / 2);
        let bg = rgb((18, 18, 30));
        g.frame(x, y, w, h, rgb((255, 200, 60)), bg, " HELP ");
        for (k, line) in lines.iter().enumerate() {
            let heading = !line.starts_with(' ') && !line.contains(':');
            let fg = if heading { rgb((255, 200, 60)) } else { rgb((200, 200, 220)) };
            g.text(x + 2, y + 1 + k as i32, line, fg, bg);
        }
        g.text(x + 2, y + h - 2, "press any key", rgb((110, 110, 140)), bg);
    }
}

// ---------------------------------------------------------------------------
// Frame buffer
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    fg: Color,
    bg: Color,
}

/// One frame, built in memory and written in a single pass to avoid flicker.
struct Grid {
    cols: usize,
    rows: usize,
    cells: Vec<Cell>,
}

impl Grid {
    fn new(cols: usize, rows: usize) -> Self {
        let blank = Cell { ch: ' ', fg: Color::Reset, bg: Color::Reset };
        Grid { cols, rows, cells: vec![blank; cols * rows] }
    }

    fn cell(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        if x < 0 || y < 0 || x as usize >= self.cols || y as usize >= self.rows {
            return None;
        }
        self.cells.get_mut(y as usize * self.cols + x as usize)
    }

    /// Draw a character, keeping whatever background is there.
    fn put(&mut self, x: i32, y: i32, ch: char, fg: Color) {
        if let Some(c) = self.cell(x, y) {
            c.ch = ch;
            c.fg = fg;
        }
    }

    fn text(&mut self, x: i32, y: i32, s: &str, fg: Color, bg: Color) {
        for (k, ch) in s.chars().enumerate() {
            if let Some(c) = self.cell(x + k as i32, y) {
                *c = Cell { ch, fg, bg };
            }
        }
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, bg: Color) {
        for yy in y..y + h {
            for xx in x..x + w {
                if let Some(c) = self.cell(xx, yy) {
                    *c = Cell { ch: ' ', fg: Color::Reset, bg };
                }
            }
        }
    }

    fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, border: Color, bg: Color, title: &str) {
        self.fill(x, y, w, h, bg);
        let line = "─".repeat((w - 2).max(0) as usize);
        self.text(x, y, &format!("╭{line}╮"), border, bg);
        self.text(x, y + h - 1, &format!("╰{line}╯"), border, bg);
        for yy in y + 1..y + h - 1 {
            self.text(x, yy, "│", border, bg);
            self.text(x + w - 1, yy, "│", border, bg);
        }
        let tx = x + (w - title.chars().count() as i32) / 2;
        self.text(tx, y, title, border, bg);
    }

    fn flush(&self, out: &mut impl Write) -> io::Result<()> {
        let (mut fg, mut bg) = (None, None);
        for (y, row) in self.cells.chunks(self.cols.max(1)).enumerate() {
            queue!(out, cursor::MoveTo(0, y as u16))?;
            for c in row {
                if fg != Some(c.fg) {
                    queue!(out, SetForegroundColor(c.fg))?;
                    fg = Some(c.fg);
                }
                if bg != Some(c.bg) {
                    queue!(out, SetBackgroundColor(c.bg))?;
                    bg = Some(c.bg);
                }
                queue!(out, Print(c.ch))?;
            }
        }
        queue!(out, ResetColor)?;
        out.flush()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn rgb((r, g, b): Rgb) -> Color {
    Color::Rgb { r, g, b }
}

/// Hue in degrees, saturation and value in 0..=1.
fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |n: f32| ((n + m) * 255.0) as u8;
    (to(r), to(g), to(b))
}

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

fn scale((r, g, b): Rgb, f: f32) -> Rgb {
    let s = |n: u8| (n as f32 * f) as u8;
    (s(r), s(g), s(b))
}

/// Deterministic pseudo-random value in 0..1 for a pair of inputs, so a
/// character flickers consistently within a frame without keeping state.
fn noise(a: u64, b: u64) -> f32 {
    let mut z = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

fn cycle(i: usize, len: usize, delta: i32) -> usize {
    (i as i32 + delta).rem_euclid(len as i32) as usize
}

fn nearest(values: &[f32], want: f32) -> usize {
    (0..values.len())
        .min_by(|&a, &b| (values[a] - want).abs().total_cmp(&(values[b] - want).abs()))
        .unwrap_or(0)
}

/// Find an item by name, ignoring case and spaces; exact match first, then prefix.
fn pick<T: Copy>(items: &[T], name: fn(T) -> &'static str, query: &str, what: &str) -> Result<usize, String> {
    let norm = |s: &str| s.to_lowercase().replace(' ', "");
    let q = norm(query);
    if q.is_empty() {
        return Err(format!("usage: /{what} <name> (see /help)"));
    }
    items
        .iter()
        .position(|&t| norm(name(t)) == q)
        .or_else(|| items.iter().position(|&t| norm(name(t)).starts_with(&q)))
        .ok_or_else(|| format!("no {what} named '{query}'"))
}

/// Small xorshift generator; plenty for star placement and surprises.
struct Rng(u64);

impl Rng {
    fn seeded() -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        Rng(nanos | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

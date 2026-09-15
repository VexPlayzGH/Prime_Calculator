use std::{io, sync::mpsc};

use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{DefaultTerminal, prelude::*, symbols::border, widgets::{Block, Gauge, Paragraph, Wrap}};

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();

    let mut app = App {
        exit: false,
        progress_bar_color: Color::Green,
        max_number: 0_u128,
        background_progress: 0_f64,
        prime_vector: Vec::new(),
        results_scroll: 0,
        input: String::new(),
        input_error: None,
        running: false,
    };

    let (event_tx, event_rx) = mpsc::channel::<Event>();

    let tx_to_input_events = event_tx.clone();
    std::thread::spawn(move || handle_input_events(tx_to_input_events));

    let app_result = app.run(&mut terminal, event_rx, event_tx);

    ratatui::restore();
    app_result
}

enum Event {
    Input(crossterm::event::KeyEvent),
    Progress(u128, f64),
    Complete(u128, Vec<u128>),
}

fn handle_input_events(tx: mpsc::Sender<Event>) {
    loop {
        match crossterm::event::read().unwrap() {
            crossterm::event::Event::Key(key_event) => tx.send(Event::Input(key_event)).unwrap(),
            _ => {},
        }
    }
}

fn run_background_thread(tx: mpsc::Sender<Event>, max_n: u128) {
    let mut progress = 0_f64;
    let increment = 1_f64 / max_n as f64;
    let mut comp_primes: Vec<u128> = Vec::new();
    match max_n {
        0 | 1 => {
            tx.send(Event::Complete(max_n, comp_primes)).unwrap();
        },
        2 => {
            comp_primes.push(2);
            tx.send(Event::Complete(max_n, comp_primes)).unwrap();
        },
        _ => {
            for n in 2..=max_n {
                let mut is_prime = true;
                for p in 2..=n.isqrt() {
                    if n % p == 0 {
                        is_prime = false;
                        break;
                    }
                }
                if is_prime {
                    comp_primes.push(n);
                }
                progress += increment;
                progress = progress.min(1.0);
                if n == max_n || n % (max_n / 10_000).max(1) == 0 {
                    tx.send(Event::Progress(max_n, progress)).unwrap();
                }
            }
            tx.send(Event::Complete(max_n, comp_primes)).unwrap();
        },
    }
}

pub struct App {
    exit: bool,
    progress_bar_color: Color,
    max_number: u128,
    background_progress: f64,
    prime_vector: Vec<u128>,
    results_scroll: u16,
    input: String,
    input_error: Option<String>,
    running: bool,
}

impl App {
    fn run(
        &mut self,
        terminal: &mut DefaultTerminal,
        rx: mpsc::Receiver<Event>,
        tx: mpsc::Sender<Event>,
    ) -> io::Result<()> {
        terminal.draw(|frame| self.draw(frame))?;
        while !self.exit {
            match rx.recv().unwrap() {
                Event::Input(key_event) => self.handle_key_event(key_event, &tx)?,
                Event::Progress(max_n, progress) => {
                    self.max_number = max_n;
                    self.background_progress = progress;
                },
                Event::Complete(max_n, comp_primes) => {
                    self.max_number = max_n;
                    self.background_progress = 1.0;
                    self.prime_vector = comp_primes;
                    self.running = false;
                },
            }
            terminal.draw(|frame| self.draw(frame))?;
        }

        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn handle_key_event(
        &mut self,
        key_event: crossterm::event::KeyEvent,
        tx: &mpsc::Sender<Event>,
    ) -> io::Result<()> {
        if key_event.kind == KeyEventKind::Press {
            match key_event.code {
                KeyCode::Esc => self.exit = true,
                KeyCode::Char('c') => {
                    self.progress_bar_color = match self.progress_bar_color {
                        Color::Green => Color::Yellow,
                        Color::Yellow => Color::Red,
                        Color::Red => Color::Blue,
                        Color::Blue => Color::Green,
                        _ => Color::Green,
                    };
                },
                KeyCode::Char(character) if character.is_ascii_digit() && !self.running => {
                    self.input.push(character);
                    self.input_error = None;
                },
                KeyCode::Backspace if !self.running => {
                    self.input.pop();
                },
                KeyCode::Enter if !self.running => {
                    match self.input.parse::<u128>() {
                        Ok(max_n) => {
                            self.max_number = max_n;
                            self.background_progress = 0.0;
                            self.prime_vector.clear();
                            self.results_scroll = 0;
                            self.input_error = None;
                            self.running = true;
                            let worker_tx = tx.clone();
                            std::thread::spawn(move || run_background_thread(worker_tx, max_n));
                        },
                        Err(_) => self.input_error = Some("Enter a valid number".into()),
                    }
                },
                KeyCode::Up => self.results_scroll = self.results_scroll.saturating_sub(1),
                KeyCode::Down => self.results_scroll = self.results_scroll.saturating_add(1),
                KeyCode::PageUp => self.results_scroll = self.results_scroll.saturating_sub(10),
                KeyCode::PageDown => self.results_scroll = self.results_scroll.saturating_add(10),
                KeyCode::Home => self.results_scroll = 0,
                _ => {},
            }
        }
        Ok(())
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where Self: Sized {
        let vertical_layout = Layout::vertical([Constraint::Percentage(20), Constraint::Percentage(80)]);
        let [title_area, gauge_area] = vertical_layout.areas(area);
        let [input_area, progress_area, results_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ]).areas(gauge_area);

        Line::from("PRIME CALCULATOR").bold().centered().render(title_area, buf);

        let instructions = Line::from(vec![
            " Change Color ".into(),
            "<C>".blue().bold(),
            " Quit ".into(),
            "<Esc>".red().bold(),
        ]).centered();

        let input_title = if self.running {
            "Maximum number (running)"
        } else {
            "Maximum number (Enter to start)"
        };
        let input_text = if let Some(error) = &self.input_error {
            format!("{}  {}", self.input, error)
        } else {
            self.input.clone()
        };
        Paragraph::new(input_text)
            .block(Block::bordered().title(input_title.bold()))
            .render(input_area, buf);
        
        if self.running {
            let block = Block::bordered()
                .title(Line::from("Progress").bold())
                .title_bottom(instructions)
                .border_set(border::THICK);

            let progress_bar = Gauge::default()
                .gauge_style(Style::default()
                .fg(self.progress_bar_color))
                .block(block)
                .label(format!("Computing primes up to {}: {:.2}%", self.max_number, self.background_progress * 100_f64))
                .ratio(self.background_progress);

             progress_bar.render( Rect {
                x: progress_area.left(),
                y: progress_area.top(),
                width: progress_area.width,
                height: 3,
            }, buf)
        }        
        
        if self.background_progress == 1.0 {

            let res_instructions = Line::from(vec![
                " Scroll one line ".into(),
                "<Up, Down>".blue().bold(),
                " Scroll ten lines ".into(),
                "<PageUp, PageDown>".yellow().bold(),
                " Return to the top ".into(),
                "<Home>".green().bold(),
                " Quit ".into(),
                "<Esc>".red().bold(),
            ]).centered();


            Paragraph::new(format!("Primes found: {:?}", self.prime_vector))
                .block(Block::bordered().title("Results".bold()).title_bottom(res_instructions))
                .wrap(Wrap { trim: true })
                .scroll((self.results_scroll, 0))
                    .render(results_area, buf);
        }    

       
    }
}

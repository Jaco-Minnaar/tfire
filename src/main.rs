use lazy_static::lazy_static;
use rand::Rng;
use std::{
    io::{stdin, stdout, Read, Stdout, StdoutLock, Write},
    sync::mpsc,
    thread,
    time::Duration,
};
use termion::{
    color::{self, Rgb},
    cursor::{self, HideCursor},
    event::Key,
    input::{Keys, TermRead},
    raw::{IntoRawMode, RawTerminal},
};

lazy_static! {
    static ref COLORS: [Rgb; 36] = [
        Rgb(7, 7, 7),
        Rgb(31, 7, 7),
        Rgb(47, 15, 7),
        Rgb(71, 15, 7),
        Rgb(81, 23, 7),
        Rgb(103, 31, 7),
        Rgb(119, 31, 7),
        Rgb(143, 39, 7),
        Rgb(159, 47, 7),
        Rgb(175, 63, 7),
        Rgb(191, 71, 7),
        Rgb(199, 71, 7),
        Rgb(223, 79, 7),
        Rgb(223, 87, 7),
        Rgb(223, 87, 7),
        Rgb(215, 95, 7),
        Rgb(215, 103, 15),
        Rgb(207, 111, 15),
        Rgb(207, 119, 15),
        Rgb(207, 127, 15),
        Rgb(207, 135, 23),
        Rgb(199, 135, 23),
        Rgb(199, 143, 23),
        Rgb(199, 151, 31),
        Rgb(191, 159, 31),
        Rgb(191, 159, 31),
        Rgb(191, 167, 39),
        Rgb(191, 167, 39),
        Rgb(191, 175, 47),
        Rgb(183, 175, 47),
        Rgb(183, 183, 47),
        Rgb(183, 183, 55),
        Rgb(207, 207, 111),
        Rgb(223, 223, 159),
        Rgb(239, 239, 199),
        Rgb(255, 255, 255),
    ];
}

fn main() {
    let (stop_tx, stop_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let mut terminal = Terminal::default();
        loop {
            terminal.next_frame();

            match stop_rx.try_recv() {
                Ok(false) => terminal.stop_flames(),
                Ok(true) => break,
                Err(_) => (),
            }

            if terminal.done {
                break;
            }
        }
    });

    let key = stdin().lock().keys().next().unwrap().unwrap();

    let val = match key {
        Key::Ctrl('c') | Key::Ctrl('d') => true,
        _ => false,
    };
    stop_tx.send(val).unwrap();

    handle.join().unwrap();
}

struct Terminal<W: Write> {
    width: usize,
    height: usize,
    read_frame_buffer: Vec<usize>,
    write_frame_buffer: Vec<usize>,
    out: W,
    stop: bool,
    pub done: bool,
}

impl<W: Write> Terminal<W> {
    fn draw(&mut self) {
        let mut has_color = false;
        for y in 0..self.height {
            for x in 0..self.width {
                let new_index = self.write_frame_buffer[y * self.width + x];
                let index = self.read_frame_buffer[y * self.width + x];

                let color = COLORS[index];
                let new_color = COLORS[new_index];

                if !has_color && index != 0 {
                    has_color = true;
                }

                if color == new_color {
                    continue;
                }

                write!(
                    self.out,
                    "{}{}█",
                    cursor::Goto((x + 1) as u16, (y + 1) as u16),
                    color::Fg(new_color)
                )
                .unwrap();
            }
        }

        self.out.flush().unwrap();

        self.read_frame_buffer = self.write_frame_buffer.clone();

        if !has_color {
            self.done = true;
        }
    }

    fn next_frame(&mut self) {
        let mut rng = rand::thread_rng();
        for x in 0..self.width {
            for y in 1..self.height {
                self.spread_fire(y * self.width + x, rng.gen());
            }
        }

        self.draw();

        if self.stop {
            self.cool_flames();
        }
    }

    fn spread_fire(&mut self, from: usize, coef: f32) {
        if self.write_frame_buffer[from] == 0 {
            self.write_frame_buffer[from - self.width] = 0;
        } else {
            let rand_val = (coef * 3.0).round() as usize;
            let to = (from - self.width).saturating_sub(rand_val);
            self.write_frame_buffer[to] =
                self.write_frame_buffer[from].saturating_sub(rand_val & 1);
        }
    }

    fn stop_flames(&mut self) {
        self.stop = true;
    }

    fn cool_flames(&mut self) {
        for y in (self.height - 5..self.height).rev() {
            for x in 0..self.width {
                let pos = y * self.width + x;
                if self.write_frame_buffer[pos] > 0 {
                    self.write_frame_buffer[pos] -= (rand::random::<f32>()).round() as usize & 3;
                }
            }
        }
    }
}

impl Default for Terminal<HideCursor<RawTerminal<StdoutLock<'_>>>> {
    fn default() -> Self {
        let (width, height) = termion::terminal_size().unwrap();

        let width = width as usize;
        let height = height as usize;
        let pixels = width * height;
        let stdout = stdout().lock().into_raw_mode().unwrap();
        let writer = HideCursor::from(stdout);

        let mut frame_buffer = vec![0; pixels.into()];
        let bottom_row = (width * (height - 1)) as usize;
        for i in bottom_row..frame_buffer.len() {
            frame_buffer[i] = 35;
        }

        let mut terminal = Self {
            width,
            height,
            read_frame_buffer: frame_buffer.clone(),
            write_frame_buffer: frame_buffer,
            out: writer,
            stop: false,
            done: false,
        };

        terminal.draw();

        terminal
    }
}

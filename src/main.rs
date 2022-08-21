use lazy_static::lazy_static;
use rand::Rng;
use std::{
    io::{stdin, stdout, Read, Stdout, StdoutLock, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
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
        let (signal_tx, signal_rx) = mpsc::channel();
        let (buffer_tx, buffer_rx) = mpsc::channel();
        let (stop_tx_2, stop_rx_2) = mpsc::channel();
        let is_done = Arc::new(AtomicBool::new(false));
        let is_done_clone = Arc::clone(&is_done);

        let mut terminal = Terminal::new(buffer_rx, signal_tx);

        let width = terminal.width;
        let height = terminal.height;

        let draw_handle = thread::spawn(move || {
            let mut buffer_container = BufferContainer::new(width, height, buffer_tx, signal_rx);
            loop {
                buffer_container.next_frame();

                match stop_rx_2.try_recv() {
                    Ok(true) => break,
                    Ok(false) => buffer_container.stop_flames(),
                    Err(_) => (),
                }

                if buffer_container.done {
                    is_done.store(true, Ordering::Relaxed);
                    break;
                }

                thread::sleep(Duration::from_millis(20));
            }
        });

        loop {
            terminal.draw();

            if let Ok(val) = stop_rx.try_recv() {
                stop_tx_2.send(val).unwrap();
                if val {
                    break;
                }
            }

            if is_done_clone.load(Ordering::Relaxed) {
                break;
            }
        }

        draw_handle.join().unwrap();
    });

    let key = stdin().lock().keys().next().unwrap().unwrap();

    let val = match key {
        Key::Ctrl('c') | Key::Ctrl('d') => true,
        _ => false,
    };
    stop_tx.send(val).unwrap();

    handle.join().unwrap();
}

struct BufferContainer {
    width: usize,
    height: usize,
    write_frame_buffer: Vec<usize>,
    stop: bool,
    sender: Sender<Vec<usize>>,
    signal: Receiver<()>,
    pub done: bool,
}

impl BufferContainer {
    pub fn new(
        width: usize,
        height: usize,
        sender: Sender<Vec<usize>>,
        signal: Receiver<()>,
    ) -> Self {
        let mut frame_buffer = vec![0; width * height];
        let bottom_row = (width * (height - 1)) as usize;
        for i in bottom_row..frame_buffer.len() {
            frame_buffer[i] = 35;
        }
        Self {
            width,
            height,
            sender,
            signal,
            write_frame_buffer: frame_buffer,
            stop: false,
            done: false,
        }
    }

    fn next_frame(&mut self) {
        let mut rng = rand::thread_rng();

        if self.stop {
            self.cool_flames();
        }

        for x in 0..self.width {
            for y in 1..self.height {
                self.spread_fire(y * self.width + x, rng.gen());
            }
        }

        if let Ok(_) = self.signal.try_recv() {
            self.sender.send(self.write_frame_buffer.clone()).unwrap();
        }

        let mut all_black = true;
        for index in &self.write_frame_buffer {
            if index != &0 {
                all_black = false;
                break;
            }
        }

        self.done = all_black;
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

struct Terminal<W: Write> {
    pub width: usize,
    pub height: usize,
    pub frame_buffer: Vec<usize>,
    out: W,
    signal_tx: Sender<()>,
    buffer_rx: Receiver<Vec<usize>>,
}

impl Terminal<HideCursor<RawTerminal<StdoutLock<'static>>>> {
    pub fn new(buffer_rx: Receiver<Vec<usize>>, signal_tx: Sender<()>) -> Self {
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
            buffer_rx,
            signal_tx,
            frame_buffer: frame_buffer.clone(),
            out: writer,
        };

        terminal
    }
}

impl<W: Write> Terminal<W> {
    fn draw(&mut self) {
        if let Err(_) = self.signal_tx.send(()) {
            return;
        }

        let write_frame_buffer = self.buffer_rx.recv().unwrap();

        for y in 0..self.height {
            for x in 0..self.width {
                let new_index = write_frame_buffer[y * self.width + x];
                let index = self.frame_buffer[y * self.width + x];

                let color = COLORS[index];
                let new_color = COLORS[new_index];

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

        self.frame_buffer = write_frame_buffer.clone();
    }
}

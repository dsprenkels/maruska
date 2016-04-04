use std::char;
use std::thread;
use std::cmp::max;

use chan;
use termbox::*;
use time::{Duration, get_time};

use libclient::Client;

pub enum Error {
    Custom(String),
    Quit,
}

pub struct TUI {
    results_offset: usize,
    results_invalidated: bool,
    query_invalidated: bool,
    query: String,
}

impl TUI {
    pub fn new() -> TUI {
        unsafe { tb_init(); }
        TUI {
            results_offset: 0,
            results_invalidated: true,
            query_invalidated: true,
            query: String::new(),
        }
    }

    pub fn run() -> chan::Receiver<RawEvent> {
        let (s, r) = chan::async();
        thread::spawn(move || TUI::mainloop(s));
        r
    }

    fn mainloop(events_s: chan::Sender<RawEvent>) {
        loop {
            unsafe {
                let mut event = RawEvent {
                    etype: 0,
                    emod: 0,
                    key: 0,
                    ch: 0,
                    w: 0,
                    h: 0,
                    x: 0,
                    y: 0,
                };
                tb_poll_event(&mut event);
                events_s.send(event);
            }
        }
    }

    pub fn handle_event(&mut self, event: RawEvent) -> Result<(), Error> {
        match event.etype {
            TB_EVENT_KEY => if event.ch == 0 {
                self.handle_input_key(event.key)
            } else {
                self.handle_input_ch(event.ch)
            },
            TB_EVENT_RESIZE => unimplemented!(),
            TB_EVENT_MOUSE => unimplemented!(),
            _ => {
                error!("unknown etype {}", event.etype);
                Ok(())
            },
        }
    }

    fn handle_input_ch(&mut self, ch: u32) -> Result<(), Error> {
        match ch {
            47 | 58 => self.handle_input_cmdtypechar(ch),
            33 ... 126 => self.handle_input_alphanum(ch),
            ch => Err(Error::Custom(format!("handling of ch {} is not implemented", ch)))
        }
    }

    fn handle_input_key(&mut self, key: u16) -> Result<(), Error> {
        match key {
            TB_KEY_ENTER => self.handle_input_linefeed(key),
            TB_KEY_SPACE => self.handle_input_alphanum(' ' as u32),
            TB_KEY_BACKSPACE | TB_KEY_BACKSPACE2 => self.handle_input_backspace(key),
            TB_KEY_CTRL_C => Err(Error::Quit),
            TB_KEY_CTRL_U => self.handle_input_nak(key),
            key => Err(Error::Custom(format!("handling of keycode {} is not implemented", key))),
        }
    }

    fn handle_input_backspace(&mut self, _: u16) -> Result<(), Error> {
        self.query.pop();
        self.invalidate_query();
        Ok(())
    }

    fn handle_input_linefeed(&mut self, _: u16) -> Result<(), Error> {
        Err(Error::Custom(format!("handle_input_linefeed is not implemented")))
    }

    fn handle_input_nak(&mut self, _: u16) -> Result<(), Error> {
        if self.query.len() > 1 {
            self.query.truncate(1);
        } else {
            self.query.clear();
        }
        Ok(self.invalidate_query())
    }

    fn handle_input_cmdtypechar(&mut self, ch: u32) -> Result<(), Error> {
        if !self.query.is_empty() { return Ok(()); }

        if self.query.len() == 0 {
            match ch {
                47 => self.query.push('/'),
                58 => self.query.push(':'),
                _ => error!("unreachable"),
            }
        }
        Ok((self.invalidate_query()))
    }

    fn handle_input_alphanum(&mut self, input_ch: u32) -> Result<(), Error> {
        let ch_option = char::from_u32(input_ch as u32);
        match ch_option {
            Some(ch) => {
                if self.query.is_empty() { self.query.push('/') };
                self.query.push(ch);
            },
            None => error!("unreachable")
        }
        Ok((self.invalidate_query()))
    }

    unsafe fn print(&self, x: i32, y: i32, fg: u16, bg: u16, s: &str) {
        for (i, ch) in s.chars().enumerate() {
            tb_change_cell(x+i as i32, y, ch as u32, fg, bg);
        }
    }

    unsafe fn print_truncate(&self, x: i32, y: i32, fg: u16, bg: u16, s: &str, maxlen: usize,
                             trunc_fg: u16, trunc_bg: u16, trunc_s: &str) {
        if s.len() < maxlen || s.is_empty() {
            self.print(x, y, fg, bg, s);
        } else {
            let print_len = max(maxlen - trunc_s.len(), 0);
            for (i, ch) in s.chars().take(print_len).enumerate() {
                tb_change_cell(x+i as i32, y, ch as u32, fg, bg);
            }
            for (i, ch) in trunc_s.chars().take(maxlen).enumerate() {
                tb_change_cell(x+(print_len as i32)+i as i32, y, ch as u32, trunc_fg, trunc_bg);
            }
        }
    }

    pub fn draw(&mut self, client: &Client) {
        unsafe { tb_clear(); }
        if self.results_invalidated {
            self.draw_results(client)
        }
        if self.query_invalidated {
            self.draw_query(client)
        }
        unsafe { tb_present(); }
    }

    fn draw_results(&mut self, client: &Client) {
        let (w, h) = unsafe { (tb_width() as usize, tb_height() as usize) };
        let mut str_table: Vec<Vec<String>> = Vec::new();

        // first line shows currently playing song
        let mut queue_length = Duration::zero();
        str_table.push(if let &Some(ref playing) = client.get_playing() {
            let requested_by = String::from(unwrap_requested_by(&playing.requested_by));
            queue_length = queue_length + (playing.end_time - get_time());
            vec!(requested_by, playing.media.artist.clone(), playing.media.title.clone(),
                 format_duration(queue_length))
        } else {
            vec!(String::new(), String::new(), String::new(), String::new())
        });

        // rest shows the current request queue
        if let &Some(ref requests) = client.get_requests() {
            for request in requests.iter().skip(self.results_offset).take(h - 2) {
                let requested_by = String::from(unwrap_requested_by(&request.by));
                let media = &request.media;
                queue_length = queue_length + media.length;;
                str_table.push(vec!(requested_by, media.artist.clone(), media.title.clone(),
                                    format_duration(queue_length)))
            }
        }

        // get optimal column widths
        let col_widths = fit_columns(&str_table, &[1f32, 4f32, 4f32, 1f32], w as usize);

        if col_widths.len() != 4 {
            unsafe { tb_shutdown() };
            panic!("col_widths.len() != 4 (is {})", col_widths.len());
        }

        // do the actual drawing
        for (y, row) in str_table.iter().enumerate() {
            unsafe {
                for (j, cell) in row.iter().enumerate() {
                    let x = col_widths.iter().take(j).fold(0, |a, b| a + b);
                    let maxlen = col_widths[j];
                    self.print_truncate(x as i32, y as i32, 0, 0, cell, maxlen, 0, 0, "$");
                }
            }
        }

        self.results_invalidated = false;
    }

    fn draw_query(&mut self, _: &Client) {
        // draw query field
        unsafe {
            let (w, h) = (tb_width(), tb_height());
            self.print_truncate(0, h-1, 0, 0, &self.query, w as usize, 0, 0, "...");
        }
        // for i in 0..ncurses::COLS {
            // ncurses::mvwaddch(self.querywindow, 0, i, ' ' as ncurses::chtype);
        // }
        // ncurses::mvwaddnstr(self.querywindow, 0, 0, &self.query, self.query.len() as i32);
        // ncurses::wrefresh(self.querywindow);
        self.query_invalidated = false;
    }

    pub fn invalidate_resultswindow(&mut self) {
        self.results_invalidated = true;
    }

    pub fn invalidate_query(&mut self) {
        self.query_invalidated = true;
    }
}

impl Drop for TUI {
    fn drop(&mut self) {
        unsafe { tb_shutdown() };
    }
}

fn unwrap_requested_by<'a>(requested_by: &'a Option<String>) -> &'a str {
    match requested_by {
        &Some(ref by) => &by,
        &None => "marietje"
    }
}

fn format_duration(d: Duration) -> String {
    match () {
        _ if d.num_days() != 0 => format!("{}d{}:{:02}:{:02}",
            d.num_days(), d.num_hours() % 24, d.num_minutes() % 60, d.num_seconds() % 60),
        _ if d.num_hours() != 0 => format!("{}:{}:{:02}",
            d.num_hours(), d.num_minutes() % 60, d.num_seconds() % 60),
        _ =>  format!("{}:{:02}", d.num_minutes(), d.num_seconds() % 60)
    }
}

fn fit_columns(rows: &Vec<Vec<String>>, expand_factors: &[f32], fit_width: usize) -> Vec<usize> {
    assert!(!rows.is_empty());

    let mut cols = {
        let (row_count, col_count) = (rows.len(), rows[0].len());
        let mut cols = Vec::with_capacity(col_count);
        for _ in 0..col_count {
            cols.push(Vec::with_capacity(row_count));
        }
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                cols[i].push(cell.len());
            }
        }
        cols
    };

    assert!(cols.len() > 0);
    for mut col in &mut cols {
        assert!(col.len() > 0);
        col.sort()
    }

    let mut search_vec: Vec<Vec<&usize>> = Vec::with_capacity(rows.len());
    for i in 0..rows.len() {
        search_vec.push((0..3).map(|j| &cols[j][i]).collect());
    }

    let col_widths: Vec<usize> = match search_vec.binary_search_by(|row| {
        row.iter().fold(0, |a, b| a + *b).cmp(&fit_width)
    }) {
        Ok(i) => return cols.iter().map(|x| x[i]).collect(),
        Err(0) => cols.iter().map(|x| x[0]).collect(), // not enough space
        Err(i) => cols.iter().map(|x| x[i-1]).collect(), // there is space left
    };

    let space_left = fit_width - col_widths.iter().fold(0, |a, b| a + b);
    let expand_units = space_left as f32 / expand_factors.iter().fold(0_f32, |a, b| a + b);
    col_widths.iter()
        .zip(expand_factors)
        .map(|(w, f)| w + ((expand_units*f).round() as usize))
        .collect()
}

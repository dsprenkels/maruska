extern crate env_logger;
extern crate libclient;
#[macro_use] extern crate log;
extern crate ncurses;

use std::char;
use libclient::it_works;

macro_rules! ncurses_cleanup {
    ( $contents:expr ) => {
        {
            ncurses::endwin();
            $contents
        }
    };
}


struct App {
    resultswindow: ncurses::WINDOW,
    querywindow: ncurses::WINDOW,
    query: String,
}

impl App {
    fn new() -> App {
        ncurses::initscr();
        ncurses::cbreak();
        ncurses::keypad(ncurses::stdscr, true);
        ncurses::noecho();
        App {
            resultswindow: ncurses::newwin(ncurses::LINES - 1, ncurses::COLS, 0, 0),
            querywindow: ncurses::newwin(1, ncurses::COLS, ncurses::LINES - 1, 0),
            query: String::new()
        }
    }

    fn mainloop(&mut self) {
        'mainloop:loop {
            self.handle_input(ncurses::getch());
        }
    }

    fn handle_input(&mut self, ch: i32) {
        match ch {
            ncurses::KEY_BACKSPACE => self.handle_input_backspace(ch),
            10 => self.handle_input_linefeed(ch),
            21 => self.handle_input_nak(ch),
            47 | 58 => self.handle_input_cmdtypechar(ch), // '/' and ':'
            32 ... 126 => self.handle_input_alphanum(ch),
            ch => ncurses_cleanup!(panic!("handling of keycode {} is not implemented", ch))
        }
    }

    fn handle_input_backspace(&mut self, _: i32) {
        self.query.pop();
        self.redraw_querywindow()
    }

    fn handle_input_linefeed(&mut self, _: i32) {
        ncurses_cleanup!(unimplemented!());
    }

    fn handle_input_nak(&mut self, _: i32) {
        if self.query.len() > 1 {
            self.query.truncate(1);
        } else {
            self.query.clear();
        }
        self.redraw_querywindow()
    }

    fn handle_input_cmdtypechar(&mut self, ch: i32) {
        if !self.query.is_empty() { return; }

        if self.query.len() == 0 {
            match ch {
                47 => self.query.push('/'),
                58 => self.query.push(':'),
                _ => ncurses_cleanup!(unreachable!()),
            }
        }
        self.redraw_querywindow()
    }

    fn handle_input_alphanum(&mut self, input_ch: i32) {
        let ch_option = char::from_u32(input_ch as u32);
        match ch_option {
            Some(ch) => {
                if self.query.is_empty() { self.query.push('/') };
                self.query.push(ch);
            },
            None => ncurses_cleanup!(unreachable!())
        }
        self.redraw_querywindow()
    }

    fn redraw_querywindow(&mut self) {
        // draw query field
        for i in 0..ncurses::COLS {
            ncurses::mvwaddch(self.querywindow, 0, i, ' ' as ncurses::chtype);
        }
        ncurses::mvwaddnstr(self.querywindow, 0, 0, &self.query, self.query.len() as i32);
        ncurses::wrefresh(self.querywindow);
    }
}

impl Drop for App {
    fn drop(&mut self) {
        ncurses::endwin();
    }
}

fn main() {
    // initialize logger
    if let Err(err) = env_logger::init() {
        panic!("Failed to initialize logger: {}", err);
    }

    // initialize locale (best guess)
    let locale_conf = ncurses::LcCategory::all;
    ncurses::setlocale(locale_conf, "");

    it_works();
    let mut app = App::new();
    app.mainloop();
}

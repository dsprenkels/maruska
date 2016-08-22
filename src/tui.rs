use std::borrow::Cow;
use std::char;
use std::cmp::{max, min};
use std::error::Error;
use std::fmt;
use std::iter::repeat;
use std::thread;

use chan;
use lru_time_cache::LruCache;
use regex::Regex;
use rustc_serialize::json::Json;
use strsim::levenshtein;
use termbox::*;
use time::{Duration, get_time};

use libclient::{Client, ClientError, md5, Message, RequestStatus};

macro_rules! cleanup {
    ( $ret:expr ) => {
        {
            unsafe { tb_shutdown() };
            $ret
        }
    }
}

macro_rules! cleanup_if {
    ( $cond:expr, $ret:expr ) => {
        {
            if $cond {
                unsafe { tb_shutdown() };
                $ret
            }
        }
    }
}

macro_rules! clean_assert {
    ( $cond:expr $(, $rest:expr )* ) => {
        {
            if !$cond {
                unsafe { tb_shutdown() };
                assert!($cond $(, $rest)* );
            }
        }
    }
}

const CMD_USERNAME: &'static str = "username";
const CMD_PASSWORD: &'static str = "password";
const CMD_QUIT: &'static str = "quit";
const COMMANDS: [&'static str; 3] = [
    CMD_USERNAME, CMD_PASSWORD, CMD_QUIT,
];
const MIN_STATUS_WIDTH: usize = 30;
const MAX_STATUS_WIDTH: usize = 60;
const STATUS_TIMEOUT_MILLIS: u64 = 5000;
const QM_BUFFER_SIZE: usize = 5000;

#[derive(Debug)]
pub enum TUIError {
    Client(ClientError),
    Quit,
}

enum Secret {
    AccessKey(String),
    PasswordHash(String),
}

enum StatusType {
    Info,    // blue
    Success, // green
    Warning, // yellow
    Error,   // red
}

pub struct TUI {
    client: Client,
    username: Option<String>,
    secret: Option<Secret>,
    results_offset: usize,
    results_focus: usize,
    query: String,
    status: LruCache<(), (Cow<'static, str>, StatusType)>,
}

impl fmt::Display for TUIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl From<ClientError> for TUIError {
    fn from(err: ClientError) -> Self {
        TUIError::Client(err)
    }
}

impl Error for TUIError {
    fn description(&self) -> &str {
        match *self {
            TUIError::Client(ref err) => err.description(),
            TUIError::Quit => "quit",
        }
    }
}

impl TUI {
    pub fn new(url: &str) -> Result<(TUI, (chan::Receiver<Json>,
                                    chan::Receiver<RawEvent>,
                                    chan::Receiver<chan::Sender<()>>)), TUIError> {
        // shadow the `Duration` from the one of the `time` crate
        use std::time::Duration;

        // initialize client
        let (mut client, client_r) = match Client::new(url) {
            Ok((client, client_r)) => (client, client_r),
            Err(err) => return Err(TUIError::from(err)),
        };
        client.follow_all();
        client.serve();

        // initialize user interface
        unsafe { tb_init(); }

        let status_ttl = Duration::from_millis(STATUS_TIMEOUT_MILLIS);
        let mut status = LruCache::with_expiry_duration_and_capacity(status_ttl, 1);
        status.insert((), (Cow::from(format!("Connected to {}", url)), StatusType::Success));
        let tui = TUI {
            client: client,
            username: None,
            secret: None,
            results_offset: 0,
            results_focus: 0,
            query: String::new(),
            status: status,
        };
        let tui_r = TUI::serve_events();

        let tick_r = chan::tick(Duration::from_secs(1));
        Ok((tui, (client_r, tui_r, tick_r)))
    }

    pub fn serve_events() -> chan::Receiver<RawEvent> {
        let (s, r) = chan::sync(0);
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

    fn try_login(&mut self) -> bool {
        match (&self.username, &self.secret) {
            (&Some(ref username), &Some(Secret::PasswordHash(ref secret))) =>
                self.client.do_login(username, secret),
            (&Some(ref username), &Some(Secret::AccessKey(ref secret))) =>
                self.client.do_login_accesskey(username, secret),
            _ => return false,
        };
        true
        // TODO show some visual feedback "logging in..."
    }

    fn update_client_query(&mut self) {
        if self.query.starts_with('/') {
            self.client.update_query(Some(&self.query[1..]), self.results_offset + QM_BUFFER_SIZE);
        } else {
            self.client.update_query(None, 0);
        }
    }

    fn do_request(&mut self) -> Result<(), TUIError> {
        clean_assert!(self.query.starts_with('/'));
        let media_key = {
            let ref results = self.client.get_qm_results().0;
            if results.len() == 0 {
                self.status.insert((), (Cow::from("No song selected"), StatusType::Warning));
                return Ok(());
            }
            results[self.results_focus].key.clone()
        };

        self.query.clear();
        match self.client.do_request_from_key(&media_key) {
            RequestStatus::Ok => {},
            RequestStatus::Deferred => {
                // Tell the user that logging in is needed
                self.status.insert((), (Cow::from("Not logged in"), StatusType::Warning));
                self.query.push_str(":username ");
            },
        }
        Ok(())
    }

    fn do_command(&mut self) -> Result<(), TUIError> {
        lazy_static! {
            static ref WORD: Regex = Regex::new(r#"\S+"#).unwrap();
        }
        clean_assert!(self.query.starts_with(':'));
        let (_, idx) = if let Some(m) = WORD.find(&self.query[1..]) {
            m
        } else {
            return Ok(()) // empty command, do nothing
        };
        let query = self.query.clone();
        let (command, rest) = query[1..].split_at(idx);
        let args = if rest.len() >= 1 {
            Some(&rest[1..])
        } else {
            None
        };

        match (command, args) {
            (CMD_USERNAME, args) => self.do_command_username(args),
            (CMD_PASSWORD, args) => self.do_command_password(args),
            (CMD_QUIT, args) => self.do_command_quit(args),
            (cmd, args) => self.do_invalid_command(cmd, args),
        }
    }

    fn do_command_username(&mut self, username_option: Option<&str>) -> Result<(), TUIError> {
        let username = username_option.unwrap_or_else(|| cleanup!(panic!("no username provided")));
        self.username = Some(username.to_string());
        self.query.clear();
        if !self.try_login() {
            self.query.push_str(":password ");
        }
        Ok(())
    }

    fn do_command_password(&mut self, password_option: Option<&str>) -> Result<(), TUIError> {
        if let Some(ref password) = password_option {
            self.secret = Some(Secret::PasswordHash(md5(password)));
            self.status.insert((), (Cow::from("Logging in"), StatusType::Info));
            self.try_login();
        } else {
            self.status.insert((), (Cow::from("No password provided"), StatusType::Error));
        }
        self.query.clear();
        Ok(())
    }

    fn do_command_quit(&self, _: Option<&str>) -> Result<(), TUIError> {
        Err(TUIError::Quit)
    }

    fn do_invalid_command(&mut self, cmd: &str, _: Option<&str>) -> Result<(), TUIError> {
        let commands = COMMANDS;
        let (other_cmd, dist) = commands.iter().map(|x| (x, levenshtein(x, &cmd)))
                                               .min_by_key(|x| x.1).unwrap();
        let msg = if dist < 3 {
            format!(r#"Not a command. Did you mean "{}"?"#, other_cmd)
        } else {
            format!(r#"Not a maruska command: "{}""#, cmd)
        };
        self.status.insert((), (Cow::from(msg), StatusType::Error));
        self.query.clear();
        Ok(())
    }

    fn move_focus(&mut self, x: isize, fix_offset: bool) {
        if self.query.starts_with('/') {
            self.move_results_focus(x, fix_offset)
        }
    }

    fn move_results_focus(&mut self, x: isize, fix_offset: bool) {
        fn bounded<T: Ord>(v1: T, v2: T, v3: T) -> T {
            max(v1, min(v2, v3))
        }
        let max_index = self.client.get_qm_results().0.len().saturating_sub(1);
        let h = self.get_viewport_height();

        let new_results_focus = if x >= 0 {
            (self.results_focus).saturating_add(x as usize)
        } else {
            (self.results_focus).saturating_sub(-x as usize)
        };
        self.results_focus = bounded(0, new_results_focus, max_index);

        let new_results_offset = if fix_offset {
            if x >= 0 {
                (self.results_offset).saturating_add(x as usize)
            } else {
                (self.results_offset).saturating_sub(-x as usize)
            }
        } else {
            self.results_offset
        };
        self.results_offset = bounded(self.results_focus.saturating_sub(h as usize - 1),
                                      new_results_offset, self.results_focus);

        self.update_client_query();
    }

    pub fn handle_message_from_client(&mut self, message: &Json) -> Result<(), ClientError> {
        self.client.handle_message(message).map(|x| match x {
            Message::QueryMediaResults => {
                self.move_results_focus(0, false); // reinit focus inside the new bounds
            },
            Message::Login => {
                self.status.insert((), (Cow::from("Succesfully logged in"), StatusType::Success));
            },
            Message::LoginError(ref msg) if msg == "User does not exist" => {
                let msg = format!("Login failed: user \"{}\" does not exist",
                                  self.username.as_ref().unwrap());
                self.status.insert((), (Cow::from(msg), StatusType::Error));

                // If the user has not given any input yet, reinsert ":username " into self.query
                if self.query.is_empty() {
                    self.query.push_str(":username ");
                }
            },
            Message::LoginError(ref msg) if msg == "Wrong password" => {
                let msg = "Login failed: wrong password";
                self.status.insert((), (Cow::from(msg), StatusType::Error));

                // Same as above, but with ":password "
                if self.query.is_empty() {
                    self.query.push_str(":password ");
                }
            },
            msg => {
                debug!("unhandled message from client: {:?}", msg);
            },
        })
    }

    pub fn handle_event(&mut self, event: RawEvent) -> Result<(), TUIError> {
        match event.etype {
            TB_EVENT_KEY => {
                if event.ch == 0 {
                    self.handle_input_key(event.key)
                } else {
                    self.handle_input_ch(event.ch)
                }
            },
            TB_EVENT_RESIZE => {
                trace!("ignoring resize event");
                Ok(())
            },
            TB_EVENT_MOUSE => {
                warn!("ignoring mouse event");
                Ok(())
            },
            _ => {
                error!("ingoring unknown event type {}", event.etype);
                Ok(())
            },
        }
    }

    fn handle_input_ch(&mut self, ch: u32) -> Result<(), TUIError> {
        let ret = match ch {
            47 | 58 => self.handle_input_cmdtypechar(ch),
            33 ... 126 => self.handle_input_alphanum(ch),
            ch => {
                error!("unimplemented keycode {}", ch);
                unimplemented!();
            },
        };
        self.status.clear();
        ret
    }

    fn handle_input_key(&mut self, key: u16) -> Result<(), TUIError> {
        // TODO Page {up, down} should self.results_offset -= (-)self.height()
        //      and put the current focus at the entry closes to the new bounds
        match key {
            TB_KEY_ARROW_UP => self.handle_arrow_up(),
            TB_KEY_ARROW_DOWN => self.handle_arrow_down(),
            TB_KEY_PGUP => self.handle_page_up(),
            TB_KEY_PGDN => self.handle_page_down(),
            TB_KEY_ENTER => self.handle_input_submit(key),
            TB_KEY_SPACE => self.handle_input_alphanum(' ' as u32),
            TB_KEY_BACKSPACE | TB_KEY_BACKSPACE2 => self.handle_input_backspace(key),
            TB_KEY_CTRL_C => Err(TUIError::Quit),
            TB_KEY_CTRL_W => self.handle_input_delword(key),
            TB_KEY_CTRL_U => self.handle_input_nak(key),
            key => {
                warn!("ignoring unhandled keycode {}", key);
                Ok(())
            },
        }
    }

    fn handle_arrow_up(&mut self) -> Result<(), TUIError> {
        self.move_focus(-1, false);
        Ok(())
    }

    fn handle_arrow_down(&mut self) -> Result<(), TUIError> {
        self.move_focus(1, false);
        Ok(())
    }

    fn handle_page_up(&mut self) -> Result<(), TUIError> {
        let h = self.get_viewport_height() as isize;
        self.move_focus(-h, true);
        Ok(())
    }

    fn handle_page_down(&mut self) -> Result<(), TUIError> {
        let h = self.get_viewport_height() as isize;
        self.move_focus(h, true);
        Ok(())
    }

    fn handle_input_backspace(&mut self, _: u16) -> Result<(), TUIError> {
        self.query.pop();
        self.update_client_query();
        Ok(())
    }

    fn handle_input_submit(&mut self, _: u16) -> Result<(), TUIError> {
        match &self.query.chars().nth(0) {
            &Some('/') => self.do_request(),
            &Some(':') => self.do_command(),
            &Some(_) => cleanup!(unreachable!()),
            &None => Ok(()), // do nothing
        }
    }

    fn handle_input_delword(&mut self, _: u16) -> Result<(), TUIError> {
        lazy_static! {
            static ref WORD: Regex = Regex::new(r#"\S+"#).unwrap();
        }
        match WORD.find_iter(&self.query).last() {
            _ if self.query.len() == 1 => self.query.clear(),
            Some((start, _)) => self.query.truncate(max(start, 1)),
            None => {},
        };
        self.update_client_query();
        Ok(())
    }

    fn handle_input_nak(&mut self, _: u16) -> Result<(), TUIError> {
        if self.query.len() > 1 {
            self.query.truncate(1);
        } else {
            self.query.clear();
        }
        self.update_client_query();
        Ok(())
    }

    fn handle_input_cmdtypechar(&mut self, ch: u32) -> Result<(), TUIError> {
        if !self.query.is_empty() { return Ok(()); }

        if self.query.len() == 0 {
            match ch {
                47 => {
                    self.query.push('/');
                    self.update_client_query();
                },
                58 => self.query.push(':'),
                _ => cleanup!(unreachable!()),
            }
        }
        Ok(())
    }

    fn handle_input_alphanum(&mut self, input_ch: u32) -> Result<(), TUIError> {
        let ch_option = char::from_u32(input_ch as u32);
        match ch_option {
            Some(ch) => {
                if self.query.is_empty() { self.query.push('/') };
                self.query.push(ch);
            },
            None => cleanup!(unreachable!()),
        }
        self.update_client_query();
        Ok(())
    }

    unsafe fn print(&self, x: i32, y: i32, fg: u16, bg: u16, s: &str, maxlen: usize,
                             trunc_fg: u16, trunc_bg: u16, trunc_s: &str) {
        if s.len() <= maxlen || s.is_empty() {
            for (i, ch) in s.chars().chain(repeat(' ')).take(maxlen).enumerate() {
                tb_change_cell(x+i as i32, y, ch as u32, fg, bg);
            }
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

    pub fn draw(&mut self) {
        unsafe { tb_clear(); }
        if self.query.starts_with('/') {
            self.draw_search_results();
        } else {
            self.draw_current_requests();
        }
        self.draw_query();
        self.draw_status();
        unsafe { tb_present(); }
    }

    fn draw_current_requests<'a>(&'a mut self) {
        let (w, h) = self.get_viewport_size();
        let mut str_table: Vec<Vec<Cow<'a, str>>> = Vec::new();

        // first line shows currently playing song
        let mut queue_length = Duration::zero();
        str_table.push(if let &Some(ref playing) = self.client.get_playing() {
            let requested_by = String::from(unwrap_requested_by(&playing.requested_by));
            queue_length = queue_length + (playing.end_time - get_time());
            vec!(Cow::from(requested_by),
                 Cow::from(playing.media.artist.as_ref()),
                 Cow::from(playing.media.title.as_ref()),
                 Cow::from(format_duration(queue_length)))
        } else {
            repeat(Cow::from("")).take(4).collect()
        });

        // rest shows the current request queue, offset is ignored (-> 0)
        if let Some(ref requests) = *self.client.get_requests() {
            for request in requests.iter().take(h as usize - 1) {
                let requested_by = String::from(unwrap_requested_by(&request.by));
                let media = &request.media;
                queue_length = queue_length + media.length;;
                str_table.push(vec!(Cow::from(requested_by),
                                    Cow::from(media.artist.clone()),
                                    Cow::from(media.title.clone()),
                                    Cow::from(format_duration(queue_length))))
            }
        }

        // get optimal column widths
        let col_widths = fit_columns(&str_table, &[1f32, 4f32, 4f32, 1f32], w as usize);

        // do the actual drawing
        self.draw_table(0, str_table.iter(), &col_widths, (TB_DEFAULT, TB_BLUE, TB_DEFAULT), None);
    }

    fn draw_search_results<'a>(&'a mut self) {
        // TODO Show blue tildes '~' (as in vim) at the end of the range.
        let (w, h) = self.get_viewport_size();
        let mut str_table: Vec<Vec<Cow<'a, str>>> = Vec::new();

        let (results, qm_done) = self.client.get_qm_results();
        for media in results.iter().skip(self.results_offset).take(h as usize) {
            str_table.push(vec!(Cow::from(media.artist.as_ref()),
                                Cow::from(media.title.as_ref())));
        }

        let col_widths = fit_columns(&str_table, &[1f32, 1f32], w as usize);
        let selected = self.results_focus - self.results_offset;
        let selection = Some((selected, (TB_BLACK, TB_BLUE, TB_WHITE)));
        self.draw_table(0, str_table.iter(), &col_widths, (TB_DEFAULT, TB_BLUE, TB_DEFAULT),
                        selection);

                        // (TB_BLACK, TB_BLUE, TB_WHITE)
                        // (TB_DEFAULT, TB_BLUE, TB_DEFAULT)


        if *qm_done {
            // Fill up the rest with blue tildes to indicate end-of-file
            let row = vec!(Cow::from("~"));
            let from_row = results.iter()
                              .skip(self.results_offset)
                              .take(h as usize)
                              .count();
            assert!(from_row <= h as usize);

            let str_table = repeat(&row).take(h as usize - from_row);
            let style = (TB_BOLD | TB_BLUE, TB_BOLD | TB_BLUE, TB_DEFAULT);
            self.draw_table(from_row, str_table, &col_widths, style, None);
        }
    }

    fn draw_table<'a, T>(&self, offset: usize, str_table: T, col_widths: &Vec<usize>,
                         style: (u16, u16, u16),
                         selected: Option<(usize, (u16, u16, u16))>)
        where T : Iterator<Item=&'a Vec<Cow<'a, str>>> {
        for (y, row) in str_table.enumerate() {
            let (fg, fg2, bg) = selected.map_or(style, |(s, selected_style)| {
                if s == y { selected_style } else { style }
            });
            for (j, cell) in row.iter().enumerate() {
                assert!(j <= col_widths.len());
                let x = col_widths.iter().take(j).fold(0, |a, b| a + b);
                let maxlen = col_widths[j];
                unsafe {
                    self.print(x as i32, (y + offset) as i32, fg, bg, cell, maxlen, fg2, bg, "$");
                }
            }
        }
    }

    fn draw_query(&mut self) {
        // draw query field
        let (w, h) = self.get_viewport_size();
        let maxwidth: usize = if self.status.peek(&()).is_some() {
            (w as usize).saturating_sub(MAX_STATUS_WIDTH)
        } else {
            w as usize
        };
        let ref substr = format!(":{} ", CMD_PASSWORD);
        let query = if self.query.starts_with(substr) {
            let substr_len = substr.len();
            format!(":{} {}", CMD_PASSWORD, self.query
                .chars()
                .skip(substr_len)
                .map(|_| '*')
                .collect::<String>())
        } else {
            self.query.clone()
        };

        if let Some(cmd) = COMMANDS.iter().fold(None, |opt, cmd| opt.or_else(
            || if query == format!(":{}", cmd) ||
                  query.starts_with(&format!(":{} ", cmd)) { Some(cmd) } else { None }
        )) {
            // print command bold
            let cmdlen = cmd.len();
            unsafe {
                self.print(0, h, TB_DEFAULT, TB_DEFAULT, &query[0..1], maxwidth,
                           TB_DEFAULT, TB_BLUE, "$");
                self.print(1, h, TB_BOLD, TB_DEFAULT, &query[1..1+cmdlen], maxwidth - 1,
                           TB_DEFAULT, TB_BLUE, "$");
                self.print(cmdlen as i32 + 1, h, TB_DEFAULT, TB_DEFAULT, &query[1+cmdlen..],
                           maxwidth - 1 - cmdlen, TB_DEFAULT, TB_BLUE, "$");
            }
        } else {
            unsafe {
                self.print(0, h, TB_DEFAULT, TB_DEFAULT, &query,
                           maxwidth as usize, TB_DEFAULT, TB_DEFAULT, "$");
            }
        }

        // update cursor
        unsafe {
            tb_set_cursor(self.query.len() as i32, h);
        }
    }

    fn draw_status(&self) {
        if let Some(&(ref status, ref ty)) = self.status.peek(&()) {
            let (w, h) = self.get_viewport_size();
            let status_width = min(max(MIN_STATUS_WIDTH, status.len()), MAX_STATUS_WIDTH);
            let offset = (w as usize).saturating_sub(status_width);
            let maxwidth = w as usize - offset;
            let fg = match *ty {
                StatusType::Info => TB_BLUE,
                StatusType::Success => TB_GREEN,
                StatusType::Warning => TB_YELLOW,
                StatusType::Error => TB_RED,
            } | TB_BOLD;
            let bg = TB_DEFAULT;
            unsafe {
                self.print(offset as i32, h, fg, bg, &status,
                           maxwidth, TB_BLUE, bg, "$");
            }
        }
    }

    fn get_width(&self) -> i32 {
        unsafe { tb_width() as i32 }
    }

    fn get_height(&self) -> i32 {
        unsafe { tb_height() as i32 }
    }

    fn get_size(&self) -> (i32, i32) {
        (self.get_width(), self.get_height())
    }

    fn get_viewport_width(&self) -> i32 {
        self.get_width()
    }

    fn get_viewport_height(&self) -> i32 {
        match self.get_height().checked_sub(1) {
            Some(h) => h,
            None => cleanup!(panic!("viewport height is too small")),
        }
    }

    fn get_viewport_size(&self) -> (i32, i32) {
        (self.get_viewport_width(), self.get_viewport_height())
    }

}

impl Drop for TUI {
    fn drop(&mut self) {
        unsafe { tb_shutdown() };
    }
}

fn unwrap_requested_by<'a>(requested_by: &'a Option<String>) -> &'a str {
    match *requested_by {
        Some(ref by) => &by,
        None => "marietje"
    }
}

fn format_duration(d: Duration) -> String {
    match () {
        _ if d.num_days() != 0 => format!("{}d{:02}:{:02}:{:02}",
            d.num_days(), d.num_hours() % 24, d.num_minutes() % 60, d.num_seconds() % 60),
        _ if d.num_hours() != 0 => format!("{}:{:02}:{:02}",
            d.num_hours(), d.num_minutes() % 60, d.num_seconds() % 60),
        _ =>  format!("{}:{:02}", d.num_minutes(), d.num_seconds() % 60)
    }
}

fn fit_columns<'a>(rows: &Vec<Vec<Cow<'a, str>>>, expand_factors: &[f32], fit_width: usize) -> Vec<usize> {
    let col_count = expand_factors.len();
    let mut cols = {
        let row_count = rows.len();
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
        col.sort()
    }

    let mut search_vec: Vec<Vec<&usize>> = Vec::with_capacity(rows.len());
    for i in 0..rows.len() {
        search_vec.push((0..col_count).map(|j| {
            if let Some(val) = cols.get(j).and_then(|x| x.get(i)) {
                val
            } else {
                cleanup!(panic!("array indexing failure, array: {:?}", cols));
            }
        }).collect());
    }

    let col_widths: Vec<usize> = match search_vec.binary_search_by(|row| {
        row.iter().fold(0, |a, b| a + *b).cmp(&fit_width)
    }) {
        Ok(i) => return cols.iter().map(|x| {
            if let Some(val) = x.get(i) {
                *val
            } else {
                cleanup!(panic!("array indexing failure"));
            }
        }).collect(),
        Err(0) => cols.iter().map(|_|
            0
        ).collect(), // not enough space
        Err(i) if i == search_vec.len() => cols.iter().map(|_|
            0
        ).collect(), // enough space for all rows
        Err(i) => cols.iter().map(|x| {
            if let Some(val) = x.get(i - 1) {
                *val
            } else {
                cleanup!(panic!("array indexing failure"));
            }
        }).collect(), // there is space left
    };

    let space_left = fit_width - col_widths.iter().fold(0, |a, b| a + b);
    let expand_units = space_left as f32 / expand_factors.iter().fold(0f32, |a, b| a + b);
    col_widths.iter()
        .zip(expand_factors)
        .map(|(w, f)| w + ((expand_units*f).round() as usize))
        .collect()
}

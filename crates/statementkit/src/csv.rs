//! Incremental CSV reader: byte chunks in, complete records out.
//!
//! Quoted fields, doubled-quote escapes, embedded commas and newlines,
//! LF/CRLF, leading UTF-8 BOM. Malformed input (invalid UTF-8, quote
//! unterminated at end) drops the affected record and increments `bad`;
//! nothing is passed on half-parsed.

#[derive(Default)]
pub struct Csv {
    field: Vec<u8>,
    record: Vec<String>,
    state: State,
    poisoned: bool,
    pending_cr: bool,
    bom_acc: Vec<u8>,
    bom_done: bool,
    /// Records dropped for invalid UTF-8 or an unterminated quote.
    pub bad: usize,
}

#[derive(Default, Clone, Copy, PartialEq)]
enum State {
    #[default]
    Start,
    Field,
    Quoted,
    QuoteIn,
}

impl Csv {
    /// Feed one chunk; every completed record is handed to `sink`.
    pub fn feed(&mut self, data: &[u8], sink: &mut impl FnMut(Vec<String>)) {
        let mut data = data;
        if !self.bom_done {
            let take = (3 - self.bom_acc.len()).min(data.len());
            self.bom_acc.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.bom_acc.len() < 3 {
                return;
            }
            self.bom_done = true;
            let head = std::mem::take(&mut self.bom_acc);
            let replay: &[u8] = if head == b"\xEF\xBB\xBF" { &[] } else { &head };
            for &b in replay.iter().chain(data) {
                self.byte(b, sink);
            }
            return;
        }
        for &b in data {
            self.byte(b, sink);
        }
    }

    /// End of stream: flush a final record that lacks a trailing newline.
    pub fn finish(&mut self, sink: &mut impl FnMut(Vec<String>)) {
        if !self.bom_done {
            self.bom_done = true;
            let head = std::mem::take(&mut self.bom_acc);
            for &b in &head {
                self.byte(b, sink);
            }
        }
        if self.state == State::Quoted || self.state == State::QuoteIn {
            self.state = State::Start;
            self.field.clear();
            self.record.clear();
            self.poisoned = false;
            self.bad += 1;
            return;
        }
        if self.state != State::Start || !self.field.is_empty() || !self.record.is_empty() {
            self.end_record(sink);
        }
    }

    fn byte(&mut self, b: u8, sink: &mut impl FnMut(Vec<String>)) {
        if self.pending_cr {
            self.pending_cr = false;
            if b == b'\n' {
                return;
            }
        }
        match self.state {
            State::Quoted => match b {
                b'"' => self.state = State::QuoteIn,
                _ => self.field.push(b),
            },
            State::QuoteIn => match b {
                b'"' => {
                    self.field.push(b'"');
                    self.state = State::Quoted;
                }
                b',' => {
                    self.end_field();
                    self.state = State::Field;
                }
                b'\n' => self.end_record(sink),
                b'\r' => {
                    self.pending_cr = true;
                    self.end_record(sink);
                }
                _ => {
                    self.field.push(b);
                    self.state = State::Field;
                }
            },
            State::Start | State::Field => match b {
                b'"' if self.field.is_empty() => self.state = State::Quoted,
                b'"' => self.field.push(b),
                b',' => {
                    self.end_field();
                    self.state = State::Field;
                }
                b'\n' => self.end_record(sink),
                b'\r' => {
                    self.pending_cr = true;
                    self.end_record(sink);
                }
                _ => {
                    self.field.push(b);
                    self.state = State::Field;
                }
            },
        }
    }

    fn end_field(&mut self) {
        let bytes = std::mem::take(&mut self.field);
        match String::from_utf8(bytes) {
            Ok(s) => self.record.push(s),
            Err(_) => {
                self.poisoned = true;
                self.record.push(String::new());
            }
        }
    }

    fn end_record(&mut self, sink: &mut impl FnMut(Vec<String>)) {
        self.state = State::Start;
        if self.record.is_empty() && self.field.is_empty() && !self.poisoned {
            return;
        }
        self.end_field();
        let rec = std::mem::take(&mut self.record);
        let poisoned = std::mem::replace(&mut self.poisoned, false);
        if poisoned {
            self.bad += 1;
            return;
        }
        if rec.len() == 1 && rec[0].is_empty() {
            return;
        }
        sink(rec);
    }
}

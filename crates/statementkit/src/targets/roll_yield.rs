//! `roll-yield`: what staying in futures costs.
//!
//! Contract identity is root + month letter + year code (`ESM6` —
//! month letters F H K M Q U V N Z (G J C rare), year digit resolved
//! to the decade nearest the statement period). The multiplier is
//! the verified identity `|Notional ÷ (Price × Quantity)|` — an
//! exact integer on every futures fill — with `Open Positions.Mult`
//! by root as fallback.
//!
//! A roll is INFERENCE: no roll label exists anywhere. A close
//! (`Code ∋ {C,A}`) in contract M pairs with the earliest opposite
//! open (`Code ∋ {O}`) in the next contract month of the same root,
//! within 5 days, at equal-or-greater remaining quantity; the
//! matched-size part is the roll and any resize remainder stays a
//! regular open/close. Unmatched closes stay out of the roll series.
//!
//! Roll cost = ±(new price − old price) × multiplier × matched
//! quantity, `+` for a long book (pays in contango), flipped for a
//! short. Basis gap = Δprice ÷ old price, annualized by the months
//! of contract separation; also as a share of that file's NAV.

use crate::dates;
use crate::dec::Dec;
use crate::json;
use crate::json::Value;
use crate::target::{RowRef, Target};

const MONTHS: &[u8] = b"FHKMQUVNZGJC";

#[derive(Clone)]
struct Contract {
    root: String,
    expiry_days: i64, // first of contract month
    label: String,
}

struct Fill {
    day: i64,
    contract: Contract,
    qty: Dec,
    price: Dec,
    mult: Option<Dec>,
    remaining: Dec,
    close: bool,
}

#[derive(Default)]
struct FileRec {
    nav: Option<Dec>,
    period_end: i64,
}

pub struct RollYield {
    fills: Vec<Fill>,
    file: FileRec,
    files: Vec<FileRec>,
    mult_by_root: Vec<(String, Dec)>,
    no_mult: usize,
}

impl RollYield {
    pub fn new() -> Self {
        RollYield {
            fills: Vec::new(),
            file: FileRec::default(),
            files: Vec::new(),
            mult_by_root: Vec::new(),
            no_mult: 0,
        }
    }
}

fn dec(raw: Option<&str>) -> Option<Dec> {
    raw.and_then(|r| Dec::parse(r).ok())
}

fn dabs(d: Dec) -> Dec {
    if d < Dec::ZERO { Dec::ZERO - d } else { d }
}

/// Parse `ROOT` + month letter + year digit; June 2026 = `MESU6`.
fn parse_contract(symbol: &str, period_end: i64) -> Option<Contract> {
    let b = symbol.as_bytes();
    if symbol.contains(' ') || b.len() < 3 {
        return None;
    }
    let m = MONTHS.iter().position(|&c| c == b[b.len() - 2])? as i64;
    let d = (b[b.len() - 1] as char).to_digit(10)? as i64;
    let end_iso = dates::days_to_iso(period_end);
    let end_year: i64 = end_iso[..4].parse().ok()?;
    // nearest decade-consistent year to the period end
    let mut best = None;
    for y in [end_year - 10, end_year, end_year + 10] {
        let cand = if y % 10 <= d {
            y + (d - y % 10)
        } else {
            y + (10 + d - y % 10)
        };
        let prev = cand - 10;
        for c in [prev, cand] {
            if c % 10 == d
                && best
                    .map(|b: i64| (c - end_year).abs() < (b - end_year).abs())
                    .unwrap_or(true)
            {
                best = Some(c);
            }
        }
    }
    let year = best?;
    let month = m + 1;
    let expiry_days = dates::iso_to_days(&format!("{year:04}-{month:02}-01"))?;
    Some(Contract {
        root: symbol[..symbol.len() - 2].to_string(),
        expiry_days,
        label: symbol.to_string(),
    })
}

impl Target for RollYield {
    fn name(&self) -> &'static str {
        "roll-yield"
    }

    fn on_meta(&mut self, key: &str, value: &str) {
        if key == "Period" {
            self.file.period_end = dates::stated_period(value)
                .and_then(|(_, e, _)| e)
                .and_then(|e| dates::iso_to_days(&e))
                .unwrap_or(0);
        }
    }

    fn on_file_end(&mut self) {
        self.files.push(std::mem::take(&mut self.file));
    }

    fn on_row(&mut self, row: &RowRef) {
        match row.title {
            "Change in Combined NAV" => {
                if row.cell("Key").unwrap_or("") == "Ending Value" {
                    self.file.nav = dec(row.cell("Value"));
                }
            }
            "Open Positions" => {
                if let (Some(sym), Some(mult)) = (row.cell("Symbol"), dec(row.cell("Mult"))) {
                    if let Some(c) = parse_contract(sym, self.file.period_end) {
                        if !self.mult_by_root.iter().any(|(r, _)| *r == c.root) {
                            self.mult_by_root.push((c.root, mult));
                        }
                    }
                }
            }
            "Trades" => {
                let (Some(sym), Some(qty), Some(code)) = (
                    row.cell("Symbol"),
                    dec(row.cell("Quantity")),
                    row.cell("Code"),
                ) else {
                    return;
                };
                if code.contains("IA") || code.contains("IM") {
                    return;
                }
                let close = code.contains('C') || code.contains('A');
                let open = !close && code.contains('O');
                if !close && !open {
                    return;
                }
                let Some(day) = row.cell("Date/Time").and_then(dates::date_to_days) else {
                    return;
                };
                let Some(contract) = parse_contract(sym, self.file.period_end) else {
                    return;
                };
                let Some(price) =
                    dec(row.cell("T. Price")).or_else(|| dec(row.cell("Trade Price")))
                else {
                    return;
                };
                let mult = dec(row.cell("Notional Value"))
                    .and_then(|n| {
                        let q = (price * qty).to_f64().abs();
                        if q == 0.0 {
                            return None;
                        }
                        let r = n.to_f64().abs() / q;
                        let i = (r + 0.5) as i64;
                        if ((r - i as f64).abs() < 1e-6) && i > 0 {
                            Some(Dec::parse(&i.to_string()).ok()?)
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        self.mult_by_root
                            .iter()
                            .find(|(r, _)| *r == contract.root)
                            .map(|(_, m)| *m)
                    });
                if mult.is_none() {
                    self.no_mult += 1;
                }
                self.fills.push(Fill {
                    day,
                    contract,
                    qty,
                    price,
                    mult,
                    remaining: dabs(qty),
                    close,
                });
            }
            _ => {}
        }
    }

    fn report(&self) -> Value {
        let mut closes: Vec<&Fill> = self.fills.iter().filter(|f| f.close).collect();
        let mut opens: Vec<&Fill> = self.fills.iter().filter(|f| !f.close).collect();
        closes.sort_by_key(|f| (f.day, f.contract.expiry_days));
        opens.sort_by_key(|f| (f.day, f.contract.expiry_days));
        let mut open_rem: Vec<Dec> = opens.iter().map(|f| f.remaining).collect();
        let mut rolls: Vec<Value> = Vec::new();
        let mut unmatched_closes = 0usize;
        let mut total_cost = Dec::ZERO;
        for c in &closes {
            let long_book = c.qty < Dec::ZERO;
            let mut need = c.remaining;
            for i in 0..opens.len() {
                if need <= Dec::ZERO {
                    break;
                }
                let o = opens[i];
                if open_rem[i] <= Dec::ZERO {
                    continue;
                }
                if o.contract.root != c.contract.root {
                    continue;
                }
                if o.contract.expiry_days <= c.contract.expiry_days {
                    continue;
                }
                if (o.day - c.day).abs() > 5 {
                    continue;
                }
                if long_book != (o.qty > Dec::ZERO) {
                    continue;
                }
                let take = if open_rem[i] < need {
                    open_rem[i]
                } else {
                    need
                };
                let Some(mult_c) = c.mult else {
                    continue;
                };
                let gap = o.price - c.price;
                let cost = if long_book {
                    gap * mult_c * take
                } else {
                    Dec::ZERO - gap * mult_c * take
                };
                open_rem[i] = open_rem[i] - take;
                need = need - take;
                total_cost = total_cost + cost;
                let months = ((o.contract.expiry_days - c.contract.expiry_days) / 30).max(1);
                let gap_pct = gap.to_f64() / c.price.to_f64();
                let ann = gap_pct * 12.0 / months as f64;
                let mut obj = vec![
                    (
                        "flat_date".to_string(),
                        Value::Str(dates::days_to_iso(c.day)),
                    ),
                    (
                        "closing_fill".to_string(),
                        Value::Str(c.contract.label.clone()),
                    ),
                    (
                        "next_contract".to_string(),
                        Value::Str(o.contract.label.clone()),
                    ),
                    ("close_price".to_string(), Value::num(&c.price)),
                    ("open_price".to_string(), Value::num(&o.price)),
                    ("quantity".to_string(), Value::num(&take)),
                    ("multiplier".to_string(), Value::num(&mult_c)),
                    ("roll_cost".to_string(), Value::num(&cost)),
                    ("gap_pct".to_string(), Value::Num(json::ratio_text(gap_pct))),
                    (
                        "gap_annualized".to_string(),
                        Value::Num(json::ratio_text(ann)),
                    ),
                ];
                if let Some(nav) = self.file_nav(c) {
                    if nav > Dec::ZERO {
                        obj.push((
                            "cost_pct_of_nav".to_string(),
                            Value::Num(json::ratio_text(cost.to_f64() / nav.to_f64())),
                        ));
                    }
                }
                rolls.push(Value::Obj(obj));
            }
            if need > Dec::ZERO {
                unmatched_closes += 1;
            }
        }
        let mut notes = vec![
            Value::Str("rolls are inferred: root + next-month symbol + quantity match; no roll label exists in any file".to_string()),
            Value::Str("unmatched closes stay out of the roll series; resize remainders are regular opens/closes".to_string()),
            Value::Str("multiplier from |Notional / (Price x Quantity)| exact-integer identity, Open Positions Mult by root as fallback".to_string()),
        ];
        if self.no_mult > 0 {
            notes.push(Value::Str(format!(
                "futures fills with no multiplier source (excluded from roll costs): {}",
                self.no_mult
            )));
        }
        Value::Obj(vec![
            ("rolls".to_string(), Value::Arr(rolls)),
            (
                "unmatched_closes".to_string(),
                Value::Num(unmatched_closes.to_string()),
            ),
            ("total_roll_cost".to_string(), Value::num(&total_cost)),
            ("notes".to_string(), Value::Arr(notes)),
        ])
    }
}

impl RollYield {
    /// NAV of the file a close came from would need per-fill file
    /// tracking; single-file sessions (the annual/monthly roll
    /// windows) use the last file's NAV.
    fn file_nav(&self, _f: &Fill) -> Option<Dec> {
        self.files.iter().rev().find_map(|f| f.nav)
    }
}

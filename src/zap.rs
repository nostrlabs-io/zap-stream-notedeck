use crate::note_util::NoteUtil;
use fixed_decimal::FixedDecimal;
use icu::decimal::FixedDecimalFormatter;
use icu::locid::Locale;
use nostr::{Event, JsonUtil, Kind, TagStandard};
use nostrdb::Note;

pub struct Zap<'a> {
    pub sender: [u8; 32],
    pub receiver: [u8; 32],
    pub zapper_service: &'a [u8; 32],
    pub amount: u64,
    pub message: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NotAZap;

impl<'a> Zap<'a> {
    pub fn from_receipt(event: Note<'a>) -> Result<Zap<'a>, NotAZap> {
        if event.kind() != 9735 {
            return Err(NotAZap);
        }

        let req_json = event.get_tag_value("description").ok_or(NotAZap)?;
        let req = Event::from_json(req_json.variant().str().ok_or(NotAZap)?).map_err(|_| NotAZap)?;

        if req.kind != Kind::ZapRequest {
            return Err(NotAZap);
        }

        let dest = req
            .tags
            .iter()
            .find_map(|t| match t.as_standardized() {
                Some(TagStandard::PublicKey { public_key, .. }) => Some(public_key.to_bytes()),
                _ => None,
            })
            .ok_or(NotAZap)?;

        let amount = req.tags.iter().find_map(|t| match t.as_standardized() {
            Some(TagStandard::Amount { millisats, .. }) => Some(*millisats),
            _ => None,
        });

        Ok(Zap {
            sender: req.pubkey.to_bytes(),
            receiver: dest,
            zapper_service: event.pubkey(),
            amount: amount.unwrap_or(0u64),
            message: req.content,
        })
    }
}

pub fn format_sats(n: f32) -> String {
    let (div_n, suffix) = if n >= 1_000. && n < 1_000_000. {
        (1_000., "K")
    } else if n >= 1_000_000. {
        (1_000_000., "M")
    } else {
        (1., "")
    };

    let fmt = FixedDecimalFormatter::try_new(&Locale::UND.into(), Default::default()).expect("icu");
    let d: FixedDecimal = (n / div_n).to_string().parse().expect("fixed decimal");
    format!("{}{}", fmt.format_to_string(&d), suffix)
}

pub fn top_senders(zaps: &[Zap<'_>], limit: usize) -> Vec<([u8; 32], u64)> {
    let mut by_sender: std::collections::HashMap<[u8; 32], u64> = Default::default();
    for z in zaps {
        *by_sender.entry(z.sender).or_default() += z.amount / 1000;
    }
    let mut list: Vec<_> = by_sender.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    list.truncate(limit);
    list
}

pub const SENDER_TOTALS_BATCH: usize = 6;

#[derive(Default)]
pub struct SenderTotals<K: std::hash::Hash + Eq + Copy> {
    seen: std::collections::HashSet<K>,
    by_sender: std::collections::HashMap<[u8; 32], u64>,
    total: u64,
    dirty: bool,
}

impl<K: std::hash::Hash + Eq + Copy> SenderTotals<K> {
    pub fn new() -> Self {
        Self {
            seen: Default::default(),
            by_sender: Default::default(),
            total: 0,
            dirty: false,
        }
    }

    pub fn absorb<'a, F>(&mut self, keys: impl Iterator<Item = &'a K>, batch: usize, mut parse: F) -> bool
    where
        K: 'a,
        F: FnMut(K) -> Option<([u8; 32], u64)>,
    {
        let mut parsed = 0;
        for k in keys {
            if self.seen.contains(k) {
                continue;
            }
            if parsed >= batch {
                return true;
            }
            parsed += 1;
            self.seen.insert(*k);
            if let Some((sender, msats)) = parse(*k) {
                let sats = msats / 1000;
                self.total += sats;
                *self.by_sender.entry(sender).or_default() += sats;
                self.dirty = true;
            }
        }
        false
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn top(&self, limit: usize) -> Vec<([u8; 32], u64)> {
        let mut list: Vec<_> = self.by_sender.iter().map(|(k, v)| (*k, *v)).collect();
        list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        list.truncate(limit);
        list
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zap(sender: u8, msats: u64) -> Zap<'static> {
        Zap {
            sender: [sender; 32],
            receiver: [9; 32],
            zapper_service: &[0; 32],
            amount: msats,
            message: String::new(),
        }
    }

    #[test]
    fn top_senders_sums_and_orders() {
        let zaps = vec![zap(1, 1_000), zap(2, 5_000), zap(1, 6_000), zap(3, 500)];
        let top = top_senders(&zaps, 2);
        assert_eq!(top, vec![([1; 32], 7), ([2; 32], 5)]);
    }

    #[test]
    fn sender_totals_parses_each_note_once_and_in_batches() {
        let keys: Vec<u32> = (0..10).collect();
        let mut totals = SenderTotals::new();
        let mut calls = 0;
        let mut parse = |k: u32| {
            calls += 1;
            Some(([(k % 2) as u8; 32], 2_000))
        };
        assert!(totals.absorb(keys.iter(), 4, &mut parse));
        assert!(totals.absorb(keys.iter(), 4, &mut parse));
        assert!(!totals.absorb(keys.iter(), 4, &mut parse));
        assert!(!totals.absorb(keys.iter(), 4, &mut parse));
        assert_eq!(calls, 10);
        assert_eq!(totals.total(), 20);
        assert_eq!(totals.top(1), vec![([0; 32], 10)]);
        assert!(totals.take_dirty());
        assert!(!totals.take_dirty());
    }
}

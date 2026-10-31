use crate::note_util::NoteUtil;
use anyhow::{anyhow, bail, Result};
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

impl<'a> Zap<'a> {
    pub fn from_receipt(event: Note<'a>) -> Result<Zap> {
        if event.kind() != 9735 {
            bail!("not a zap receipt");
        }

        let req_json = event
            .get_tag_value("description")
            .ok_or(anyhow!("missing description"))?;
        let req = Event::from_json(
            req_json
                .variant()
                .str()
                .ok_or(anyhow!("empty description"))?,
        )?;

        if req.kind != Kind::ZapRequest {
            bail!("not a zap request");
        }

        let dest = req
            .tags
            .iter()
            .find_map(|t| match t.as_standardized() {
                Some(TagStandard::PublicKey { public_key, .. }) => Some(public_key.to_bytes()),
                _ => None,
            })
            .ok_or(anyhow!("missing p tag in zap request"))?;

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
}

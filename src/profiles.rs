use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

static DEMANDED: OnceLock<Mutex<HashSet<[u8; 32]>>> = OnceLock::new();

pub fn first_demand(pubkey: &[u8; 32]) -> bool {
    DEMANDED
        .get_or_init(Default::default)
        .lock()
        .map(|mut s| s.insert(*pubkey))
        .unwrap_or(false)
}

pub struct ProfileLoader {
    queue: HashSet<[u8; 32]>,
    fetched: HashSet<[u8; 32]>,
}

impl ProfileLoader {
    pub fn new() -> Self {
        Self {
            queue: HashSet::new(),
            fetched: HashSet::new(),
        }
    }

    pub fn demand(&mut self, pubkey: [u8; 32]) {
        if self.fetched.contains(&pubkey) {
            return;
        }
        self.queue.insert(pubkey);
    }

    pub fn next(&mut self) -> Vec<[u8; 32]> {
        let ret: Vec<[u8; 32]> = self.queue.drain().collect();
        for p in ret.iter() {
            self.fetched.insert(*p);
        }
        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_demand_is_true_once_per_pubkey() {
        let pk = [77u8; 32];
        assert!(first_demand(&pk));
        assert!(!first_demand(&pk));
        assert!(!first_demand(&pk));
        assert!(first_demand(&[78u8; 32]));
    }
}

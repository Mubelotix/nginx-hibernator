use std::collections::BinaryHeap;

/// A struct to represent a pending check for a site.
/// Its `Ord` implementation is to be used by a `BinaryHeap`.
#[derive(PartialEq, Eq)]
pub struct PendingCheck {
    pub site_index: usize,
    pub check_at: u64,
}

pub type CheckQueue = BinaryHeap<PendingCheck>;

impl Ord for PendingCheck {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.check_at.cmp(&other.check_at).reverse()
    }
}

impl PartialOrd for PendingCheck {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

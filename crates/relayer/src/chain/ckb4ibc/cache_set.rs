use std::collections::{HashSet, VecDeque};
use std::hash::Hash;

/// Recording a set of elements and its time to be inserted.
/// When the size of this set reach the maximum, remove the earliest element.
pub struct CacheSet<T>
where
    T: PartialEq + Eq + Hash + Clone,
{
    size: usize,
    times: VecDeque<T>,
    set: HashSet<T>,
}

impl<T> CacheSet<T>
where
    T: PartialEq + Eq + Hash + Clone,
{
    pub fn new(size: usize) -> CacheSet<T> {
        CacheSet {
            size,
            times: VecDeque::with_capacity(size),
            set: HashSet::with_capacity(size),
        }
    }

    pub fn insert(&mut self, element: T) -> bool {
        if self.set.contains(&element) {
            return true;
        }

        if self.size == self.times.len() {
            self.pop();
        }

        self.times.push_back(element.clone());
        self.set.insert(element);
        false
    }

    pub fn has(&self, element: &T) -> bool {
        self.set.contains(element)
    }

    fn pop(&mut self) {
        let element = self.times.pop_front().unwrap();
        self.set.remove(&element);
    }
}

#[cfg(test)]
mod tests {
    use super::CacheSet;

    #[test]
    fn cache_set_test() {
        let mut cache_set = CacheSet::<u32>::new(3);
        cache_set.insert(1);
        cache_set.insert(2);
        cache_set.insert(3);

        assert!(cache_set.has(&1));
        assert!(cache_set.has(&2));
        assert!(cache_set.has(&3));
        assert!(!cache_set.has(&4));

        cache_set.insert(4);

        assert!(!cache_set.has(&1));
        assert!(cache_set.has(&2));
        assert!(cache_set.has(&3));
        assert!(cache_set.has(&4));

        cache_set.insert(5);
        assert!(!cache_set.has(&1));
        assert!(!cache_set.has(&2));
        assert!(cache_set.has(&3));
        assert!(cache_set.has(&4));
        assert!(cache_set.has(&5));
    }
}

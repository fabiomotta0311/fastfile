//! Índice secundário: chave -> identificadores, em ordem de inserção.

use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Index<K: Ord> {
    map: BTreeMap<K, Vec<usize>>,
}

impl<K: Ord> Default for Index<K> {
    fn default() -> Self {
        Index { map: BTreeMap::new() }
    }
}

impl<K: Ord + Clone> Index<K> {
    pub fn insert(&mut self, key: K, id: usize) {
        self.map.entry(key).or_default().push(id);
    }

    /// Remove `id` da chave; apaga a chave quando fica vazia. Devolve se havia algo.
    pub fn remove(&mut self, key: &K, id: usize) -> bool {
        let Some(ids) = self.map.get_mut(key) else {
            return false;
        };
        let before = ids.len();
        ids.retain(|&x| x != id);
        let removed = ids.len() != before;
        if ids.is_empty() {
            self.map.remove(key);
        }
        removed
    }

    pub fn get(&self, key: &K) -> &[usize] {
        self.map.get(key).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.map.keys()
    }

    /// Todos os ids cujas chaves estão no intervalo, em ordem de chave.
    pub fn range<R: std::ops::RangeBounds<K>>(&self, range: R) -> Vec<usize> {
        self.map.range(range).flat_map(|(_, v)| v.iter().copied()).collect()
    }

    pub fn len(&self) -> usize {
        self.map.values().map(|v| v.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove() {
        let mut i: Index<&str> = Index::default();
        i.insert("a", 1);
        i.insert("a", 2);
        i.insert("b", 3);
        assert_eq!(i.get(&"a"), &[1, 2]);
        assert!(i.remove(&"a", 1));
        assert!(!i.remove(&"a", 1));
        assert!(!i.remove(&"z", 1));
        assert_eq!(i.get(&"a"), &[2]);
        assert!(i.remove(&"a", 2));
        assert_eq!(i.keys().count(), 1);
        assert_eq!(i.len(), 1);
    }

    #[test]
    fn range_in_key_order() {
        let mut i: Index<u32> = Index::default();
        i.insert(3, 30);
        i.insert(1, 10);
        i.insert(2, 20);
        assert_eq!(i.range(1..3), vec![10, 20]);
        assert_eq!(i.range(2..), vec![20, 30]);
    }
}

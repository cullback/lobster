//! A small typed generational arena used by indexed book implementations.

use core::fmt;
use core::marker::PhantomData;
use core::mem;

pub(crate) struct Key<Tag> {
    index: u32,
    generation: u32,
    marker: PhantomData<fn() -> Tag>,
}

impl<Tag> Key<Tag> {
    const fn new(index: u32, generation: u32) -> Self {
        Self {
            index,
            generation,
            marker: PhantomData,
        }
    }
}

impl<Tag> Clone for Key<Tag> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Tag> Copy for Key<Tag> {}

impl<Tag> PartialEq for Key<Tag> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}

impl<Tag> Eq for Key<Tag> {}

impl<Tag> fmt::Debug for Key<Tag> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Key")
            .field(&self.index)
            .field(&self.generation)
            .finish()
    }
}

#[derive(Clone, Debug)]
enum Slot<T> {
    Occupied {
        generation: u32,
        value: T,
    },
    Vacant {
        generation: u32,
        next_free: Option<u32>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Arena<T, Tag> {
    slots: Vec<Slot<T>>,
    free_head: Option<u32>,
    len: usize,
    marker: PhantomData<fn() -> Tag>,
}

impl<T, Tag> Arena<T, Tag> {
    pub(crate) const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
            len: 0,
            marker: PhantomData,
        }
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn insert(&mut self, value: T) -> Key<Tag> {
        self.len += 1;
        if let Some(index) = self.free_head {
            let slot = &self.slots[index as usize];
            let Slot::Vacant {
                generation,
                next_free,
            } = slot
            else {
                unreachable!("free-list entry was occupied");
            };
            let generation = *generation;
            self.free_head = *next_free;
            self.slots[index as usize] = Slot::Occupied { generation, value };
            return Key::new(index, generation);
        }

        let index = u32::try_from(self.slots.len()).expect("arena exhausted u32 keys");
        self.slots.push(Slot::Occupied {
            generation: 0,
            value,
        });
        Key::new(index, 0)
    }

    pub(crate) fn get(&self, key: Key<Tag>) -> Option<&T> {
        match self.slots.get(key.index as usize)? {
            Slot::Occupied { generation, value } if *generation == key.generation => Some(value),
            Slot::Occupied { .. } | Slot::Vacant { .. } => None,
        }
    }

    pub(crate) fn get_mut(&mut self, key: Key<Tag>) -> Option<&mut T> {
        match self.slots.get_mut(key.index as usize)? {
            Slot::Occupied { generation, value } if *generation == key.generation => Some(value),
            Slot::Occupied { .. } | Slot::Vacant { .. } => None,
        }
    }

    pub(crate) fn remove(&mut self, key: Key<Tag>) -> Option<T> {
        let slot = self.slots.get(key.index as usize)?;
        let Slot::Occupied { generation, .. } = slot else {
            return None;
        };
        if *generation != key.generation {
            return None;
        }

        let next_generation = key.generation.wrapping_add(1);
        let old = mem::replace(
            &mut self.slots[key.index as usize],
            Slot::Vacant {
                generation: next_generation,
                next_free: self.free_head,
            },
        );
        self.free_head = Some(key.index);
        self.len -= 1;

        let Slot::Occupied { value, .. } = old else {
            unreachable!("validated arena entry became vacant");
        };
        Some(value)
    }

    pub(crate) fn clear(&mut self) {
        self.slots.clear();
        self.free_head = None;
        self.len = 0;
    }
}

impl<T, Tag> Default for Arena<T, Tag> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Arena;

    enum TestTag {}

    #[test]
    fn rejects_a_key_after_its_slot_is_reused() {
        let mut arena = Arena::<_, TestTag>::new();
        let old_key = arena.insert("old");
        assert_eq!(arena.remove(old_key), Some("old"));

        let new_key = arena.insert("new");

        assert_ne!(old_key, new_key);
        assert_eq!(arena.get(old_key), None);
        assert_eq!(arena.get(new_key), Some(&"new"));
    }
}

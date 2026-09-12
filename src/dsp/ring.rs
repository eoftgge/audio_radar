use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct SpscRing {
    slots: UnsafeCell<Box<[f32]>>,
    mask: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl Send for SpscRing {}
unsafe impl Sync for SpscRing {}

impl SpscRing {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.next_power_of_two().max(2);
        Self {
            slots: UnsafeCell::new(vec![0.0; cap].into_boxed_slice()),
            mask: cap - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    pub fn available(&self) -> usize {
        self.head
            .load(Ordering::Acquire)
            .wrapping_sub(self.tail.load(Ordering::Relaxed))
    }

    pub fn push(&self, data: &[f32]) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        let free = self.capacity() - head.wrapping_sub(tail);
        let n = data.len().min(free);

        let slots = unsafe { &mut *self.slots.get() };
        for (i, &v) in data.iter().take(n).enumerate() {
            slots[head.wrapping_add(i) & self.mask] = v;
        }

        self.head.store(head.wrapping_add(n), Ordering::Release);
        n
    }

    pub fn peek(&self, out: &mut [f32]) -> bool {
        if self.available() < out.len() {
            return false;
        }
        let tail = self.tail.load(Ordering::Relaxed);
        let slots = unsafe { &*self.slots.get() };
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = slots[tail.wrapping_add(i) & self.mask];
        }
        true
    }

    pub fn consume(&self, n: usize) {
        let tail = self.tail.load(Ordering::Relaxed);
        let n = n.min(self.available());
        self.tail.store(tail.wrapping_add(n), Ordering::Release);
    }

    pub fn clear(&self) {
        self.tail
            .store(self.head.load(Ordering::Acquire), Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_rounds_up_to_power_of_two() {
        assert_eq!(SpscRing::new(1000).capacity(), 1024);
        assert_eq!(SpscRing::new(1024).capacity(), 1024);
        assert_eq!(SpscRing::new(1).capacity(), 2);
    }

    #[test]
    fn peek_does_not_consume() {
        let ring = SpscRing::new(1024);
        let data: Vec<f32> = (0..300).map(|i| i as f32).collect();
        assert_eq!(ring.push(&data), 300);

        let mut out = vec![0.0; 100];
        assert!(ring.peek(&mut out));
        assert_eq!(out[0], 0.0);
        assert_eq!(out[99], 99.0);
        assert_eq!(ring.available(), 300, "peek не должен сдвигать хвост");

        ring.consume(50);
        assert!(ring.peek(&mut out));
        assert_eq!(out[0], 50.0);
        assert_eq!(ring.available(), 250);
    }

    #[test]
    fn peek_fails_when_short() {
        let ring = SpscRing::new(1024);
        ring.push(&[1.0, 2.0, 3.0]);
        let mut out = vec![0.0; 10];
        assert!(!ring.peek(&mut out));
    }

    #[test]
    fn overflow_drops_excess_instead_of_blocking() {
        let ring = SpscRing::new(64);
        let data = vec![1.0f32; 100];
        assert_eq!(ring.push(&data), 64);
        assert_eq!(ring.push(&data), 0);
        assert_eq!(ring.available(), 64);
    }

    #[test]
    fn survives_wraparound() {
        let ring = SpscRing::new(64);
        for round in 0..10u32 {
            let data: Vec<f32> = (0..40).map(|i| (round * 100 + i) as f32).collect();
            assert_eq!(ring.push(&data), 40);
            let mut out = vec![0.0; 40];
            assert!(ring.peek(&mut out));
            assert_eq!(out[0], (round * 100) as f32);
            assert_eq!(out[39], (round * 100 + 39) as f32);
            ring.consume(40);
        }
    }

    #[test]
    fn clear_drops_everything() {
        let ring = SpscRing::new(64);
        ring.push(&vec![1.0f32; 30]);
        ring.clear();
        assert_eq!(ring.available(), 0);
    }
}

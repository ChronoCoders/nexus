/// Fixed-capacity FIFO ring. Push on a full ring drops the value.
pub(crate) struct Ring<T, const N: usize> {
    items: [Option<T>; N],
    head: usize,
    len: usize,
}

impl<T: Copy, const N: usize> Ring<T, N> {
    pub(crate) const fn new() -> Self {
        Self {
            items: [None; N],
            head: 0,
            len: 0,
        }
    }

    pub(crate) const fn push(&mut self, item: T) {
        debug_assert!(self.len < N, "ring overflow");
        if self.len == N {
            return;
        }
        self.items[(self.head + self.len) % N] = Some(item);
        self.len += 1;
    }

    pub(crate) const fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let item = self.items[self.head].take();
        self.head = (self.head + 1) % N;
        self.len -= 1;
        item
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_order_and_wraparound() {
        let mut r: Ring<u32, 4> = Ring::new();
        assert!(r.is_empty());
        r.push(1);
        r.push(2);
        assert_eq!(r.pop(), Some(1));
        r.push(3);
        r.push(4);
        r.push(5);
        assert_eq!(r.pop(), Some(2));
        assert_eq!(r.pop(), Some(3));
        assert_eq!(r.pop(), Some(4));
        assert_eq!(r.pop(), Some(5));
        assert_eq!(r.pop(), None);
    }
}

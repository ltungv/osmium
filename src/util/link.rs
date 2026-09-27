//! Support for doubly-linked lists.

use core::{
    cell::Cell,
    marker::{PhantomData, PhantomPinned},
    pin::Pin,
    ptr::NonNull,
};

/// An intrusive link to some previous and next elements in a linked list.
pub struct Link<T: ?Sized + AsRef<Self>> {
    prev: Cell<Option<NonNull<Self>>>,
    next: Cell<Option<NonNull<T>>>,
    _pinned: PhantomPinned,
}

impl<T: ?Sized + AsRef<Self>> core::fmt::Debug for Link<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Link")
            .field("prev", &self.prev.get().map(NonNull::as_ptr))
            .field("next", &self.next.get().map(NonNull::as_ptr))
            .finish()
    }
}

impl<T: ?Sized + AsRef<Self>> Default for Link<T> {
    fn default() -> Self {
        Self {
            prev: Cell::new(None),
            next: Cell::new(None),
            _pinned: PhantomPinned,
        }
    }
}

impl<T: ?Sized + AsRef<Self>> Drop for Link<T> {
    fn drop(&mut self) {
        self.remove()
    }
}

impl<T: ?Sized + AsRef<Self>> Link<T> {
    /// Insert a node after this link.
    pub fn insert(self: Pin<&Self>, next: Pin<&T>) {
        let link = next.get_ref().as_ref();
        link.prev.set(Some(NonNull::from(self.get_ref())));
        link.next.set(self.next.get());
        if let Some(next) = self.next.get() {
            let next = unsafe { next.as_ref() };
            next.as_ref().prev.set(Some(NonNull::from(link)));
        }
        self.next.set(Some(NonNull::from(next.get_ref())));
    }

    /// Remove this link from the list.
    pub fn remove(&self) {
        if let Some(prev) = self.prev.get() {
            let prev = unsafe { prev.as_ref() };
            prev.next.set(self.next.get());
        }
        if let Some(next) = self.next.get() {
            let next = unsafe { next.as_ref() };
            next.as_ref().prev.set(self.prev.get());
        }
    }

    pub fn next(&self) -> Option<Pin<&T>> {
        let next_ptr = self.next.get()?;
        let pin = unsafe { Pin::new_unchecked(next_ptr.as_ref()) };
        Some(pin)
    }

    /// Get an iterator over the list starting from the element after this link.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            next: self.next.get(),
            _data: PhantomData,
        }
    }
}

/// An iterator over a list created with [`Link`].
pub struct Iter<'iter, T: ?Sized + AsRef<Link<T>>> {
    next: Option<NonNull<T>>,
    _data: PhantomData<&'iter T>,
}

impl<'iter, T: ?Sized + AsRef<Link<T>>> Iterator for Iter<'iter, T> {
    type Item = Pin<&'iter T>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.next?;
        let next = unsafe { next.as_ref() };
        let link = next.as_ref();
        let pinned = unsafe { Pin::new_unchecked(next) };
        self.next = link.next.get();
        Some(pinned)
    }
}

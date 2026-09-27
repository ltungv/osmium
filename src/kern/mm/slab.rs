//! A slab allocator.

use core::{cell::Cell, marker::PhantomData, pin::Pin, ptr::NonNull};

use crate::{
    kern::mm::{PAGE_SIZE, align_down, align_ptr_down, kalloc::Kmem, paddr::PhysAddr},
    util::link::Link,
};

#[repr(C)]
struct Cache<T> {
    free_slabs: Link<Slab<T>>,
    full_slabs: Link<Slab<T>>,
    live_slabs: Link<Slab<T>>,
}

impl<T> Cache<T> {
    fn new() -> Self {
        Self {
            free_slabs: Link::default(),
            full_slabs: Link::default(),
            live_slabs: Link::default(),
        }
    }

    fn alloc(self: Pin<&Self>, kmem: &mut Kmem) -> Option<NonNull<T>> {
        if let Some(slab) = self.live_slabs.next() {
            let ptr = slab.take().expect("slab should have space");
            if slab.is_full() {
                slab.link.remove();
                let full_slabs = unsafe { Pin::new_unchecked(&self.full_slabs) };
                full_slabs.insert(slab);
            }
            return Some(ptr);
        }
        let slab = if let Some(slab) = self.free_slabs.next() {
            slab.link.remove();
            slab
        } else {
            let ptr = Slab::alloc(NonNull::from(self.get_ref()), kmem)?;
            unsafe { Pin::new_unchecked(ptr.as_ref()) }
        };
        let ptr = slab.take().expect("slab should have space");
        let live_slabs = unsafe { Pin::new_unchecked(&self.live_slabs) };
        live_slabs.insert(slab);
        Some(ptr)
    }

    fn free(self: Pin<&Self>, ptr: NonNull<T>) {
        let page_ptr = align_ptr_down(ptr.as_ptr().cast(), PAGE_SIZE);
        let slab_ptr = {
            let ptr = unsafe { page_ptr.add(Slab::<T>::PAGE_OFFSET) };
            let ptr = unsafe { NonNull::new_unchecked(ptr) };
            ptr.cast::<Slab<T>>()
        };
        let slab = unsafe { Pin::new_unchecked(slab_ptr.as_ref()) };
        let slab_was_full = slab.is_full();
        slab.give(ptr);
        if slab_was_full {
            slab.link.remove();
            let live_slabs = unsafe { Pin::new_unchecked(&self.live_slabs) };
            live_slabs.insert(slab);
        }
        if slab.is_free() {
            slab.link.remove();
            let free_slabs = unsafe { Pin::new_unchecked(&self.free_slabs) };
            free_slabs.insert(slab);
        }
    }

    fn gc(self: Pin<&Self>, kmem: &mut Kmem) {
        while let Some(slab) = self.free_slabs.next() {
            Slab::<T>::free(NonNull::from(slab.get_ref()), kmem);
        }
    }
}

#[repr(C)]
struct Slab<T> {
    cache: NonNull<Cache<T>>,
    link: Link<Slab<T>>,
    slot: Slot<T>,
    refs: Cell<usize>,
}

impl<T> AsRef<Link<Self>> for Slab<T> {
    fn as_ref(&self) -> &Link<Self> {
        &self.link
    }
}

impl<T> Slab<T> {
    const PAGE_OFFSET: usize = {
        let offset = PAGE_SIZE - size_of::<Self>();
        align_down(offset, align_of::<Self>())
    };

    const NUM_SLOTS: usize = {
        let available = align_down(Self::PAGE_OFFSET, Slot::<T>::ALIGN);
        available / Slot::<T>::SIZE
    };

    fn alloc(cache: NonNull<Cache<T>>, kmem: &mut Kmem) -> Option<NonNull<Self>> {
        let ppn = kmem.alloc(1)?;
        let paddr = ppn.addr();
        let vaddr = unsafe { paddr.direct() };
        let page_ptr = vaddr.as_ptr_mut::<u8>();
        let mut slot = None;
        for i in (0..Self::NUM_SLOTS).rev() {
            let slot_ptr = {
                let ptr = unsafe { page_ptr.add(i * Slot::<T>::SIZE) };
                let ptr = unsafe { NonNull::new_unchecked(ptr) };
                ptr.cast::<Slot<T>>()
            };
            unsafe {
                slot_ptr.write(Slot::new(slot));
            }
            slot = Some(slot_ptr);
        }
        let slab_ptr = {
            let ptr = unsafe { page_ptr.add(Self::PAGE_OFFSET) };
            let ptr = unsafe { NonNull::new_unchecked(ptr) };
            ptr.cast::<Self>()
        };
        unsafe {
            slab_ptr.write(Self {
                cache,
                link: Link::default(),
                slot: Slot::new(slot),
                refs: Cell::new(0),
            });
        }
        Some(slab_ptr)
    }

    fn free(slab_ptr: NonNull<Self>, kmem: &mut Kmem) {
        let slab = unsafe { slab_ptr.as_ref() };
        assert!(slab.refs.get() == 0);
        let page_ptr = {
            let ptr = slab_ptr.cast::<u8>();
            unsafe { ptr.sub(Self::PAGE_OFFSET) }
        };
        for i in (0..Self::NUM_SLOTS).rev() {
            let slot_ptr = {
                let ptr = unsafe { page_ptr.add(i * Slot::<T>::SIZE) };
                ptr.cast::<Slot<T>>()
            };
            unsafe {
                slot_ptr.drop_in_place();
            }
        }
        unsafe {
            slab_ptr.drop_in_place();
        }
        let paddr = PhysAddr::new(page_ptr.addr().get());
        let ppn = paddr.page_number();
        kmem.dealloc(ppn);
    }

    fn take(&self) -> Option<NonNull<T>> {
        let ptr = self.slot.take()?;
        self.refs.set(self.refs.get() + 1);
        Some(ptr)
    }

    fn give(&self, ptr: NonNull<T>) {
        self.refs.set(self.refs.get() - 1);
        self.slot.give(ptr);
    }

    fn is_free(&self) -> bool {
        self.refs.get() == 0
    }

    fn is_full(&self) -> bool {
        self.refs.get() == Self::NUM_SLOTS
    }
}

struct Slot<T> {
    next: Cell<Option<NonNull<Slot<T>>>>,
    _data: PhantomData<T>,
}

impl<T> Slot<T> {
    const SIZE: usize = {
        let slot_size = size_of::<Self>();
        let item_size = size_of::<T>();
        if slot_size > item_size {
            slot_size
        } else {
            item_size
        }
    };

    const ALIGN: usize = {
        let slot_align = align_of::<Self>();
        let item_align = align_of::<T>();
        if slot_align > item_align {
            slot_align
        } else {
            item_align
        }
    };

    const fn new(next: Option<NonNull<Slot<T>>>) -> Self {
        Self {
            next: Cell::new(next),
            _data: PhantomData,
        }
    }

    fn take(&self) -> Option<NonNull<T>> {
        let next = self.next.get()?;
        {
            let slot = unsafe { next.read() };
            self.next.set(slot.next.replace(None));
        }
        Some(next.cast())
    }

    fn give(&self, ptr: NonNull<T>) {
        unsafe {
            ptr.drop_in_place();
        }
        let ptr = ptr.cast::<Self>();
        unsafe {
            ptr.write(Slot::new(self.next.get()));
        }
        self.next.set(Some(ptr));
    }
}

#[cfg(test)]
mod tests {
    use core::{pin::pin, ptr::NonNull};

    use crate::{
        kern::mm::{
            kalloc,
            slab::{Cache, Slab},
        },
        println,
    };

    #[test_case]
    fn smoke() {
        struct Dummy {
            _a: bool,
            _b: u8,
            _c: u16,
            _d: u32,
            _e: u64,
        }

        println!();
        println!("{:?}", kalloc::kmem().lock());

        let cache = pin!(Cache::<Dummy>::new());
        let mut ptrs: [Option<NonNull<Dummy>>; Slab::<Dummy>::NUM_SLOTS * 3] =
            [None; Slab::<Dummy>::NUM_SLOTS * 3];

        for ptr in &mut ptrs {
            *ptr = cache.as_ref().alloc(&mut kalloc::kmem().lock());
        }
        println!("{:?}", kalloc::kmem().lock());

        for ptr in &mut ptrs {
            if let Some(ptr) = ptr.take() {
                cache.as_ref().free(ptr);
            }
        }
        println!("{:?}", kalloc::kmem().lock());

        cache.as_ref().gc(&mut kalloc::kmem().lock());
        println!("{:?}", kalloc::kmem().lock());
    }
}

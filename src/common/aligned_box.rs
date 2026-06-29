use core::alloc::Layout;

// Necessary because GlobalAlloc::dealloc requires the layout to be the same, and therefore Box
// cannot be used for increased alignment directly.
pub struct AlignedBox<T: ?Sized, const ALIGN: usize> {
    inner: *mut T,
}
unsafe impl<T: Send + ?Sized, const ALIGN: usize> Send for AlignedBox<T, ALIGN> {}
unsafe impl<T: Sync + ?Sized, const ALIGN: usize> Sync for AlignedBox<T, ALIGN> {}

/// # Safety
/// All types implementing this trait must be valid when zeroed
pub unsafe trait ValidForZero {}
unsafe impl<const N: usize> ValidForZero for [u8; N] {}
unsafe impl ValidForZero for u8 {}

impl<T: ?Sized, const ALIGN: usize> AlignedBox<T, ALIGN> {
    fn layout(&self) -> Layout {
        layout_upgrade_align(Layout::for_value::<T>(self), ALIGN)
    }
}
const fn layout_upgrade_align(layout: Layout, align: usize) -> Layout {
    const fn max(a: usize, b: usize) -> usize {
        if a > b {
            a
        } else {
            b
        }
    }
    let Ok(x) = Layout::from_size_align(layout.size(), max(align, layout.align())) else {
        panic!("failed to calculate layout");
    };
    x
}

impl<T, const ALIGN: usize> AlignedBox<T, ALIGN> {
    #[inline(always)]
    pub fn try_zeroed() -> Result<Self, ()>
    where
        T: ValidForZero,
    {
        Ok(unsafe {
            let layout = layout_upgrade_align(Layout::new::<T>(), ALIGN);
            let ptr = alloc::alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                return Err(());
            }
            Self { inner: ptr.cast() }
        })
    }
}
impl<T, const ALIGN: usize> AlignedBox<[T], ALIGN> {
    #[inline]
    pub fn try_zeroed_slice(len: usize) -> Result<Self, ()>
    where
        T: ValidForZero,
    {
        Ok(unsafe {
            let layout = layout_upgrade_align(Layout::array::<T>(len).unwrap(), ALIGN);
            let ptr = alloc::alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                return Err(());
            }
            Self {
                inner: core::ptr::slice_from_raw_parts_mut(ptr.cast(), len),
            }
        })
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.as_ref().len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.as_ref().is_empty()
    }
}

impl<T: ?Sized, const ALIGN: usize> AlignedBox<T, ALIGN> {
    #[inline]
    pub fn as_ref(&self) -> &T {
        unsafe { &*self.inner }
    }
    #[inline]
    pub fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *self.inner }
    }
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.inner
    }
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.inner
    }
    #[inline]
    pub fn leak<'a>(self) -> &'a mut T {
        let ptr = self.inner;
        core::mem::forget(self);
        unsafe { &mut *ptr }
    }
}

impl<T: ?Sized, const ALIGN: usize> core::ops::Deref for AlignedBox<T, ALIGN> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.as_ref()
    }
}
impl<T: ?Sized, const ALIGN: usize> core::ops::DerefMut for AlignedBox<T, ALIGN> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.as_mut()
    }
}

impl<T: ?Sized, const ALIGN: usize> Drop for AlignedBox<T, ALIGN> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            alloc::alloc::dealloc(self.inner.cast::<u8>(), self.layout());
        }
    }
}

impl<T: core::fmt::Debug + ?Sized, const ALIGN: usize> core::fmt::Debug for AlignedBox<T, ALIGN> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.as_ref().fmt(f)
    }
}

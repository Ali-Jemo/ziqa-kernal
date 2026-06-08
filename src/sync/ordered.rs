//! Compile-time deadlock prevention via ordered lock levels.
//! Ported and simplified from Redox OS.

use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use spin::{Mutex as SpinMutex, MutexGuard as SpinMutexGuard};
use spin::{RwLock as SpinRwLock, RwLockReadGuard as SpinRwLockReadGuard, RwLockWriteGuard as SpinRwLockWriteGuard};

pub trait Level {}
pub trait Lower<O: Level>: Level {}

#[derive(Debug)] pub struct L0 {}
#[derive(Debug)] pub struct L1 {}
#[derive(Debug)] pub struct L2 {}
#[derive(Debug)] pub struct L3 {}
#[derive(Debug)] pub struct L4 {}

impl Level for L0 {}
impl Level for L1 {}
impl Level for L2 {}
impl Level for L3 {}
impl Level for L4 {}

impl Lower<L1> for L0 {}
impl Lower<L2> for L0 {}
impl Lower<L3> for L0 {}
impl Lower<L4> for L0 {}
impl Lower<L2> for L1 {}
impl Lower<L3> for L1 {}
impl Lower<L4> for L1 {}
impl Lower<L3> for L2 {}
impl Lower<L4> for L2 {}
impl Lower<L4> for L3 {}

pub trait Higher<O: Level>: Level {}
impl<L1: Level, L2: Level> Higher<L2> for L1 where L2: Lower<L1> {}

pub struct LockToken<'a, L: Level>(PhantomData<&'a mut L>);

impl<'a, L: Level> LockToken<'a, L> {
    pub fn token(&mut self) -> LockToken<'_, L> {
        LockToken(Default::default())
    }
    pub fn downgrade<LC: Higher<L>>(&mut self) -> LockToken<'_, LC> {
        LockToken(Default::default())
    }
}

pub struct CleanLockToken(());

impl CleanLockToken {
    pub fn token(&mut self) -> LockToken<'_, L0> {
        LockToken(Default::default())
    }
    pub unsafe fn new() -> Self {
        CleanLockToken(())
    }
}

// ── Mutex ───────────────────────────────────────────────────────────────────

pub struct Mutex<L: Level, T> {
    inner: SpinMutex<T>,
    _phantom: PhantomData<L>,
}

impl<L: Level, T> Mutex<L, T> {
    pub const fn new(value: T) -> Self {
        Self {
            inner: SpinMutex::new(value),
            _phantom: PhantomData,
        }
    }

    pub fn lock<'a>(&'a self, _token: LockToken<'a, L0>) -> MutexGuard<'a, L, T> {
        MutexGuard {
            inner: self.inner.lock(),
            _phantom: PhantomData,
        }
    }
}

pub struct MutexGuard<'a, L: Level, T> {
    inner: SpinMutexGuard<'a, T>,
    _phantom: PhantomData<L>,
}

impl<'a, L: Level, T> Deref for MutexGuard<'a, L, T> {
    type Target = T;
    fn deref(&self) -> &T { &*self.inner }
}

impl<'a, L: Level, T> DerefMut for MutexGuard<'a, L, T> {
    fn deref_mut(&mut self) -> &mut T { &mut *self.inner }
}

// ── RwLock ──────────────────────────────────────────────────────────────────

pub struct RwLock<L: Level, T> {
    inner: SpinRwLock<T>,
    _phantom: PhantomData<L>,
}

impl<L: Level, T> RwLock<L, T> {
    pub const fn new(value: T) -> Self {
        Self {
            inner: SpinRwLock::new(value),
            _phantom: PhantomData,
        }
    }

    pub fn read<'a>(&'a self, _token: LockToken<'a, L0>) -> RwLockReadGuard<'a, L, T> {
        RwLockReadGuard {
            inner: self.inner.read(),
            _phantom: PhantomData,
        }
    }

    pub fn write<'a>(&'a self, _token: LockToken<'a, L0>) -> RwLockWriteGuard<'a, L, T> {
        RwLockWriteGuard {
            inner: self.inner.write(),
            _phantom: PhantomData,
        }
    }
}

pub struct RwLockReadGuard<'a, L: Level, T> {
    inner: SpinRwLockReadGuard<'a, T>,
    _phantom: PhantomData<L>,
}

pub struct RwLockWriteGuard<'a, L: Level, T> {
    inner: SpinRwLockWriteGuard<'a, T>,
    _phantom: PhantomData<L>,
}

impl<'a, L: Level, T> Deref for RwLockReadGuard<'a, L, T> {
    type Target = T;
    fn deref(&self) -> &T { &*self.inner }
}

impl<'a, L: Level, T> Deref for RwLockWriteGuard<'a, L, T> {
    type Target = T;
    fn deref(&self) -> &T { &*self.inner }
}

impl<'a, L: Level, T> DerefMut for RwLockWriteGuard<'a, L, T> {
    fn deref_mut(&mut self) -> &mut T { &mut *self.inner }
}

use std::{
    marker::PhantomData,
    sync::{
        atomic::{AtomicU16, AtomicUsize, Ordering},
        Arc,
    },
};

/// On instantiation `Registration` adds 1 and on destruction 1 is substracted
pub(crate) struct Registration<T, R>
where
    T: Atomic<R>,
    R: From<u8>,
{
    nb: T,
    _phantom: PhantomData<R>,
}

impl<T, R> Registration<T, R>
where
    T: Atomic<R>,
    R: From<u8>,
{
    pub(crate) fn new(nb: T) -> Self {
        let _ = nb.fetch_add(R::from(1), Ordering::Release);
        Self {
            nb,
            _phantom: PhantomData,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn value(&self) -> R {
        self.nb.load(Ordering::Relaxed)
    }
}

impl<T, R> Drop for Registration<T, R>
where
    T: Atomic<R>,
    R: From<u8>,
{
    fn drop(&mut self) {
        let _ = self.nb.fetch_sub(R::from(1), Ordering::Release);
    }
}

pub(crate) trait Atomic<T> {
    fn load(&self, order: Ordering) -> T;
    fn fetch_add(&self, val: T, order: Ordering) -> T;
    fn fetch_sub(&self, val: T, order: Ordering) -> T;
}

macro_rules! impl_registration_ref {
    ($t:ty, $r:ty) => {
        impl Atomic<$r> for &$t {
            #[inline]
            fn load(&self, order: Ordering) -> $r {
                (*self).load(order)
            }

            #[inline]
            fn fetch_add(&self, val: $r, order: Ordering) -> $r {
                (*self).fetch_add(val, order)
            }

            #[inline]
            fn fetch_sub(&self, val: $r, order: Ordering) -> $r {
                (*self).fetch_sub(val, order)
            }
        }
    };
}

macro_rules! impl_registration_as_ref {
    ($t:ty, $r:ty) => {
        impl Atomic<$r> for $t {
            #[inline]
            fn load(&self, order: Ordering) -> $r {
                self.as_ref().load(order)
            }

            #[inline]
            fn fetch_add(&self, val: $r, order: Ordering) -> $r {
                self.as_ref().fetch_add(val, order)
            }

            #[inline]
            fn fetch_sub(&self, val: $r, order: Ordering) -> $r {
                self.as_ref().fetch_sub(val, order)
            }
        }
    };
}

pub(crate) type ArcRegistrationU16 = Registration<Arc<AtomicU16>, u16>;
impl_registration_as_ref!(Arc<AtomicU16>, u16);

// pub(crate) type RegistrationUsize<'a> = Registration<&'a AtomicUsize, usize>;
impl_registration_ref!(AtomicUsize, usize);

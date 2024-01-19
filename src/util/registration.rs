use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/// On instantiation `Registration` adds 1 and on destruction 1 is substracted
pub(crate) struct Registration<'a> {
    nb: &'a AtomicUsize,
}

impl<'a> Registration<'a> {
    pub(crate) fn new(nb: &'a AtomicUsize) -> Self {
        let _ = nb.fetch_add(1, Ordering::Release);
        Self { nb }
    }
}

impl Drop for Registration<'_> {
    fn drop(&mut self) {
        let _ = self.nb.fetch_sub(1, Ordering::Release);
    }
}

/// On instantiation `ArcRegistration` adds 1 and on destruction 1 is substracted
pub(crate) struct ArcRegistration {
    nb: Arc<AtomicUsize>,
}

impl ArcRegistration {
    pub(crate) fn new(nb: Arc<AtomicUsize>) -> Self {
        let _ = nb.fetch_add(1, Ordering::Release);
        Self { nb }
    }
}

impl Drop for ArcRegistration {
    fn drop(&mut self) {
        let _ = self.nb.fetch_sub(1, Ordering::Release);
    }
}

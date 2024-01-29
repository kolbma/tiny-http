#[cfg(feature = "log")]
pub(crate) use log::{debug, error, info, warn};

#[cfg(not(feature = "log"))]
macro_rules! log_mock {
    (target: $target:expr, $($arg:tt)+) => {};
    ($($arg:tt)+) => {};
}

#[cfg(not(feature = "log"))]
pub(crate) use {log_mock as debug, log_mock as error, log_mock as info, log_mock as warn};

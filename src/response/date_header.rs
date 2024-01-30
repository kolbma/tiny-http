use std::{
    sync::{Once, OnceLock, RwLock},
    thread,
    time::{Duration, Instant, SystemTime},
};

use ascii::AsciiString;
use httpdate::HttpDate;

use crate::Header;

/// Fixed length date and time in bytes e.g. Mon, 29 Jan 2024 22:13:01 GMT
const DATE_TIME_SAMPLE: &[u8; 29] = b"Mon, 29 Jan 2024 22:13:01 GMT";
/// Stores updated date and time
static DATE_TIME: RwLock<AsciiString> = RwLock::new(AsciiString::new());
/// Header template for cloning
static DATE_TIME_HEADER: RwLock<Option<Header>> = RwLock::new(None);
static DATE_TIME_HEADER_ONCE: Once = Once::new();
static DATE_TIME_THREAD: OnceLock<thread::JoinHandle<()>> = OnceLock::new();

/// `DateHeader` caching for performance
pub(super) struct DateHeader {
    header: Header,
}

impl DateHeader {
    ///
    #[inline]
    pub(super) fn current() -> Header {
        let mut header = Self::new().header;
        {
            let date_time = DATE_TIME.read().unwrap();
            header.value = (*date_time).clone();
        }
        header
    }

    fn new() -> Self {
        DATE_TIME_HEADER_ONCE.call_once(|| {
            let _ = *DATE_TIME_HEADER
                .write()
                .unwrap()
                .insert(Header::from_bytes(b"Date", DATE_TIME_SAMPLE).unwrap());

            *DATE_TIME.write().unwrap() = AsciiString::from_ascii(DATE_TIME_SAMPLE).unwrap();

            let _ = DATE_TIME_THREAD.get_or_init(|| thread::spawn(Self::timer_thread));
        });

        Self {
            header: DATE_TIME_HEADER.read().unwrap().as_ref().unwrap().clone(),
        }
    }

    fn timer_thread() {
        #[inline]
        fn inner_loop() {
            let http_date = HttpDate::from(SystemTime::now()).to_string();
            let http_date = http_date.as_bytes();
            {
                let date_time = DATE_TIME.write();
                let mut date_time = date_time.unwrap();
                debug_assert!(http_date.len() == date_time.len());
                *date_time = AsciiString::from_ascii(http_date).unwrap();
            }
        }

        let now = Instant::now();
        inner_loop();
        let sleep_duration = Duration::from_millis(1000) - now.elapsed();

        loop {
            inner_loop();
            thread::sleep(sleep_duration);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_header_test() {
        let mut dh_1 = DateHeader::current();
        assert_eq!(dh_1.field.as_str(), "Date");
        thread::sleep(Duration::from_millis(1000));
        let mut dh_2 = DateHeader::current();
        assert_ne!(dh_1.to_string(), dh_2.to_string());

        dh_2 = DateHeader::current();
        while dh_1.value != dh_2.value {
            dh_1 = DateHeader::current();
        }
        thread::sleep(Duration::from_millis(750));
        assert_eq!(dh_1.value, dh_2.value);
    }
}

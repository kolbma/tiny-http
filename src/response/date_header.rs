use std::{
    sync::{Once, RwLock},
    thread,
    time::{Duration, Instant, SystemTime},
};

use ascii::AsciiString;
use httpdate::HttpDate;
use lazy_static::lazy_static;

use crate::common::{Header, HeaderFieldValue};

/// Fixed length date and time in bytes e.g. Mon, 29 Jan 2024 22:13:01 GMT
const DATE_TIME_SAMPLE: &[u8; 29] = b"Mon, 29 Jan 2024 22:13:01 GMT";

// TODO: Works with Rust >= 1.63 (replaces lazy_static below)
// /// Stores updated date and time
// static DATE_TIME: RwLock<AsciiString> = RwLock::new(AsciiString::new());
// /// Used for storing/swap updated date and time
// static DATE_TIME_WORK: RwLock<AsciiString> = RwLock::new(AsciiString::new());
// /// Header template for cloning
// static DATE_TIME_HEADER: RwLock<Option<Header>> = RwLock::new(None);

lazy_static! {
    static ref DATE_HEADER_SINGLETON: DateHeader = DateHeader::new();
    /// Stores updated date and time
    static ref DATE_TIME: RwLock<AsciiString> = RwLock::new(AsciiString::new());
    /// Used for storing/swap updated date and time
    static ref DATE_TIME_WORK: RwLock<AsciiString> = RwLock::new(AsciiString::new());
    /// Header template for cloning
    static ref DATE_TIME_HEADER: RwLock<Option<Header>> = RwLock::new(None);
}

static DATE_TIME_HEADER_ONCE: Once = Once::new();

/// `DateHeader` caching for performance
pub(super) struct DateHeader {
    header: Header,
}

impl DateHeader {
    /// Header for _Date_ with current Http Date and Time
    #[inline]
    pub(super) fn current() -> Header {
        let mut header = DATE_HEADER_SINGLETON.header.clone();
        let date_time_clone;

        {
            let date_time = DATE_TIME.read().unwrap();
            date_time_clone = date_time.clone();
        }

        header.value = HeaderFieldValue::from_ascii_unchecked(date_time_clone);
        header
    }

    fn new() -> Self {
        DATE_TIME_HEADER_ONCE.call_once(|| {
            let mut header = DATE_TIME_HEADER.write().unwrap();
            let _ = header.insert(Header::from_bytes(b"Date", DATE_TIME_SAMPLE).unwrap());

            let mut date_time = DATE_TIME_WORK.write().unwrap();
            *date_time = AsciiString::from_ascii(&DATE_TIME_SAMPLE[..]).unwrap();

            let mut date_time = DATE_TIME.write().unwrap();
            *date_time = AsciiString::from_ascii(&DATE_TIME_SAMPLE[..]).unwrap();

            let _ = thread::spawn(Self::timer_thread);
        });

        let header = DATE_TIME_HEADER.read().unwrap();
        let header = header.as_ref().unwrap().clone();

        Self { header }
    }

    fn timer_thread() {
        #[inline]
        fn task() {
            let http_date = HttpDate::from(SystemTime::now()).to_string();
            let http_date = http_date.as_bytes();

            let mut date_time_work = DATE_TIME_WORK.write().unwrap();
            debug_assert!(http_date.len() == date_time_work.len());
            *date_time_work = AsciiString::from_ascii(http_date).unwrap();

            let mut date_time = DATE_TIME.write().unwrap();
            std::mem::swap(&mut *date_time, &mut *date_time_work);
        }

        let now = Instant::now();
        task();
        let sleep_duration = Duration::from_millis(1000) - now.elapsed();

        loop {
            task();
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

        dh_1 = DateHeader::current();
        dh_2 = DateHeader::current();
        while dh_1.value == dh_2.value {
            dh_2 = DateHeader::current();
            thread::sleep(Duration::from_millis(10));
        }

        thread::sleep(Duration::from_millis(750));
        dh_1 = DateHeader::current();

        assert_eq!(dh_1.value, dh_2.value);
    }
}

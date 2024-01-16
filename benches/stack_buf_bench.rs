#![feature(test)]

use std::io::{Cursor, Read};

extern crate test;

const READERS: &[&[u8]] = &[
    &[1u8; 0],
    &[2u8; 1],
    &[3u8; 8],
    &[4u8; 12],
    &[5u8; 80],
    &[6u8; 200],
    &[7u8; 1024],
    &[8u8; 4096],
    &[9u8; 8192],
    &[10u8; 25_000_000],
];

const COUNTS: [usize; 10] = [10000, 100, 100, 50, 50, 10, 10, 10, 5, 1];

const SIZES: [usize; 34] = [
    0_usize, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 79, 80, 81, 199, 200, 201, 1023,
    1024, 1025, 4095, 4096, 4097, 8191, 8192, 8193, 24_999_999, 25_000_000, 25_000_001,
];

fn stack_buf_bench<const N: usize>(bencher: &mut test::Bencher) {
    let readers = std::hint::black_box(READERS);

    bencher.iter(|| {
        for (idx, reader) in readers.iter().enumerate() {
            for _ in 0..COUNTS[idx] {
                let mut reader = Cursor::new(*reader);

                for size in SIZES {
                    let mut size = size;
                    if size == 0 {
                        continue;
                    }

                    let mut buf = &mut [0u8; N][..];
                    if size < N {
                        buf = &mut buf[..size];
                    }

                    while size > 0 {
                        match reader.read(buf) {
                            Ok(0) => {
                                break;
                            }
                            Ok(nr_bytes) => size -= nr_bytes,
                            Err(err) => {
                                unreachable!("{}", err);
                            }
                        }

                        if size < N {
                            buf = &mut buf[..size];
                        }
                    }

                    std::hint::black_box(buf);
                }
            }
        }
    });
}

fn vec_bench<const N: usize>(bencher: &mut test::Bencher) {
    let readers = std::hint::black_box(READERS);

    bencher.iter(|| {
        for (idx, reader) in readers.iter().enumerate() {
            for _ in 0..COUNTS[idx] {
                let mut reader = Cursor::new(*reader);

                for size in SIZES {
                    let mut size = size;
                    if size == 0 {
                        continue;
                    }

                    let mut buf = vec![0u8; if size > N { N } else { size }];

                    while size > 0 {
                        match reader.read(&mut buf) {
                            Ok(0) => {
                                break;
                            }
                            Ok(nr_bytes) => size -= nr_bytes,
                            Err(err) => {
                                unreachable!("{}", err);
                            }
                        }

                        buf.truncate(size);
                    }
                    std::hint::black_box(buf);
                }
            }
        }
    });
}

fn vec_stack_buf_bench<const N: usize>(bencher: &mut test::Bencher) {
    let readers = std::hint::black_box(READERS);

    bencher.iter(|| {
        for (idx, reader) in readers.iter().enumerate() {
            for _ in 0..COUNTS[idx] {
                let mut reader = Cursor::new(*reader);

                for size in SIZES {
                    let mut size = size;
                    if size == 0 {
                        continue;
                    }

                    if size <= N {
                        let mut buf = &mut [0u8; N][..];
                        while size > 0 {
                            match reader.read(buf) {
                                Ok(0) => {
                                    break;
                                }
                                Ok(nr_bytes) => size -= nr_bytes,
                                Err(err) => {
                                    unreachable!("{}", err);
                                }
                            }

                            if size < N {
                                buf = &mut buf[..size];
                            }
                        }
                        std::hint::black_box(buf);
                    } else {
                        let buf = &mut vec![0u8; N];
                        while size > 0 {
                            match reader.read(buf) {
                                Ok(0) => {
                                    break;
                                }
                                Ok(nr_bytes) => size -= nr_bytes,
                                Err(err) => {
                                    unreachable!("{}", err);
                                }
                            }

                            buf.truncate(size);
                        }
                        std::hint::black_box(buf);
                    };
                }
            }
        }
    });
}

macro_rules! create_benches {
    ( $($fstack:ident, $fvec:ident, $fvecstack:ident, $s:expr),+ ) => {
        $(
            // #[ignore]
            #[bench]
            fn $fstack(bencher: &mut test::Bencher) {
                stack_buf_bench::<$s>(bencher);
            }

            // #[ignore]
            #[bench]
            fn $fvec(bencher: &mut test::Bencher) {
                vec_bench::<$s>(bencher);
            }

            // #[ignore]
            #[bench]
            fn $fvecstack(bencher: &mut test::Bencher) {
                vec_stack_buf_bench::<$s>(bencher);
            }
        )+
    };
    ( $([$f1:ident, $f2:ident, $f3:ident, $s:expr]),+ ) => {
        $(
            create_benches!($f1, $f2, $f3, $s);
        )+
    };
}

create_benches!(
    [stack_buf_128, vec_128, vec_stack_buf_128, 128_usize],
    [stack_buf_192, vec_192, vec_stack_buf_192, 192_usize],
    [stack_buf_256, vec_256, vec_stack_buf_256, 256_usize],
    [stack_buf_384, vec_384, vec_stack_buf_384, 384_usize],
    [stack_buf_512, vec_512, vec_stack_buf_512, 512_usize]
);

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use super::Registration;

/// Manages a collection of threads.
///
/// A new thread is created every time all the existing threads are full.
/// Any idle thread will automatically die after a few seconds.
pub(crate) struct TaskPool {
    sharing: Arc<Sharing>,
}

pub(crate) type TaskFn = Box<dyn FnMut() + Send>;

struct Sharing {
    // list of the queued tasks to be done by worker threads
    queue: Mutex<VecDeque<TaskFn>>,

    // condvar that will be notified whenever a task is added to `tasks`
    condvar: Condvar,

    // flag to decide to run or exit
    run: AtomicBool,

    // number of idle worker threads
    threads_idle: AtomicUsize,

    // number of total worker threads running
    threads_total: AtomicUsize,
}

/// Minimum number of active threads.
pub(crate) const MIN_THREADS: usize = 4;

/// Minimum number of idle threads.
const MIN_IDLE_THREADS: usize = 1;

/// Time threads stay alive without working task
const IDLE_TIME: Duration = Duration::from_millis(5000);

impl TaskPool {
    pub(crate) fn new() -> TaskPool {
        let pool = TaskPool {
            sharing: Arc::new(Sharing {
                queue: Mutex::new(VecDeque::new()),
                condvar: Condvar::new(),
                run: AtomicBool::from(true),
                threads_total: AtomicUsize::default(),
                threads_idle: AtomicUsize::default(),
            }),
        };

        for _ in 0..MIN_THREADS {
            pool.add_thread(None);
        }

        pool
    }

    /// Executes a function in a thread.
    ///
    /// If no thread is available, spawns a new one.
    pub(crate) fn spawn_task(&self, code: TaskFn) {
        let mut queue = self.sharing.queue.lock().unwrap();

        if self.sharing.threads_idle.load(Ordering::Acquire) == 0
            || queue.len() > self.sharing.threads_total.load(Ordering::Acquire)
        {
            self.add_thread(Some(code));
        } else {
            queue.push_back(code);
            self.sharing.condvar.notify_one();
        }
    }

    #[inline]
    fn add_thread(&self, initial_fn: Option<TaskFn>) {
        let sharing = Arc::clone(&self.sharing);

        let _ = thread::spawn(move || {
            let sharing = sharing;
            let _active_guard = Registration::new(&sharing.threads_total);

            if let Some(mut f) = initial_fn {
                f();
            }

            while sharing.run.load(Ordering::Acquire) {
                let mut task: Box<dyn FnMut() + Send> = {
                    let mut queue = sharing.queue.lock().unwrap();

                    let task;
                    loop {
                        if let Some(new_task) = queue.pop_front() {
                            task = new_task;
                            break;
                        }
                        let _waiting_guard = Registration::new(&sharing.threads_idle);

                        let received =
                            if sharing.threads_total.load(Ordering::Acquire) <= MIN_THREADS {
                                queue = sharing.condvar.wait(queue).unwrap();
                                true
                            } else {
                                let (new_lock, wait_res) =
                                    sharing.condvar.wait_timeout(queue, IDLE_TIME).unwrap();
                                queue = new_lock;
                                !wait_res.timed_out()
                            };

                        if !received {
                            if !sharing.run.load(Ordering::Acquire) {
                                return;
                            } else if sharing.threads_idle.load(Ordering::Acquire)
                                <= MIN_IDLE_THREADS
                                || sharing.threads_total.load(Ordering::Acquire) <= MIN_THREADS
                            {
                                continue;
                            } else if queue.is_empty() {
                                return;
                            }
                        }
                    }

                    task
                };

                task();
            }
        });
    }

    /// Number of total threads in pool
    #[inline]
    pub(crate) fn threads_total(&self) -> usize {
        self.sharing.threads_total.load(Ordering::Relaxed)
    }
}

impl Drop for TaskPool {
    fn drop(&mut self) {
        // Make sure spawned threads run to return or last task and doesn't continue
        self.sharing.run.store(false, Ordering::Release);
        self.sharing.condvar.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::Ordering,
        thread,
        time::{Duration, Instant},
    };

    use super::{TaskPool, IDLE_TIME, MIN_THREADS};

    #[test]
    fn task_pool_constructor_test() {
        let tp = TaskPool::new();
        assert!(tp.sharing.threads_total.load(Ordering::Relaxed) <= MIN_THREADS);
        assert!(tp.sharing.threads_idle.load(Ordering::Relaxed) <= MIN_THREADS);

        thread::sleep(Duration::from_millis(100));

        assert_eq!(
            tp.sharing.threads_total.load(Ordering::Relaxed),
            MIN_THREADS
        );
        assert_eq!(tp.sharing.threads_idle.load(Ordering::Relaxed), MIN_THREADS);
    }

    #[test]
    fn task_pool_min_threads_test() {
        let tp = TaskPool::new();

        thread::sleep(Duration::from_millis(5100));

        assert_eq!(
            tp.sharing.threads_total.load(Ordering::Relaxed),
            MIN_THREADS
        );
        assert_eq!(tp.sharing.threads_idle.load(Ordering::Relaxed), MIN_THREADS);
    }

    #[test]
    fn task_pool_add_thread_test() {
        let tp = TaskPool::new();

        for _ in 0..500 {
            tp.add_thread(None);
        }

        let now = Instant::now();

        while tp.sharing.threads_total.load(Ordering::Relaxed) != 500 + MIN_THREADS {
            thread::sleep(Duration::from_millis(5));
            assert!(now.elapsed() < Duration::from_millis(5000));
        }
    }

    #[test]
    fn task_pool_task_test() {
        let tp = TaskPool::new();

        while tp.sharing.threads_total.load(Ordering::Relaxed) != MIN_THREADS {
            thread::sleep(Duration::from_millis(5));
        }

        tp.spawn_task(Box::new(|| thread::sleep(Duration::from_millis(20))));

        thread::sleep(Duration::from_millis(10));

        assert_eq!(
            tp.sharing.threads_idle.load(Ordering::Relaxed),
            MIN_THREADS - 1
        );

        thread::sleep(Duration::from_millis(11));

        assert_eq!(
            tp.sharing.threads_total.load(Ordering::Relaxed),
            MIN_THREADS
        );
        assert_eq!(tp.sharing.threads_idle.load(Ordering::Relaxed), MIN_THREADS);
    }

    #[test]
    fn task_pool_multi_task_test() {
        let tp = TaskPool::new();

        for n in 0..100 {
            tp.spawn_task(Box::new(move || thread::sleep(Duration::from_millis(n))));
            thread::sleep(Duration::from_micros(100));
        }

        let now = Instant::now();

        thread::sleep(Duration::from_millis(2));

        assert!(tp.sharing.threads_total.load(Ordering::Acquire) > MIN_THREADS);
        assert!(tp.sharing.threads_idle.load(Ordering::Acquire) < MIN_THREADS);

        while tp.sharing.threads_total.load(Ordering::Acquire) != MIN_THREADS
            || tp.sharing.threads_idle.load(Ordering::Acquire) != MIN_THREADS
        {
            thread::sleep(Duration::from_millis(10));
        }

        let elaps = now.elapsed();
        assert!(
            elaps > IDLE_TIME && elaps < IDLE_TIME + Duration::from_millis(200),
            "elaps: {}",
            elaps.as_millis()
        );
    }

    #[test]
    fn task_pool_idle_test() {
        let f = |n: usize, millis: u64| {
            eprintln!("[{n}] start");
            thread::sleep(Duration::from_millis(millis));
            eprintln!("[{n}] end");
        };

        let tp = TaskPool::new();

        for n in 1..=(MIN_THREADS + 1) {
            tp.spawn_task(Box::new(move || {
                #[allow(clippy::cast_possible_truncation)]
                f(
                    n,
                    (IDLE_TIME + Duration::from_millis(20)).as_millis() as u64,
                );
            }));
            thread::sleep(Duration::from_micros(25));
        }

        for n in (MIN_THREADS + 2)..=(MIN_THREADS + 2) {
            tp.spawn_task(Box::new(move || {
                #[allow(clippy::cast_possible_truncation)]
                f(n, 10);
            }));
            thread::sleep(Duration::from_micros(25));
        }

        let dur = IDLE_TIME + Duration::from_millis(15);
        thread::sleep(dur);

        let threads_idle = tp.sharing.threads_idle.load(Ordering::Acquire);
        let threads_total = tp.sharing.threads_total.load(Ordering::Acquire);

        assert_eq!(threads_idle, 1, "idle: {threads_idle}");
        assert!(threads_total >= MIN_THREADS, "total: {}", threads_total);
    }
}

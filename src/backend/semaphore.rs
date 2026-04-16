use std::mem::MaybeUninit;
use std::sync::{Condvar, Mutex};

static mut SEM: MaybeUninit<Semaphore> = MaybeUninit::uninit();
/// A simple semaphore implementation using Mutex and Condvar
pub struct Semaphore {
    count: Mutex<usize>,
    condvar: Condvar,
}

impl Semaphore {
    #[allow(static_mut_refs)]
    pub fn init(count: usize) {
        unsafe {
            SEM.write(Semaphore {
                count: Mutex::new(count),
                condvar: Condvar::new(),
            });
        }
    }

    #[allow(static_mut_refs)]
    pub fn get_sem() -> &'static Self {
        unsafe { SEM.assume_init_ref() }
    }

    fn acquire(&self) {
        let mut count = self.count.lock().unwrap();
        while *count == 0 {
            count = self.condvar.wait(count).unwrap();
        }
        *count -= 1;
    }

    fn release(&self) {
        let mut count = self.count.lock().unwrap();
        *count += 1;
        self.condvar.notify_one();
    }

    pub fn execute<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.acquire();
        let result = f();
        self.release();
        result
    }
}

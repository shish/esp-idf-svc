pub mod asynch {
    #[cfg(all(
        feature = "embedded-async-executor",
        feature = "alloc",
        target_has_atomic = "ptr"
    ))]
    pub mod embedded {
        use core::sync::atomic::{AtomicPtr, Ordering};
        use core::{mem, ptr};

        extern crate alloc;
        use alloc::sync::{Arc, Weak};

        use embedded_svc::utils::asynch::executor::embedded::{
            Notify, NotifyFactory, RunContextFactory, Wait,
        };

        use esp_idf_hal::interrupt;

        pub struct CurrentTaskWait;

        impl CurrentTaskWait {
            pub fn new() -> Self {
                Self
            }
        }

        impl Wait for CurrentTaskWait {
            fn wait(&self) {
                interrupt::task::wait_any_notification();
            }
        }

        pub struct TaskHandle(Arc<AtomicPtr<esp_idf_sys::tskTaskControlBlock>>);

        impl TaskHandle {
            pub fn new() -> Self {
                Self(Arc::new(AtomicPtr::new(ptr::null_mut())))
            }
        }

        impl Drop for TaskHandle {
            fn drop(&mut self) {
                let mut arc = mem::replace(&mut self.0, Arc::new(AtomicPtr::new(ptr::null_mut())));

                // Busy loop until we can destroy the Arc - which means that nobody is actively holding a strong reference to it
                // and thus trying to notify our FreeRtos task, which will likely be destroyed afterwards
                loop {
                    arc = match Arc::try_unwrap(arc) {
                        Ok(_) => break,
                        Err(a) => a,
                    }
                }
            }
        }

        impl NotifyFactory for TaskHandle {
            type Notify = SharedTaskHandle;

            fn notifier(&self) -> Self::Notify {
                SharedTaskHandle(Arc::downgrade(&self.0))
            }
        }

        impl RunContextFactory for TaskHandle {
            fn prerun(&self) {
                let current_task = interrupt::task::current().unwrap();
                let stored_task = self.0.load(Ordering::SeqCst);

                if stored_task.is_null() {
                    self.0.store(current_task, Ordering::SeqCst);
                } else if stored_task != current_task {
                    panic!("Cannot call prerun() twice from two diffeent threads");
                }
            }
        }

        pub struct SharedTaskHandle(Weak<AtomicPtr<esp_idf_sys::tskTaskControlBlock>>);

        impl Notify for SharedTaskHandle {
            fn notify(&self) {
                if let Some(notify) = self.0.upgrade() {
                    let freertos_task = notify.load(Ordering::SeqCst);

                    if !freertos_task.is_null() {
                        unsafe {
                            interrupt::task::notify(freertos_task, 1);
                        }
                    }
                }
            }
        }
    }
}

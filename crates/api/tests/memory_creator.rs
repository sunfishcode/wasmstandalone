#[cfg(not(target_os = "windows"))]
mod not_for_windows {
    use wasmtime::*;
    use wasmtime_environ::{WASM_MAX_PAGES, WASM_PAGE_SIZE};

    use libc::c_void;
    use libc::MAP_FAILED;
    use libc::{mmap, mprotect, munmap};
    use libc::{sysconf, _SC_PAGESIZE};
    use libc::{MAP_ANON, MAP_PRIVATE, PROT_NONE, PROT_READ, PROT_WRITE};

    use std::cell::RefCell;
    use std::io::Error;
    use std::ptr::null_mut;
    use std::sync::{Arc, Mutex};

    struct CustomMemory {
        mem: *mut c_void,
        size: usize,
        used_wasm_pages: RefCell<u32>,
    }

    impl CustomMemory {
        unsafe fn new(num_wasm_pages: u32, max_wasm_pages: u32) -> Self {
            let page_size = sysconf(_SC_PAGESIZE) as usize;
            let guard_size = page_size;
            let size = max_wasm_pages as usize * WASM_PAGE_SIZE as usize + guard_size;
            let used_size = num_wasm_pages as usize * WASM_PAGE_SIZE as usize;
            assert_eq!(size % page_size, 0); // we rely on WASM_PAGE_SIZE being multiple of host page size

            let mem = mmap(null_mut(), size, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0);
            assert_ne!(mem, MAP_FAILED, "mmap failed: {}", Error::last_os_error());

            let r = mprotect(mem, used_size, PROT_READ | PROT_WRITE);
            assert_eq!(r, 0, "mprotect failed: {}", Error::last_os_error());

            Self {
                mem,
                size,
                used_wasm_pages: RefCell::new(num_wasm_pages),
            }
        }
    }

    impl Drop for CustomMemory {
        fn drop(&mut self) {
            let r = unsafe { munmap(self.mem, self.size) };
            assert_eq!(r, 0, "munmap failed: {}", Error::last_os_error());
        }
    }

    unsafe impl LinearMemory for CustomMemory {
        fn size(&self) -> u32 {
            *self.used_wasm_pages.borrow()
        }

        fn grow(&self, delta: u32) -> Option<u32> {
            let delta_size = (delta as usize).checked_mul(WASM_PAGE_SIZE as usize)?;

            let prev_pages = *self.used_wasm_pages.borrow();
            let prev_size = (prev_pages as usize).checked_mul(WASM_PAGE_SIZE as usize)?;

            let new_pages = prev_pages.checked_add(delta)?;
            let new_size = (new_pages as usize).checked_mul(WASM_PAGE_SIZE as usize)?;

            let guard_size = unsafe { sysconf(_SC_PAGESIZE) as usize };

            if new_size > self.size - guard_size {
                return None;
            }
            unsafe {
                let start = (self.mem as *mut u8).add(prev_size) as _;
                let r = mprotect(start, delta_size, PROT_READ | PROT_WRITE);
                assert_eq!(r, 0, "mprotect failed: {}", Error::last_os_error());
            }

            *self.used_wasm_pages.borrow_mut() = new_pages;
            Some(prev_pages)
        }

        fn as_ptr(&self) -> *mut u8 {
            self.mem as *mut u8
        }
    }

    struct CustomMemoryCreator {
        pub num_created_memories: Mutex<usize>,
    }

    impl CustomMemoryCreator {
        pub fn new() -> Self {
            Self {
                num_created_memories: Mutex::new(0),
            }
        }
    }

    unsafe impl MemoryCreator for CustomMemoryCreator {
        fn new_memory(&self, ty: MemoryType) -> Result<Box<dyn LinearMemory>, String> {
            let max = ty.limits().max().unwrap_or(WASM_MAX_PAGES);
            unsafe {
                let mem = Box::new(CustomMemory::new(ty.limits().min(), max));
                *self.num_created_memories.lock().unwrap() += 1;
                Ok(mem)
            }
        }
    }

    #[test]
    fn host_memory() -> anyhow::Result<()> {
        let mem_creator = Arc::new(CustomMemoryCreator::new());
        let mut config = Config::default();
        config.with_host_memory(mem_creator.clone());
        let engine = Engine::new(&config);
        let store = Store::new(&engine);

        let module = Module::new(
            &store,
            r#"
            (module
                (memory (export "memory") 1)
            )
        "#,
        )?;
        Instance::new(&module, &[])?;

        assert_eq!(*mem_creator.num_created_memories.lock().unwrap(), 1);

        Ok(())
    }
}

use std::mem;

// use crate::map::map_vdso;
// use libvdsoexample::{interface::TestIf, *};
use libvdsoexample::*;
use memmap2::MmapMut;

// mod map;

struct MemImpl;

#[crate_interface::impl_interface]
impl MemIf for MemImpl {
    #[doc = " 分配用于vDSO和vVAR的空间，返回指向首地址的指针。"]
    #[doc = ""]
    #[doc = " 若需要实现vDSO和vVAR在多地址空间的共享，则需要在分配时使这块空间可被共享。"]
    fn alloc(size: usize) -> *mut u8 {
        let mut map = MmapMut::map_anon(size).unwrap();
        let ptr = map.as_mut_ptr();
        mem::forget(map);
        ptr
    }

    #[doc = " 从`alloc`返回的空间中，设置其中一块的访问权限。"]
    #[doc = ""]
    #[doc = " `flags`可能包含：READ、WRITE、EXECUTE、USER。"]
    fn protect(addr: *mut u8, len: usize, flags: MappingFlags) {
        let mut libc_flag = libc::PROT_READ;
        if flags.contains(MappingFlags::EXECUTE) {
            libc_flag |= libc::PROT_EXEC;
        }
        if flags.contains(MappingFlags::WRITE) {
            libc_flag |= libc::PROT_WRITE;
        }
        unsafe {
            if libc::mprotect(addr as _, len, libc_flag) == libc::MAP_FAILED as _ {
                panic!("vdso: mprotect res failed");
            }
        };
    }
}

struct TestImpl(usize);

impl TestIf for TestImpl {
    fn test_fn1(&self, arg: usize) -> usize {
        log::info!("test_fn1 called with arg: {}, self.0: {}", arg, self.0);
        self.0 + arg
    }

    fn test_fn2(&mut self, arg: usize) -> usize {
        log::info!("test_fn2 called with arg: {}, self.0: {}", arg, self.0);
        self.0 += arg;
        self.0
    }

    fn test_fn3(arg: usize) {
        log::info!("test_fn3 called with arg: {}", arg);
    }
}

fn main() {
    env_logger::init();
    log::info!("Starting VDSO test...");
    let regions = load_and_init();
    println!("vDSO and vVAR loaded with the following regions:");
    for (i, (addr, size, flags)) in regions.iter().enumerate() {
        println!(
            "Region {}: Address = 0x{:016x}, Size = {}, Flags = {:?}",
            i, *addr as usize, size, flags
        );
    }
    // let example: ArgumentExample = get_shared();
    // assert!(
    //     example.i == 0,
    //     "Expected get_shared() to return 0, got {}",
    //     example.i
    // );
    // set_shared(1);
    // let example: ArgumentExample = get_shared();
    // assert!(
    //     example.i == 1,
    //     "Expected get_shared() to return 1, got {}",
    //     example.i
    // );
    // let example: ArgumentExample = get_private();
    // assert!(
    //     example.i == 0,
    //     "Expected get_shared() to return 1, got {}",
    //     example.i
    // );
    // set_private(1);
    // let example: ArgumentExample = get_private();
    // assert!(
    //     example.i == 1,
    //     "Expected get_shared() to return 1, got {}",
    //     example.i
    // );

    // assert_eq!(test_args(Some(1), Ok(2), (3, 4)), (Some(2), Ok(3), (4, 5)));

    init_vtable_TestIf::<TestImpl>();
    let mut test_impl = TestImpl(10);
    let ptr = &mut test_impl as *mut TestImpl as *mut ();
    test_call(ptr);
    println!("Test passed!");
    // drop(map);
}

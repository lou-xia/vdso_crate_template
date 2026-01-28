use alloc::string::ToString;
use core::str::from_utf8;
use crate_interface::{call_interface, def_interface};
use include_bytes_aligned::include_bytes_aligned;
pub use page_table_entry::MappingFlags;
use vdso_example::VvarData;
use xmas_elf::program::SegmentData;

#[def_interface]
pub trait MemIf {
    /// 分配用于vDSO和vVAR的空间，返回指向首地址的指针。
    ///
    /// 若需要实现vDSO和vVAR在多地址空间的共享，则需要在分配时使这块空间可被共享。
    fn alloc(size: usize) -> *mut u8;

    /// 从`alloc`返回的空间中，设置其中一块的访问权限。
    ///
    /// `flags`可能包含：READ、WRITE、EXECUTE、USER。
    fn protect(addr: *mut u8, len: usize, flags: MappingFlags);
}

const PAGES_SIZE: usize = 0x1000;
const VDSO: &[u8] = include_bytes_aligned!(8, "../../libvdsoexample.so");
const VDSO_SIZE: usize = ((VDSO.len() + PAGES_SIZE - 1) & (!(PAGES_SIZE - 1))) + PAGES_SIZE; // 额外加了一页，用于bss段等未出现在文件中的段
const VVAR_SIZE: usize = (core::mem::size_of::<VvarData>() + PAGES_SIZE - 1) & (!(PAGES_SIZE - 1));

pub fn load_so() -> *mut u8 {
    let vdso_map = call_interface!(MemIf::alloc(VVAR_SIZE + VDSO_SIZE));
    #[cfg(feature = "log")]
    {
        log::info!(
            "vVAR: [0x{:016x}, 0x{:016x})",
            vdso_map as usize,
            (vdso_map as usize) + VVAR_SIZE
        );
        log::info!(
            "vDSO: [0x{:016x}, 0x{:016x})",
            (vdso_map as usize) + VVAR_SIZE,
            (vdso_map as usize) + VVAR_SIZE + VDSO_SIZE
        );
    }

    // vVAR初始化
    #[cfg(feature = "log")]
    log::info!("mapping vVAR...");
    #[cfg(feature = "log")]
    log::info!(
        "protect: [0x{:016x}, 0x{:016x}), {:?}",
        vdso_map as usize,
        (vdso_map as usize) + core::mem::size_of::<VvarData>(),
        MappingFlags::READ | MappingFlags::WRITE | MappingFlags::USER
    );
    call_interface!(MemIf::protect(
        vdso_map,
        core::mem::size_of::<VvarData>(),
        MappingFlags::READ | MappingFlags::WRITE | MappingFlags::USER
    ));
    unsafe { (vdso_map as *mut _ as *mut VvarData).write(VvarData::default()) };

    // vDSO初始化
    #[cfg(feature = "log")]
    log::info!("mapping vDSO...");

    let vdso_elf = xmas_elf::ElfFile::new(VDSO).expect("Error parsing app ELF file.");
    if let Some(interp) = vdso_elf
        .program_iter()
        .find(|ph| ph.get_type() == Ok(xmas_elf::program::Type::Interp))
    {
        let interp = match interp.get_data(&vdso_elf) {
            Ok(SegmentData::Undefined(data)) => data,
            _ => panic!("Invalid data in Interp Elf Program Header"),
        };

        let interp_path = from_utf8(interp).expect("Interpreter path isn't valid UTF-8");
        // remove trailing '\0'
        let _interp_path = interp_path.trim_matches(char::from(0)).to_string();
        #[cfg(feature = "log")]
        log::debug!("Interpreter path: {:?}", _interp_path);
    }
    let elf_base_addr = Some((vdso_map as usize) + VVAR_SIZE);
    let segments = elf_parser::get_elf_segments(&vdso_elf, elf_base_addr);
    let relocate_pairs = elf_parser::get_relocate_pairs(&vdso_elf, elf_base_addr);
    for segment in segments {
        if segment.size == 0 {
            #[cfg(feature = "log")]
            log::warn!(
                "Segment with size 0 found, skipping: {:?}, {:#x}, {:?}",
                segment.vaddr,
                segment.size,
                segment.flags
            );
            continue;
        }
        #[cfg(feature = "log")]
        log::debug!(
            "{:?}, {:#x}, {:?}",
            segment.vaddr,
            segment.size,
            segment.flags
        );

        if let Some(data) = segment.data {
            assert!(data.len() <= segment.size);
            let src = data.as_ptr();
            let dst = segment.vaddr.as_usize() as *mut u8;
            let count = data.len();
            unsafe {
                core::ptr::copy_nonoverlapping(src, dst, count);
                if segment.size > count {
                    core::ptr::write_bytes(dst.add(count), 0, segment.size - count);
                }
            }
        } else {
            unsafe { core::ptr::write_bytes(segment.vaddr.as_usize() as *mut u8, 0, segment.size) };
        }

        #[cfg(feature = "log")]
        log::info!(
            "protect: [0x{:016x}, 0x{:016x}), {:?}",
            segment.vaddr.as_usize(),
            segment.vaddr.as_usize() + segment.size,
            segment.flags
        );
        call_interface!(MemIf::protect(
            segment.vaddr.as_usize() as *mut u8,
            segment.size,
            segment.flags
        ));
    }

    for relocate_pair in relocate_pairs {
        let src: usize = relocate_pair.src.into();
        let dst: usize = relocate_pair.dst.into();
        let count = relocate_pair.count;
        #[cfg(feature = "log")]
        log::info!(
            "Relocate: src: 0x{:x}, dst: 0x{:x}, count: {}",
            src,
            dst,
            count
        );
        unsafe { core::ptr::copy_nonoverlapping(src.to_ne_bytes().as_ptr(), dst as *mut u8, count) }
    }

    #[cfg(feature = "log")]
    log::info!("mapping complete!");

    ((vdso_map as usize) + VVAR_SIZE) as _
}

use std::{fmt::format, fs, path::Path};

use xmas_elf::symbol_table::Entry;

use crate::BuildConfig;

/// 在输出路径中创建一个Rust项目“api”，用于：
/// - 向调用者提供so文件和vvar数据结构的定义，用于调用者初始化vdso。
/// - 向调用者提供调用vdso的接口定义。
pub(crate) fn gen_api(config: &BuildConfig) {
    let lib_path = Path::new(&config.out_dir).join(&config.api_lib_name);
    let src_path = lib_path.join("src");
    fs::create_dir_all(&src_path).unwrap();
    let cargo_toml = cargo_toml_content(config);
    let lib_rs = lib_rs_content(config);
    let api_rs = api_rs_content(config);
    let loader_rs = loader_rs_content(config);

    fs::write(&lib_path.join("Cargo.toml"), cargo_toml).unwrap();
    fs::write(&src_path.join("lib.rs"), lib_rs).unwrap();
    fs::write(&src_path.join("api.rs"), api_rs).unwrap();
    fs::write(&src_path.join("loader.rs"), loader_rs).unwrap();
}

fn cargo_toml_content(config: &BuildConfig) -> String {
    let absolute_src_dir = fs::canonicalize(Path::new(&config.src_dir)).unwrap();
    format!(
        r#"[package]
name = "{}"
edition = "2021"

[dependencies]
{} = {{ path = "{}" }}
log = {{ version = "0.4", optional = true }}
crate_interface = "0.2"
page_table_entry = "0.5.7"
include_bytes_aligned = "0.1.4"
xmas-elf = "0.9.0"
elf_parser = {{ git = "https://github.com/rosy233333/elf_parser.git" }}

[features]
log = ["dep:log"]
default = []
"#,
        config.api_lib_name,
        config.package_name,
        absolute_src_dir.display()
    )
}

fn lib_rs_content(_config: &BuildConfig) -> String {
    String::from(
        r#"#![no_std]
pub mod api;
pub use api::*;
pub mod loader;
pub use loader::*;

extern crate alloc;
"#,
    )
}

fn api_rs_content(config: &BuildConfig) -> String {
    // 修改自https://github.com/AsyncModules/vsched/blob/e728dadd75aeb8da5cec1642320a6bd24af5b5bb/vsched_apis/build.rs的build_vsched_api函数

    // 获取vDSO的 api
    let api_rs_path = Path::new(&config.src_dir)
        .join("src")
        .join("api")
        .with_extension("rs");
    // println!("api.rs path: {}", api_rs_path.display());
    let mut vsched_api_file_content = fs::read_to_string(&api_rs_path).unwrap();
    vsched_api_file_content = vsched_api_file_content.split('\n').collect();
    vsched_api_file_content = vsched_api_file_content.split('\t').collect();
    vsched_api_file_content = vsched_api_file_content.split("    ").collect();
    // println!("vsched_api_file_content: {}", vsched_api_file_content);
    let elf_path = Path::new(&config.out_dir).join(format!("{}.so", config.so_name));
    let so_content = fs::read(&elf_path).unwrap();
    let vdso_elf = xmas_elf::ElfFile::new(&so_content).expect("Error parsing app ELF file.");

    let re = regex::Regex::new(
        r#"#\[unsafe\(no_mangle\)\]pub extern \"C\" fn ([a-zA-Z0-9_]+)(\([a-zA-Z0-9_:]?[^\{]*\)[->]?[^\{]*) \{"#,
    )
    .unwrap();

    let mut fns = vec![];
    for (_, [name, args]) in re
        .captures_iter(&vsched_api_file_content)
        .map(|c| c.extract())
    {
        // println!("name: {}\nargs: {}", name, args);
        fns.push((name, args));
    }

    let interface_rs_path = Path::new(&config.src_dir)
        .join("src")
        .join("interface")
        .with_extension("rs");
    // println!("api.rs path: {}", api_rs_path.display());
    let mut vsched_interface_file_content = fs::read_to_string(&interface_rs_path).unwrap();
    vsched_interface_file_content = vsched_interface_file_content.split('\n').collect();
    vsched_interface_file_content = vsched_interface_file_content.split('\t').collect();
    vsched_interface_file_content = vsched_interface_file_content.split("    ").collect();
    println!(
        "cargo:warning=vsched_interface_file_content: {}",
        vsched_interface_file_content
    );

    let re = regex::Regex::new(r#"pub trait ([a-zA-Z0-9_]+) \{([^\{\}]+)\}"#).unwrap();
    // let re = regex::Regex::new(r#"pub trait ([a-zA-Z0-9_]+)\{(.)?"#).unwrap();

    // 获取vDSO的 interface
    let mut traits = vec![];
    for (_, [name, fns]) in re
        .captures_iter(&vsched_interface_file_content)
        .map(|c| c.extract())
    {
        let mut fns_name = vec![];
        let fns_name_re = regex::Regex::new(r#"fn ([a-zA-Z0-9_]+)\("#).unwrap();
        fns_name_re
            .captures_iter(&fns)
            .map(|c| c.extract())
            .for_each(|(_, [fn_name])| {
                fns_name.push(fn_name);
            });
        traits.push((name, fns_name));
    }
    // println!("cargo:warning=traits: {:?}", traits);
    // panic!("pause");

    // pub use vdso库中的内容
    let pub_use_vdso_str = format!(
        "extern crate {};\npub use self::{}::*;\n\n",
        config.package_name, config.package_name
    );
    // vdso_vtable 数据结构定义
    let mut vdso_vtable_struct_str = "struct VdsoVTable {\n".to_string();
    for (name, args) in fns.iter() {
        vdso_vtable_struct_str.push_str(&format!("    pub {}: Option<fn{}>,\n", name, args));
    }
    for (name, fns_name) in traits.iter() {
        let init_fn_name = format!("init_vtable_{}", name);
        let args = format!(
            "({})",
            fns_name
                .iter()
                .map(|_fn_name| "usize")
                .collect::<Vec<_>>()
                .join(", ")
        );
        vdso_vtable_struct_str
            .push_str(&format!("    pub {}: Option<fn{}>,\n", init_fn_name, args));
    }
    vdso_vtable_struct_str.push_str("}\n");

    // 定义静态的 VDSO_VTABLE
    let mut static_vdso_vtable_str =
        "\nstatic mut VDSO_VTABLE: VdsoVTable = VdsoVTable {\n".to_string();
    for (name, _) in fns.iter() {
        static_vdso_vtable_str.push_str(&format!("    {}: None,\n", name));
    }
    for (name, _) in traits.iter() {
        let init_fn_name = format!("init_vtable_{}", name);
        static_vdso_vtable_str.push_str(&format!("    {}: None,\n", init_fn_name));
    }
    static_vdso_vtable_str.push_str("};\n");

    // 运行时初始化 vsched_table 的函数
    let dyn_sym_table = vdso_elf.find_section_by_name(".dynsym").unwrap();
    let dyn_sym_table = match dyn_sym_table.get_data(&vdso_elf) {
        Ok(xmas_elf::sections::SectionData::DynSymbolTable64(dyn_sym_table)) => dyn_sym_table,
        _ => panic!("Invalid data in .dynsym section"),
    };
    let mut fn_init_vdso_vtable_str = INIT_VDSO_VTABLE_STR.to_string();

    for (name, args) in fns.iter() {
        let mut sym_value: usize = 0;
        for dynsym in dyn_sym_table {
            let sym_name = dynsym.get_name(&vdso_elf).unwrap();
            if sym_name == *name {
                sym_value = dynsym.value() as usize;
                break;
            }
        }
        assert!(sym_value != 0, "Function {} not found in .dynsym", name);

        fn_init_vdso_vtable_str.push_str(&format!(
            r#"    // {}:
    let fn_ptr = base + 0x{:x};
    #[cfg(feature = "log")]
    log::debug!("{}: 0x{{:x}}", fn_ptr);
    let f: fn{} = unsafe {{ core::mem::transmute(fn_ptr) }};
    unsafe {{ VDSO_VTABLE.{}  = Some(f); }}

"#,
            name, sym_value, name, args, name
        ));
    }

    for (name, fns_name) in traits.iter() {
        let init_fn_name = format!("init_vtable_{}", name);
        let mut sym_value: usize = 0;
        for dynsym in dyn_sym_table {
            let sym_name = dynsym.get_name(&vdso_elf).unwrap();
            if sym_name == init_fn_name.as_str() {
                sym_value = dynsym.value() as usize;
                break;
            }
        }
        assert!(
            sym_value != 0,
            "Function {} not found in .dynsym",
            init_fn_name
        );

        let args = format!(
            "({})",
            fns_name
                .iter()
                .map(|_fn_name| "usize")
                .collect::<Vec<_>>()
                .join(", ")
        );

        fn_init_vdso_vtable_str.push_str(&format!(
            r#"    // {}:
    let fn_ptr = base + 0x{:x};
    #[cfg(feature = "log")]
    log::debug!("{}: 0x{{:x}}", fn_ptr);
    let f: fn{} = unsafe {{ core::mem::transmute(fn_ptr) }};
    unsafe {{ VDSO_VTABLE.{}  = Some(f); }}

"#,
            init_fn_name, sym_value, init_fn_name, args, init_fn_name
        ));
    }

    fn_init_vdso_vtable_str.push_str(
        r#"}
    "#,
    );

    fn_init_vdso_vtable_str.push_str(
        r#"
pub fn load_and_init() {
    let vdso = crate::load_so();
    unsafe{ init_vdso_vtable(vdso as _) };
}
"#,
    );

    // 构建给内核和用户运行时使用的接口
    let mut apis = vec![];

    // api部分
    for (name, args) in fns.iter() {
        let re = regex::Regex::new(r#"\(([a-zA-Z0-9_:]?.*)\)"#).unwrap();
        let mut fn_args = String::new();
        for (_, [ident_ty]) in re.captures_iter(args).map(|c| c.extract()) {
            // println!("{}: {}", name, args);
            let ident_str: Vec<&str> = ident_ty
                .split(",")
                .map(|s| {
                    let idx = s.find(":");
                    if let Some(idx) = idx {
                        let ident = s[..idx].trim();
                        ident
                    } else {
                        ""
                    }
                })
                .collect();
            for ident in ident_str.iter() {
                if ident.len() > 0 {
                    fn_args.push_str(&format!("{}, ", ident));
                }
            }
            fn_args = fn_args.trim_end_matches(", ").to_string();
            // println!("{:?}", fn_args);
        }

        apis.push(format!(
            r#"
pub fn {}{} {{
    if let Some(f) = unsafe {{ VDSO_VTABLE.{} }} {{
        #[cfg(feature = "log")]
        log::debug!("Calling {} at 0x{{:x}}.", f as *const () as usize);
        f({})
    }} else {{
        panic!("{} is not initialized")
    }}
}}
"#,
            name, args, name, name, fn_args, name
        ));
    }

    // trait的初始化api部分
    for (name, fns_name) in traits.iter() {
        let init_fn_name = format!("init_vtable_{}", name);

        let fn_args = fns_name
            .iter()
            .map(|fn_name| format!("T::{} as usize", fn_name))
            .collect::<Vec<_>>()
            .join(", ");

        apis.push(format!(
            r#"
pub fn {}<T:{}>() {{
    if let Some(f) = unsafe {{ VDSO_VTABLE.{} }} {{
        #[cfg(feature = "log")]
        log::debug!("Calling {} at 0x{{:x}}.", f as *const () as usize);
        f({})
    }} else {{
        panic!("{} is not initialized")
    }}
}}
"#,
            init_fn_name, name, init_fn_name, init_fn_name, fn_args, init_fn_name
        ));
    }

    // println!("apis: {:?}", apis);

    let mut api_content = String::new();
    api_content.push_str(&pub_use_vdso_str);
    api_content.push_str(&vdso_vtable_struct_str);
    api_content.push_str(&static_vdso_vtable_str);
    api_content.push_str(&fn_init_vdso_vtable_str);

    for api in apis.iter() {
        api_content.push_str(api);
    }

    api_content
}

const INIT_VDSO_VTABLE_STR: &str = r#"
pub unsafe fn init_vdso_vtable(base: u64) {
"#;

fn loader_rs_content(config: &BuildConfig) -> String {
    let use_content = format!(
        r#"use alloc::string::ToString;
use core::str::from_utf8;
use crate_interface::{{call_interface, def_interface}};
use include_bytes_aligned::include_bytes_aligned;
pub use page_table_entry::MappingFlags;
use {}::VvarData;
use xmas_elf::program::SegmentData;
"#,
        config.package_name
    );

    let interface_content = String::from(
        r#"
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
"#,
    );

    let const_content = format!(
        r#"
const PAGES_SIZE: usize = {};
const VDSO: &[u8] = include_bytes_aligned!(8, "../../{}.so");
const VDSO_SIZE: usize = ((VDSO.len() + PAGES_SIZE - 1) & (!(PAGES_SIZE - 1))) + PAGES_SIZE; // 额外加了一页，用于bss段等未出现在文件中的段
const VVAR_SIZE: usize = (core::mem::size_of::<VvarData>() + PAGES_SIZE - 1) & (!(PAGES_SIZE - 1));
"#,
        config.page_size, config.so_name
    );

    let load_so_content = String::from(
        r#"
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
"#,
    );

    use_content + &interface_content + &const_content + &load_so_content
}

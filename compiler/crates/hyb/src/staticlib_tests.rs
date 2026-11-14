use super::{install_staticlib, lib_arch_matches, sanitize_staticlib_file, strip_lto_sections};
use std::sync::atomic::{AtomicUsize, Ordering};

fn probe(bytes: &[u8]) -> bool {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "hyb-macho-{}-{}.a",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, bytes).unwrap();
    let result = lib_arch_matches(&path);
    std::fs::remove_file(path).unwrap();
    result
}

fn macho(cpu: u32, big_endian: bool) -> Vec<u8> {
    // mach_header_64: MH_MAGIC_64, cputype, cpusubtype, MH_OBJECT,
    // ncmds, sizeofcmds, flags, reserved.
    [0xfeedfacf_u32, cpu, 0, 1, 0, 0, 0, 0]
        .into_iter()
        .flat_map(|n| {
            if big_endian {
                n.to_be_bytes()
            } else {
                n.to_le_bytes()
            }
        })
        .collect()
}

fn host_cpu() -> u32 {
    match std::env::consts::ARCH {
        "x86_64" => 0x01000007,
        "aarch64" => 0x0100000c,
        other => panic!("unsupported test host {other}"),
    }
}

fn member(name: &str, data: &[u8]) -> Vec<u8> {
    assert!(name.len() <= 16);
    let mut out = format!(
        "{name:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
        0,
        0,
        0,
        0,
        data.len()
    )
    .into_bytes();
    assert_eq!(out.len(), 60);
    out.extend_from_slice(data);
    if !data.len().is_multiple_of(2) {
        out.push(b'\n');
    }
    out
}

fn bsd_member(name: &[u8], data: &[u8]) -> Vec<u8> {
    let mut payload = name.to_vec();
    payload.extend_from_slice(data);
    member(&format!("#1/{}", name.len()), &payload)
}

fn archive(members: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"!<arch>\n".to_vec();
    for m in members {
        out.extend_from_slice(m);
    }
    out
}

#[test]
fn macho_raw_and_short_archive_members_match_only_host_arch() {
    for cpu in [0x01000007, 0x0100000c, 7, 12, 0x01000012] {
        for big in [false, true] {
            let obj = macho(cpu, big);
            assert_eq!(probe(&obj), cpu == host_cpu(), "raw cpu={cpu:x} big={big}");
            assert_eq!(
                probe(&archive(&[member("hyrt.o/", &obj)])),
                cpu == host_cpu()
            );
        }
    }
}

#[test]
fn macho_bsd_names_and_extended_symbol_tables() {
    // LLVM/Darwin archives put NUL-padded names in the member payload,
    // including __.SYMDEF. Sizes include the name, and odd members are padded.
    let obj = macho(host_cpu(), false);
    for symbol in [b"__.SYMDEF\0\0\0".as_slice(), b"__.SYMDEF SORTED\0"] {
        assert!(probe(&archive(&[
            bsd_member(symbol, &[0; 131_073]),
            bsd_member(b"hyrt.long.rcgu.o\0\0\0", &obj),
        ])));
    }
    assert!(probe(&archive(&[
        member("/", &[0; 3]),
        member("//", b"long-name-table"),
        member("__.SYMDEF/", &[0; 4]),
        bsd_member(b"odd.o", &obj),
    ])));
    let foreign = if host_cpu() == 0x01000007 {
        0x0100000c
    } else {
        0x01000007
    };
    assert!(!probe(&archive(&[bsd_member(
        b"foreign.o",
        &macho(foreign, false)
    )])));
}

#[test]
fn macho_invalid_headers_and_archive_names_fail_closed() {
    let obj = macho(host_cpu(), false);
    for n in 0..32 {
        assert!(!probe(&obj[..n]), "truncated at {n}");
    }
    let mut executable = obj.clone();
    executable[12] = 2; // MH_EXECUTE, not an object file
    assert!(!probe(&executable));
    let mut fat = obj.clone();
    fat[..4].copy_from_slice(&[0xca, 0xfe, 0xba, 0xbe]);
    assert!(!probe(&fat));
    for name in ["#1/", "#1/no", "#1/0", "#1/999999999999"] {
        assert!(!probe(&archive(&[member(name, &obj)])), "bad name {name}");
    }
    let valid = archive(&[bsd_member(b"hyrt.o", &obj)]);
    assert!(!probe(&valid[..valid.len() - 1]), "member exceeds file");
}

#[test]
fn archive_object_probe_cannot_read_into_next_member() {
    // A 4-byte first member must not borrow the next ar header's bytes to
    // look like a 20-byte ELF/COFF object. The old reader did exactly that.
    let coff = if host_cpu() == 0x01000007 {
        [0x64, 0x86, 0, 0]
    } else {
        [0x64, 0xaa, 0, 0]
    };
    assert!(!probe(&archive(&[
        member("short.o/", &coff),
        member("next.o/", &[0; 32])
    ])));
    assert!(!probe(&archive(&[
        bsd_member(b"short.o", &macho(host_cpu(), false)[..20]),
        member("next.o/", &[0; 32])
    ])));
}

fn macho64_section(segname: &[u8], sectname: &[u8], size: u64, offset: u32, flags: u32) -> Vec<u8> {
    let mut s = vec![0u8; 80];
    s[0..sectname.len()].copy_from_slice(sectname);
    s[16..16 + segname.len()].copy_from_slice(segname);
    s[40..48].copy_from_slice(&size.to_le_bytes());
    s[48..52].copy_from_slice(&offset.to_le_bytes());
    s[60..64].copy_from_slice(&2u32.to_le_bytes()); // nreloc
    s[64..68].copy_from_slice(&flags.to_le_bytes());
    s
}

fn macho64_segment(segname: &[u8], sections: &[Vec<u8>]) -> Vec<u8> {
    let cmdsize = (72 + sections.len() * 80) as u32;
    let mut seg = vec![0u8; 72];
    seg[0..4].copy_from_slice(&0x19u32.to_le_bytes()); // LC_SEGMENT_64
    seg[4..8].copy_from_slice(&cmdsize.to_le_bytes());
    seg[8..8 + segname.len()].copy_from_slice(segname);
    seg[64..68].copy_from_slice(&(sections.len() as u32).to_le_bytes());
    for sec in sections {
        seg.extend_from_slice(sec);
    }
    seg
}

#[test]
fn strip_lto_sections_macho_64_32_and_fat_archives() {
    let seg1 = macho64_segment(
        b"",
        &[
            macho64_section(b"__TEXT", b"__text", 128, 512, 0x80000400),
            macho64_section(b"__LLVM", b"__bitcode", 4096, 640, 0),
            macho64_section(b"__LLVM", b"__cmdline", 256, 4736, 0),
        ],
    );
    let seg2 = macho64_segment(
        b"__LLVM",
        &[macho64_section(b"__LLVM", b"__lto", 1024, 4992, 0)],
    );
    let seg3 = macho64_segment(
        b"__GNU_LTO",
        &[macho64_section(b"__GNU_LTO", b".gnu.lto_main", 512, 6016, 0)],
    );
    let sizeofcmds = (seg1.len() + seg2.len() + seg3.len()) as u32;
    let mut obj64 = Vec::new();
    for w in [0xfeedfacf_u32, host_cpu(), 0, 1, 3, sizeofcmds, 0, 0] {
        obj64.extend_from_slice(&w.to_le_bytes());
    }
    obj64.extend_from_slice(&seg1);
    obj64.extend_from_slice(&seg2);
    obj64.extend_from_slice(&seg3);

    let mut ar = archive(&[
        bsd_member(b"__.SYMDEF SORTED\0", &[0; 64]),
        bsd_member(b"hyrt.rcgu.o\0\0\0\0\0", &obj64),
    ]);
    let orig_len = ar.len();
    assert!(ar.windows(9).any(|w| w == b"__bitcode"));
    assert!(ar.windows(9).any(|w| w == b"__cmdline"));
    assert!(ar.windows(6).any(|w| w == b"__LLVM"));
    assert!(ar.windows(9).any(|w| w == b"__GNU_LTO"));

    strip_lto_sections(&mut ar);

    assert_eq!(ar.len(), orig_len, "archive size must remain identical");
    assert!(probe(&ar), "sanitized Mach-O archive must still match host arch");
    for forbidden in [
        b"__LLVM".as_slice(),
        b"__GNU_LTO",
        b"__bitcode",
        b"__cmdline",
        b".gnu.lto_",
    ] {
        assert!(
            !ar.windows(forbidden.len()).any(|w| w == forbidden),
            "forbidden token {:?} remained in archive",
            std::str::from_utf8(forbidden)
        );
    }
    assert!(
        ar.windows(6).any(|w| w == b"__TEXT") && ar.windows(6).any(|w| w == b"__text"),
        "__TEXT,__text section must remain intact"
    );
    assert!(
        ar.windows(12).any(|w| w == b"__debug_noop"),
        "LTO sections must be rewritten to __DWARF,__debug_noop"
    );

    // Also test Universal Fat container wrapping the Mach-O 64 object.
    let mut fat = Vec::new();
    fat.extend_from_slice(&0xcafebabe_u32.to_be_bytes());
    fat.extend_from_slice(&1u32.to_be_bytes()); // nfat_arch = 1
    fat.extend_from_slice(&host_cpu().to_be_bytes());
    fat.extend_from_slice(&0u32.to_be_bytes()); // cpusubtype
    fat.extend_from_slice(&28u32.to_be_bytes()); // offset
    fat.extend_from_slice(&(obj64.len() as u32).to_be_bytes()); // size
    fat.extend_from_slice(&0u32.to_be_bytes()); // align
    fat.extend_from_slice(&obj64);
    strip_lto_sections(&mut fat);
    assert!(!fat.windows(6).any(|w| w == b"__LLVM"));
    assert!(fat.windows(6).any(|w| w == b"__TEXT"));
}

#[test]
fn strip_lto_sections_elf_64_and_32_archives() {
    let em_host: u16 = match std::env::consts::ARCH {
        "x86_64" => 0x3E,
        "aarch64" => 0xB7,
        other => panic!("unsupported test host {other}"),
    };
    let shstrtab = b"\0.text\0.llvmbc\0.llvmcmd\0.gnu.lto_.icf\0.custom\0.shstrtab\0";
    let shoff = 64usize + shstrtab.len();
    let shnum = 7u16;
    let mut elf = vec![0u8; shoff + (shnum as usize) * 64];
    elf[0..4].copy_from_slice(b"\x7fELF");
    elf[4] = 2; // ELFCLASS64
    elf[5] = 1; // ELFDATA2LSB
    elf[6] = 1; // EV_CURRENT
    elf[16..18].copy_from_slice(&1u16.to_le_bytes()); // ET_REL
    elf[18..20].copy_from_slice(&em_host.to_le_bytes());
    elf[20..24].copy_from_slice(&1u32.to_le_bytes());
    elf[40..48].copy_from_slice(&(shoff as u64).to_le_bytes());
    elf[58..60].copy_from_slice(&64u16.to_le_bytes()); // e_shentsize
    elf[60..62].copy_from_slice(&shnum.to_le_bytes());
    elf[62..64].copy_from_slice(&6u16.to_le_bytes()); // e_shstrndx
    elf[64..64 + shstrtab.len()].copy_from_slice(shstrtab);

    let write_shdr = |buf: &mut [u8], idx: usize, name: u32, ty: u32, flags: u64, off: u64, size: u64| {
        let base = shoff + idx * 64;
        buf[base..base + 4].copy_from_slice(&name.to_le_bytes());
        buf[base + 4..base + 8].copy_from_slice(&ty.to_le_bytes());
        buf[base + 8..base + 16].copy_from_slice(&flags.to_le_bytes());
        buf[base + 24..base + 32].copy_from_slice(&off.to_le_bytes());
        buf[base + 32..base + 40].copy_from_slice(&size.to_le_bytes());
    };
    write_shdr(&mut elf, 1, 1, 1, 6, 128, 64); // .text
    write_shdr(&mut elf, 2, 7, 1, 0x80000000, 192, 1024); // .llvmbc
    write_shdr(&mut elf, 3, 15, 1, 0x80000000, 1216, 128); // .llvmcmd
    write_shdr(&mut elf, 4, 24, 1, 0, 1344, 256); // .gnu.lto_.icf
    write_shdr(&mut elf, 5, 38, 0x6fff4c0c, 0, 1600, 512); // .custom with SHT_LLVM_LTO
    write_shdr(&mut elf, 6, 46, 3, 0, 64, shstrtab.len() as u64); // .shstrtab

    let mut ar = archive(&[
        member("/", &[0; 4]),
        member("//", b"hyrt.rcgu.o/\n"),
        member("/0", &elf),
    ]);
    let orig_len = ar.len();
    strip_lto_sections(&mut ar);
    assert_eq!(ar.len(), orig_len);
    assert!(probe(&ar), "sanitized ELF64 archive must match host arch");
    for forbidden in [b".llvmbc".as_slice(), b".llvmcmd", b".gnu.lto_"] {
        assert!(
            !ar.windows(forbidden.len()).any(|w| w == forbidden),
            "forbidden ELF section {:?} remained",
            std::str::from_utf8(forbidden)
        );
    }
    assert!(
        !ar.windows(4).any(|w| w == 0x6fff4c0c_u32.to_le_bytes()),
        "SHT_LLVM_LTO section type must be rewritten"
    );
    assert!(ar.windows(5).any(|w| w == b".text"), ".text must remain intact");
}

#[test]
fn strip_lto_sections_coff_archive_and_install_staticlib() {
    let machine: u16 = match std::env::consts::ARCH {
        "x86_64" => 0x8664,
        "aarch64" => 0xAA64,
        other => panic!("unsupported test host {other}"),
    };
    let nsects = 4u16;
    let sym_ptr = (20 + (nsects as usize) * 40) as u32;
    let nsyms = 2u32;
    let strtab_bytes = b".llvm.lto\0";
    let strtab_size = (4 + strtab_bytes.len()) as u32;
    let mut coff = vec![0u8; (sym_ptr as usize) + (nsyms as usize) * 18 + (strtab_size as usize)];
    coff[0..2].copy_from_slice(&machine.to_le_bytes());
    coff[2..4].copy_from_slice(&nsects.to_le_bytes());
    coff[8..12].copy_from_slice(&sym_ptr.to_le_bytes());
    coff[12..16].copy_from_slice(&nsyms.to_le_bytes());

    let write_sect = |buf: &mut [u8], idx: usize, name: &[u8; 8], size: u32, ptr: u32, chars: u32| {
        let base = 20 + idx * 40;
        buf[base..base + 8].copy_from_slice(name);
        buf[base + 16..base + 20].copy_from_slice(&size.to_le_bytes());
        buf[base + 20..base + 24].copy_from_slice(&ptr.to_le_bytes());
        buf[base + 36..base + 40].copy_from_slice(&chars.to_le_bytes());
    };
    write_sect(&mut coff, 0, b".text\0\0\0", 32, 200, 0x60000020);
    write_sect(&mut coff, 1, b".llvmbc\0", 512, 232, 0x40000800);
    write_sect(&mut coff, 2, b".llvmcmd", 64, 744, 0x40000800);
    write_sect(&mut coff, 3, b"/4\0\0\0\0\0\0", 128, 808, 0x40000800);

    // Symbol 0: .text, Symbol 1: .llvmbc
    let s0 = sym_ptr as usize;
    coff[s0..s0 + 8].copy_from_slice(b".text\0\0\0");
    let s1 = s0 + 18;
    coff[s1..s1 + 8].copy_from_slice(b".llvmbc\0");
    let strtab_off = s1 + 18;
    coff[strtab_off..strtab_off + 4].copy_from_slice(&strtab_size.to_le_bytes());
    coff[strtab_off + 4..strtab_off + 4 + strtab_bytes.len()].copy_from_slice(strtab_bytes);

    let ar = archive(&[member("/", &[0; 4]), member("hyrt.obj/", &coff)]);
    let dir = std::env::temp_dir().join(format!("hyb-ltocoff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("hyrt.lib");
    let libdir = dir.join("lib");
    std::fs::write(&src, &ar).unwrap();

    install_staticlib(&src, &libdir).unwrap();
    let installed = std::fs::read(libdir.join("libhyrt.a")).unwrap();
    assert_eq!(installed.len(), ar.len());
    assert!(lib_arch_matches(&libdir.join("libhyrt.a")));
    for forbidden in [b".llvmbc".as_slice(), b".llvmcmd", b".llvm.lto"] {
        assert!(
            !installed.windows(forbidden.len()).any(|w| w == forbidden),
            "forbidden COFF token {:?} remained after install_staticlib",
            std::str::from_utf8(forbidden)
        );
    }
    assert!(installed.windows(5).any(|w| w == b".text"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn real_staticlib_is_sanitized_of_disallowed_lto_sections_when_present() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../runtime/rust/hyrt/target/release/libhyrt.a");
    if !p.exists() {
        return;
    }
    sanitize_staticlib_file(&p).expect("sanitize_staticlib_file must succeed on real libhyrt.a");
    assert!(lib_arch_matches(&p), "sanitized libhyrt.a must match host arch");
    let bytes = std::fs::read(&p).unwrap();
    for forbidden in [
        b".llvmbc".as_slice(),
        b".llvmcmd",
        b"__LLVM",
        b"__GNU_LTO",
    ] {
        assert!(
            !bytes.windows(forbidden.len()).any(|w| w == forbidden),
            "real libhyrt.a still contains {:?}",
            std::str::from_utf8(forbidden)
        );
    }
}

#[path = "windows_version.rs"]
mod windows_version;

fn main() {
    // slint-build bundles gettext catalogs into the executable. Cargo does
    // not discover those inputs itself, so text edits must invalidate the build.
    println!("cargo:rerun-if-changed=native/translations");
    println!("cargo:rerun-if-changed=native/windows_version.rs");
    println!("cargo:rerun-if-changed=native/THIRD_PARTY_NOTICES.md");
    println!("cargo:rerun-if-changed=native/packaging/LGPL-3.0.txt");
    println!("cargo:rerun-if-changed=native/packaging/SLINT-LICENSE.md");
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("native/translations")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("native/ui/panel.slint", config).expect("compile ONIX panel");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let original_filename = if std::env::var_os("CARGO_FEATURE_PORTABLE").is_some() {
        "ONIX-DAC-Control-windows-x64-portable.exe"
    } else {
        "onix-xi1-pc.exe"
    };
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("native/ui/icon.ico")
        .set("OriginalFilename", original_filename);
    if std::env::var_os("CARGO_FEATURE_PORTABLE").is_some() {
        let files = [
            (201, "LICENSE".to_string()),
            (202, "native/THIRD_PARTY_NOTICES.md".to_string()),
            (203, "native/packaging/LGPL-3.0.txt".to_string()),
            (204, "native/packaging/SLINT-LICENSE.md".to_string()),
        ];
        for (id, path) in files {
            let path = std::fs::canonicalize(path)
                .expect("portable license file")
                .to_string_lossy()
                .replace('\\', "/");
            resource.append_rc_content(&format!("{id} RCDATA \"{path}\""));
        }
        if let Some(path) = std::env::var_os("ONIX_GENERATED_LICENSES") {
            let path = std::fs::canonicalize(path)
                .expect("generated dependency licenses")
                .to_string_lossy()
                .replace('\\', "/");
            resource.append_rc_content(&format!("205 RCDATA \"{path}\""));
        }
    }
    if resource.compile().is_ok() {
        return;
    }
    // GNU builds here have no windres. Link a COFF .rsrc so Task Manager
    // reads FileDescription instead of the exe filename.
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("onix-version.o");
    write_rsrc_coff(
        &out,
        &windows_version::version_info("ONIX DAC Control", original_filename),
    )
    .expect("write version resource object");
    println!("cargo:rustc-link-arg={}", out.display());
}

fn write_rsrc_coff(path: &std::path::Path, version: &[u8]) -> std::io::Result<()> {
    fn put16(buf: &mut Vec<u8>, value: u16) {
        buf.extend_from_slice(&value.to_le_bytes());
    }
    fn put32(buf: &mut Vec<u8>, value: u32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    let blob_at = 0x58u32;
    let mut rsrc = vec![0u8; blob_at as usize];
    rsrc[14..16].copy_from_slice(&1u16.to_le_bytes());
    rsrc[16..20].copy_from_slice(&16u32.to_le_bytes());
    rsrc[20..24].copy_from_slice(&0x8000_0018u32.to_le_bytes());
    rsrc[0x18 + 14..0x18 + 16].copy_from_slice(&1u16.to_le_bytes());
    rsrc[0x28..0x2C].copy_from_slice(&1u32.to_le_bytes());
    rsrc[0x2C..0x30].copy_from_slice(&0x8000_0030u32.to_le_bytes());
    rsrc[0x30 + 14..0x30 + 16].copy_from_slice(&1u16.to_le_bytes());
    rsrc[0x40..0x44].copy_from_slice(&0x0409u32.to_le_bytes());
    rsrc[0x44..0x48].copy_from_slice(&0x48u32.to_le_bytes());
    rsrc[0x48..0x4C].copy_from_slice(&0u32.to_le_bytes());
    rsrc[0x4C..0x50].copy_from_slice(&(version.len() as u32).to_le_bytes());
    rsrc.extend_from_slice(version);

    let data_off = 60u32;
    let reloc_off = data_off + rsrc.len() as u32;
    let sym_off = reloc_off + 10;
    let mut obj = Vec::new();
    put16(&mut obj, 0x8664);
    put16(&mut obj, 1);
    put32(&mut obj, 0);
    put32(&mut obj, sym_off);
    put32(&mut obj, 1);
    put16(&mut obj, 0);
    put16(&mut obj, 0);
    obj.extend_from_slice(b".rsrc\0\0\0");
    put32(&mut obj, rsrc.len() as u32);
    put32(&mut obj, 0);
    put32(&mut obj, rsrc.len() as u32);
    put32(&mut obj, data_off);
    put32(&mut obj, reloc_off);
    put32(&mut obj, 0);
    put16(&mut obj, 1);
    put16(&mut obj, 0);
    put32(&mut obj, 0x4000_0040);
    obj.extend_from_slice(&rsrc);
    put32(&mut obj, 0x48);
    put32(&mut obj, 0);
    put16(&mut obj, 3);
    obj.extend_from_slice(b".rsrc\0\0\0");
    put32(&mut obj, blob_at);
    put16(&mut obj, 1);
    put16(&mut obj, 0);
    obj.push(3);
    obj.push(0);
    put32(&mut obj, 4);
    std::fs::write(path, obj)
}

//! Dependency-free Windows VERSIONINFO for the build script.

pub fn version_info(file_description: &str, original_filename: &str) -> Vec<u8> {
    fn pad4(buf: &mut Vec<u8>) {
        while !buf.len().is_multiple_of(4) {
            buf.push(0);
        }
    }
    fn put16(buf: &mut Vec<u8>, value: u16) {
        buf.extend_from_slice(&value.to_le_bytes());
    }
    fn put32(buf: &mut Vec<u8>, value: u32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }
    fn put_wide(buf: &mut Vec<u8>, text: &str) {
        for unit in text.encode_utf16().chain(std::iter::once(0)) {
            put16(buf, unit);
        }
    }
    fn patch_len(buf: &mut [u8], at: usize) {
        let len = (buf.len() - at) as u16;
        buf[at..at + 2].copy_from_slice(&len.to_le_bytes());
    }
    fn string_pair(buf: &mut Vec<u8>, key: &str, value: &str) {
        pad4(buf);
        let start = buf.len();
        put16(buf, 0);
        put16(buf, (value.encode_utf16().count() + 1) as u16);
        put16(buf, 1);
        put_wide(buf, key);
        pad4(buf);
        put_wide(buf, value);
        pad4(buf);
        patch_len(buf, start);
    }

    let version = env!("CARGO_PKG_VERSION");
    let mut numbers = version
        .split(['.', '-'])
        .take(4)
        .map(|part| part.parse::<u16>().unwrap_or(0));
    let major = numbers.next().unwrap_or(0);
    let minor = numbers.next().unwrap_or(0);
    let patch = numbers.next().unwrap_or(0);
    let build = numbers.next().unwrap_or(0);
    let version_ms = (u32::from(major) << 16) | u32::from(minor);
    let version_ls = (u32::from(patch) << 16) | u32::from(build);

    let mut buf = Vec::new();
    let root = buf.len();
    put16(&mut buf, 0);
    put16(&mut buf, 52);
    put16(&mut buf, 0);
    put_wide(&mut buf, "VS_VERSION_INFO");
    pad4(&mut buf);
    put32(&mut buf, 0xFEEF_04BD);
    put32(&mut buf, 0x0001_0000);
    put32(&mut buf, version_ms);
    put32(&mut buf, version_ls);
    put32(&mut buf, version_ms);
    put32(&mut buf, version_ls);
    put32(&mut buf, 0x3F);
    put32(&mut buf, 0);
    put32(&mut buf, 0x0004_0004);
    put32(&mut buf, 1);
    put32(&mut buf, 0);
    put32(&mut buf, 0);
    put32(&mut buf, 0);
    pad4(&mut buf);

    let strings = buf.len();
    put16(&mut buf, 0);
    put16(&mut buf, 0);
    put16(&mut buf, 1);
    put_wide(&mut buf, "StringFileInfo");
    pad4(&mut buf);
    let table = buf.len();
    put16(&mut buf, 0);
    put16(&mut buf, 0);
    put16(&mut buf, 1);
    put_wide(&mut buf, "040904B0");
    string_pair(&mut buf, "FileDescription", file_description);
    string_pair(&mut buf, "ProductName", "ONIX DAC Control");
    string_pair(&mut buf, "OriginalFilename", original_filename);
    string_pair(&mut buf, "InternalName", original_filename);
    string_pair(&mut buf, "FileVersion", version);
    string_pair(&mut buf, "ProductVersion", version);
    patch_len(&mut buf, table);
    patch_len(&mut buf, strings);

    pad4(&mut buf);
    let vars = buf.len();
    put16(&mut buf, 0);
    put16(&mut buf, 0);
    put16(&mut buf, 1);
    put_wide(&mut buf, "VarFileInfo");
    pad4(&mut buf);
    let translation = buf.len();
    put16(&mut buf, 0);
    put16(&mut buf, 4);
    put16(&mut buf, 0);
    put_wide(&mut buf, "Translation");
    pad4(&mut buf);
    put16(&mut buf, 0x0409);
    put16(&mut buf, 0x04B0);
    patch_len(&mut buf, translation);
    patch_len(&mut buf, vars);
    patch_len(&mut buf, root);
    buf
}

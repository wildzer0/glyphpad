//! The executable's own icon (SPEC §10.1): Windows takes it from a resource
//! compiled into the binary, drawn here from the same code the window and the
//! tray use, so the two can never drift apart.
//!
//! Elsewhere there is nothing to do: the other platforms carry the icon in
//! their own way when their backend arrives.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    windows_icon();
}

/// Writes a multi-size `.ico` from [`glyphpad_core::icon::icon_rgba`] and
/// compiles it into the executable.
#[cfg(windows)]
fn windows_icon() {
    use std::io::Write as _;

    const SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let images: Vec<(u32, Vec<u8>)> = SIZES.iter().map(|&size| (size, dib(size))).collect();

    // ICONDIR, then one ICONDIRENTRY per image, then the images.
    let mut ico = Vec::new();
    ico.extend_from_slice(&0u16.to_le_bytes()); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in &images {
        // 256 is written as 0: the field is a byte.
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        ico.extend_from_slice(&[dim, dim, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &images {
        ico.extend_from_slice(data);
    }
    std::fs::write(out.join("glyphpad.ico"), &ico).expect("cannot write the icon");

    let rc = out.join("glyphpad.rc");
    let mut file = std::fs::File::create(&rc).expect("cannot write the resource script");
    writeln!(file, "1 ICON \"glyphpad.ico\"").expect("cannot write the resource script");
    drop(file);
    // `embed-resource` finds `rc.exe` in the MSVC environment the build runs
    // in, which is the one the rest of the build uses. A failed compilation
    // stops the build: an executable without its icon is a mistake, not a
    // preference.
    if let Err(e) = embed_resource::compile(&rc, &out).manifest_optional() {
        panic!("cannot compile the icon resource: {e}");
    }
}

/// One image as the DIB an `.ico` holds: a `BITMAPINFOHEADER` whose height
/// covers the colours and the mask, then BGRA rows bottom-up, then the 1-bit
/// mask that says which pixels are transparent.
#[cfg(windows)]
fn dib(size: u32) -> Vec<u8> {
    let rgba = glyphpad_core::icon::icon_rgba(size, false);
    let mut out = Vec::with_capacity((size * size * 5) as usize + 64);
    // BITMAPINFOHEADER.
    out.extend_from_slice(&40u32.to_le_bytes()); // size of the header
    out.extend_from_slice(&(size as i32).to_le_bytes()); // width
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // colours + mask
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB, no compression
    out.extend_from_slice(&(size * size * 4).to_le_bytes()); // size of the colours
    out.extend_from_slice(&[0u8; 16]); // resolution and palette: unused
    // Colours, bottom row first, BGRA: the icon has no background of its own,
    // so the alpha channel is what matters.
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    // The mask: one bit per pixel, 1 where transparent, rows padded to 32 bits.
    let row_bytes = size.div_ceil(32) * 4;
    for y in (0..size).rev() {
        let mut row = vec![0u8; row_bytes as usize];
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            if rgba[i + 3] == 0 {
                row[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        out.extend_from_slice(&row);
    }
    out
}

//! Minimal PNG encoder, enough for screenshots: RGBA 8 bits, no compression (the image data is
//! stored in "stored" deflate blocks). Files are large but the code needs no dependency.
//!
//! A PNG file is a signature followed by chunks (length, type, data, CRC-32): IHDR (size and
//! pixel format), IDAT (the pixels, zlib-wrapped, each row prefixed by a filter byte), IEND.

/// Encodes `rgba` (`width × height × 4` bytes, rows top to bottom) as a PNG file.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), (width * height * 4) as usize, "pixel count");
    let mut out = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    // 8 bits per channel, colour type 6 (RGBA), deflate, standard filters, no interlace.
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_chunk(&mut out, b"IHDR", &header);

    // Raw data: each row starts with filter type 0 (none).
    let row = (width * 4) as usize;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgba.chunks(row) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    write_chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    write_chunk(&mut out, b"IEND", &[]);
    out
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    // The CRC covers the type and the data, not the length.
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// zlib stream made of uncompressed deflate blocks (at most 65 535 bytes each).
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    // Header: deflate, 32 KiB window, no dictionary, check bits making it a multiple of 31.
    let mut out = vec![0x78, 0x01];
    let mut blocks = data.chunks(u16::MAX as usize).peekable();
    if blocks.peek().is_none() {
        // An empty stream still needs one (final, empty) block.
        out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    while let Some(block) = blocks.next() {
        let last = blocks.peek().is_none();
        out.push(last as u8); // BFINAL bit, BTYPE = 00 (stored)
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_reference_values() {
        // Reference values from the PNG and zlib specifications' examples.
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
    }

    #[test]
    fn png_has_signature_header_and_end() {
        let png = encode_png(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]);
        assert_eq!(
            &png[..8],
            &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']
        );
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..20], &2u32.to_be_bytes());
        // Ends with an empty IEND chunk and its CRC.
        assert_eq!(
            &png[png.len() - 12..],
            &[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82]
        );
    }

    #[test]
    fn large_images_are_split_in_stored_blocks() {
        let rgba = vec![7u8; 200 * 200 * 4]; // 160 800 bytes of rows: 3 blocks
        let zlib = zlib_stored(&rgba);
        assert_eq!(zlib.len(), 2 + 3 * 5 + rgba.len() + 4);
    }
}

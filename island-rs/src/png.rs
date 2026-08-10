use std::{
    fs::File,
    io::{self, Write},
    path::Path,
};

const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Writes an 8-bit RGB PNG image using uncompressed DEFLATE blocks.
///
/// # Errors
///
/// Returns an error when the RGB byte count does not match the dimensions or
/// when the destination cannot be created or written.
pub fn write_png(path: impl AsRef<Path>, width: u32, height: u32, rgb: &[u8]) -> io::Result<()> {
    let expected = width as usize * height as usize * 3;
    if rgb.len() != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("expected {expected} RGB bytes, received {}", rgb.len()),
        ));
    }

    let mut file = File::create(path)?;
    file.write_all(SIGNATURE)?;
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    write_chunk(&mut file, *b"IHDR", &header)?;

    let stride = width as usize * 3;
    let mut scanlines = Vec::with_capacity((stride + 1) * height as usize);
    for row in rgb.chunks_exact(stride) {
        scanlines.push(0);
        scanlines.extend_from_slice(row);
    }
    let compressed = zlib_uncompressed(&scanlines);
    write_chunk(&mut file, *b"IDAT", &compressed)?;
    write_chunk(&mut file, *b"IEND", &[])
}

fn write_chunk(writer: &mut impl Write, kind: [u8; 4], data: &[u8]) -> io::Result<()> {
    let length = u32::try_from(data.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "PNG chunk exceeds 4 GiB"))?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&kind)?;
    writer.write_all(data)?;
    let mut crc_input = Vec::with_capacity(kind.len() + data.len());
    crc_input.extend_from_slice(&kind);
    crc_input.extend_from_slice(data);
    writer.write_all(&crc32(&crc_input).to_be_bytes())
}

fn zlib_uncompressed(data: &[u8]) -> Vec<u8> {
    let block_count = data.len().div_ceil(u16::MAX as usize);
    let mut out = Vec::with_capacity(data.len() + block_count * 5 + 6);
    out.extend_from_slice(&[0x78, 0x01]);
    for (index, block) in data.chunks(u16::MAX as usize).enumerate() {
        out.push(u8::from(index + 1 == block_count));
        let length = u16::try_from(block.len()).expect("chunks are bounded to u16::MAX");
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&(!length).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::{crc32, zlib_uncompressed};

    #[test]
    fn known_crc() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn zlib_has_valid_wrapper() {
        let encoded = zlib_uncompressed(b"island");
        assert_eq!(&encoded[..2], &[0x78, 0x01]);
        assert_eq!(&encoded[encoded.len() - 4..], &[0x08, 0xce, 0x02, 0x7c]);
    }
}

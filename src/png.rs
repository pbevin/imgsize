use super::ImageMetadata;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum PngDecodingError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("IHDR chunk missing from PNG")]
    MissingIHDR,

    #[error("Invalid IHDR chunk length: {0}")]
    InvalidIHDRLength(u32),

    #[error("Invalid chunk CRC")]
    InvalidChunkCrc,
}

/// Read PNG data, and return its dimensions and any comments found.
pub fn read_png_data<T: AsRef<[u8]>>(buf: T) -> Result<ImageMetadata, PngDecodingError> {
    let buf = buf.as_ref();
    let mut comments: Vec<Vec<u8>> = Vec::new();
    let mut dimensions: Option<(u32, u32)> = None;

    let mut pos = 8;
    while pos < buf.len() {
        let chunk_length = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
        pos += 4;
        let chunk_type = [buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]];
        pos += 4;
        let chunk_data = &buf[pos..][..chunk_length as usize];
        pos += chunk_length as usize;
        let chunk_crc = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
        pos += 4;

        let mut crc = crc32fast::Hasher::new();
        crc.update(&chunk_type);
        crc.update(chunk_data);
        if crc.finalize() != chunk_crc {
            return Err(PngDecodingError::InvalidChunkCrc);
        }

        match &chunk_type {
            // IHDR: Image Header
            b"IHDR" => {
                if chunk_length != 13 {
                    return Err(PngDecodingError::InvalidIHDRLength(chunk_length));
                }
                let width = u32::from_be_bytes([
                    chunk_data[0],
                    chunk_data[1],
                    chunk_data[2],
                    chunk_data[3],
                ]);
                let height = u32::from_be_bytes([
                    chunk_data[4],
                    chunk_data[5],
                    chunk_data[6],
                    chunk_data[7],
                ]);
                dimensions = Some((width, height));
            }
            // tEXt: Textual Data
            b"tEXt" => {
                let mut parts = chunk_data.splitn(2, |&b| b == 0);
                let keyword = parts.next().unwrap();
                let text = parts.next().unwrap();
                if keyword == b"comment" {
                    comments.push(text.to_vec());
                }
            }
            // IEND: Image Trailer
            b"IEND" => {
                break;
            }
            _ => {
                // Ignore other chunks
            }
        }
    }

    let (width, height) = dimensions.ok_or(PngDecodingError::MissingIHDR)?;
    Ok(ImageMetadata {
        width,
        height,
        comments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn test_read_png_data_valid() {
        let metadata = read_png_data(sample_image()).unwrap();
        assert_eq!(metadata.width, 400);
        assert_eq!(metadata.height, 224);
        assert_eq!(metadata.comments.len(), 1);
        let comments = metadata
            .comments
            .iter()
            .map(|c| String::from_utf8_lossy(c))
            .collect::<Vec<_>>();
        assert_eq!(comments, vec!["Abstract watercolors"]);
    }

    #[test]
    fn test_read_png_data_invalid_crc() {
        let mut data = sample_image();
        // Corrupt the CRC of the IHDR chunk
        data[31] = data[31].wrapping_add(1);
        let result = read_png_data(&data);

        let err = result.unwrap_err();
        assert_matches!(err, PngDecodingError::InvalidChunkCrc);
    }

    #[test]
    fn test_read_png_data_missing_header() {
        let mut data = sample_image();

        // Remove the IHDR chunk:
        //  - the first 8 bytes are the PNG header,
        //  - the next 4 bytes (8..12) are the length of the IHDR chunk,
        //  - the next 4 bytes (12..16) are the type of the IHDR chunk,
        //  - the next 13 bytes (16..29) are the data of the IHDR chunk,
        //  - the next 4 bytes (29..33) are the CRC of the IHDR chunk.
        // 8 + 4 + 4 + 13 + 4 = 33
        data.splice(8..33, []);
        let result = read_png_data(&data);
        let err = result.unwrap_err();
        assert_matches!(err, PngDecodingError::MissingIHDR);
    }

    #[test]
    fn test_read_png_data_invalid_chunk_length() {
        let mut data = sample_image();

        // Create a replacement IHDR chunk with length 14 instead of 13. The
        // length, type, and CRC are all 4 bytes, so we need to add 12 bytes to
        // the 14 bytes of data to make the chunk 26 bytes long in total.
        let mut new_ihdr = Vec::new();
        new_ihdr.extend_from_slice(&14u32.to_be_bytes());
        new_ihdr.extend_from_slice(b"IHDR");
        new_ihdr.extend_from_slice(&data[16..30]); // 14 bytes of data

        // Calculate the CRC of the new IHDR chunk
        let mut crc = crc32fast::Hasher::new();
        crc.update(&new_ihdr[4..]);
        let crc = crc.finalize();
        new_ihdr.extend_from_slice(&crc.to_be_bytes());

        // Splice the new IHDR chunk into the PNG data, replacing the old one.
        data.splice(8..33, new_ihdr);

        // Decoding should now fail because the IHDR chunk has an invalid length.
        let result = read_png_data(&data);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_matches!(err, PngDecodingError::InvalidIHDRLength(14));
    }

    fn sample_image() -> Vec<u8> {
        std::fs::read("src/watercolors.png").unwrap()
    }
}

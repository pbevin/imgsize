use super::ImageMetadata;

/// Read JPEG data, and return its dimensions and any comments found.
pub fn read_jpeg_data(buf: &[u8]) -> Result<ImageMetadata, JpegDecodingError> {
    let mut comments = vec![];
    let mut dimensions = None;

    // The first 2 bytes are the SOI marker, which we have already looked at.
    let mut position = 2;

    // Loop over the segments in the JPEG data.
    loop {
        // Position should point to the first byte of a marker, which is
        // 0xff. If not, we have lost sync: warn and try to resync by
        // skipping bytes until we find it.
        if position < buf.len() && buf[position] != 0xff {
            if let Some(next_pos) = memchr::memchr(0xff, &buf[position..]) {
                log::warn!("Lost sync in JPEG data, trying to resync");
                position += next_pos;
            } else {
                // No marker found, give up.
                return Err(JpegDecodingError::NoSofMarker { position, comments });
            }
        }
        let marker = u16::from_be_bytes([buf[position], buf[position + 1]]);
        if marker == 0xffd8 {
            // SOI
            continue;
        } else if marker == 0xffd9 || marker == 0xffda {
            // 0xffd9 = EOI (end marker)
            // 0xffda = SOS (start of scan)
            // In both cases, we now know we've seen all the metadata
            if let Some((w, h)) = dimensions {
                return Ok(ImageMetadata {
                    width: w,
                    height: h,
                    comments,
                });
            } else {
                return Err(JpegDecodingError::NoSofMarker { position, comments });
            }
        } else if marker < 0xff01 || marker == 0xffff {
            return Err(JpegDecodingError::InvalidFrameMarker {
                word: marker,
                position,
            });
        }

        // By now, we know that there's a segment to read, and
        // it starts with a length field.
        let len: usize = u16::from_be_bytes([buf[position + 2], buf[position + 3]]).into();
        if is_sof_marker(marker) {
            let sof = read_sof(&buf[position + 4..][..len - 2])?;
            let (w, h) = sof;
            dimensions.replace((w.into(), h.into()));
        } else if marker == 0xfffe {
            // COM marker: read the comment and add it to the list.
            let mut comment_buf = vec![0; len - 2];
            comment_buf.copy_from_slice(&buf[position + 4..][..len - 2]);
            comments.push(comment_buf);
        }
        position += len;
    }
}

/// Returns true if the byte is the low byte of an SOF marker.
fn is_sof_marker(word: u16) -> bool {
    // The range 0xffc0 to 0xffcf is mostly SOFnn markers, except for
    //  0xffc4 (DHT)
    //  0xffc8 (JPG)
    //  0xffcc (DAC)
    (0xffc0..=0xffcf).contains(&word) && word != 0xffc4 && word != 0xffc8 && word != 0xffcc
}

/// Read the width and height from an SOF segment. The read head should be
/// positioned after the length field.
fn read_sof(buf: &[u8]) -> Result<(u16, u16), JpegDecodingError> {
    if buf.len() < 5 {
        return Err(JpegDecodingError::SofDataTooShort {
            position: buf.len(),
        });
    }
    let height = u16::from_be_bytes([buf[1], buf[2]]);
    let width = u16::from_be_bytes([buf[3], buf[4]]);

    Ok((width, height))
}

#[derive(Debug, thiserror::Error)]
pub enum JpegDecodingError {
    #[error("No SOI marker found")]
    NoSoiMarker,

    #[error("No SOF marker found")]
    NoSofMarker {
        position: usize,
        comments: Vec<Vec<u8>>,
    },

    #[error("SOF data is too short")]
    SofDataTooShort { position: usize },

    #[error("Invalid frame marker: 0x{word:04x} at position {position} (0x{position:04x})")]
    InvalidFrameMarker { word: u16, position: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sof_marker() {
        assert!(!is_sof_marker(0xff42), "Bad marker 0xff42");
        assert!(is_sof_marker(0xffc0), "SOF0");
        assert!(is_sof_marker(0xffc1), "SOF1");
        assert!(is_sof_marker(0xffc2), "SOF2");
        assert!(is_sof_marker(0xffc3), "SOF3");
        assert!(!is_sof_marker(0xffc4), "DHT");
        assert!(is_sof_marker(0xffc5), "SOF5");
        assert!(is_sof_marker(0xffc6), "SOF6");
        assert!(is_sof_marker(0xffc7), "SOF7");
        assert!(!is_sof_marker(0xffc8), "JPG");
        assert!(is_sof_marker(0xffc9), "SOF9");
        assert!(is_sof_marker(0xffca), "SOF10");
        assert!(is_sof_marker(0xffcb), "SOF11");
        assert!(!is_sof_marker(0xffcc), "DAC");
        assert!(is_sof_marker(0xffcd), "SOF13");
        assert!(is_sof_marker(0xffce), "SOF14");
        assert!(is_sof_marker(0xffcf), "SOF15");
        assert!(!is_sof_marker(0xffff), "Bad marker FFFF");
    }

    #[test]
    fn test_read_sof() {
        let data = sample_image();

        // For our sample image, the SOF0 marker is at 0xc4:
        // 000000c0: 1414 1414 ffc0 0011 0801 5502 0003 0111  ..........U.....
        // 000000d0: 0002 1101 0311 01ff c400 1d00 0001 0501  ................
        //
        // This decodes as follows:
        //
        // 0x08: 8 bits/sample
        // 0x0155: 341 pixels high
        // 0x0200: 512 pixels wide
        // 0x03: 3 components
        // 0x01: component 1 uses table 1
        // 0x1101: component 1 is 17x17 pixels
        // 0x03: component 2 uses table 3
        // 0x1101: component 2 is 17x17 pixels
        // 0x03: component 3 uses table 3

        let sof0_pos = 0xc4;
        let sof0_len = 0x11;
        let sof_data = &data[sof0_pos + 4..][..sof0_len - 2].to_owned();
        assert_eq!(sof_data.len(), 15);

        assert_eq!(read_sof(sof_data).unwrap(), (512, 341));

        // Test some other sizes. The read_sof() function doesn't read past the
        // width field, so we can just omit the rest of the data.
        assert_eq!(read_sof(&[0x08, 0x00, 0x01, 0x00, 0x01]).unwrap(), (1, 1));
        assert_eq!(read_sof(&[0x08, 0x00, 0x02, 0x00, 0x01]).unwrap(), (1, 2));
        assert_eq!(read_sof(&[0x08, 0x00, 0x01, 0x00, 0x02]).unwrap(), (2, 1));
        assert_eq!(read_sof(&[0x08, 0x00, 0x02, 0x00, 0x02]).unwrap(), (2, 2));
        assert_eq!(
            read_sof(&[0x08, 0x08, 0x00, 0x03, 0xe8]).unwrap(),
            (1000, 2048)
        );
    }

    fn sample_image() -> Vec<u8> {
        std::fs::read("src/buttercups.jpg").unwrap()
    }
}

use super::ImageMetadata;

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

    #[error("Invalid JPEG segment length: {0:?}")]
    InvalidSegmentLength(usize),
}

/// Read JPEG data, and return its dimensions and any comments found.
pub fn read_jpeg_data(buf: &[u8]) -> Result<ImageMetadata, JpegDecodingError> {
    let mut context = JpegContext {
        buf,
        position: 2, // The first 2 bytes are the SOI marker, which we have already looked at.
        comments: vec![],
        dimensions: None,
    };

    // Loop over the segments in the JPEG data.
    while let Some(segment) = context.read_segment()? {
        let marker = segment.marker;

        // What we do next depends on the marker.
        // - It might be an invalid marker, in which case we return an error.
        // - It might be a marker we don't care about, in which case we ignore
        //   it.
        // - It might be a marker we care about, in which case we read the data.
        // - It might be a marker that indicates the end of the metadata, in
        //   which case we stop.

        if marker < 0xff01 || marker == 0xffff {
            return Err(JpegDecodingError::InvalidFrameMarker {
                word: marker,
                position: context.position,
            });
        }

        // End of metadata?
        if marker == 0xffd9 || marker == 0xffda {
            // 0xffd9 = EOI (end marker)
            // 0xffda = SOS (start of scan)
            // In both cases, we now know we've seen all the metadata we're going to see.
            break;
        }

        if segment.is_sof() {
            // SOFx marker: read the dimensions and add them to the context.
            let (w, h) = segment.read_sof()?;
            context.dimensions.replace((w.into(), h.into()));
        } else if segment.is_com() {
            // COM marker: read the comment and add it to the list.
            let comment = segment.into_data();
            context.comments.push(comment);
        }
    }

    // We're done. Try to convert the context into an ImageMetadata. (This will
    // fail if we didn't find a SOF marker.)
    context.try_into()
}
struct JpegContext<'a> {
    buf: &'a [u8],
    position: usize,
    comments: Vec<Vec<u8>>,
    dimensions: Option<(u32, u32)>,
}

struct JpegSegment<'a> {
    position: usize,
    marker: u16,
    data: &'a [u8],
}

impl<'a> TryFrom<JpegContext<'a>> for ImageMetadata {
    type Error = JpegDecodingError;

    fn try_from(jpeg: JpegContext) -> Result<Self, JpegDecodingError> {
        if let Some((width, height)) = jpeg.dimensions {
            Ok(ImageMetadata {
                width,
                height,
                comments: jpeg.comments,
            })
        } else {
            Err(JpegDecodingError::NoSofMarker {
                position: jpeg.position,
                comments: jpeg.comments,
            })
        }
    }
}

impl<'a> JpegContext<'a> {
    /// Read a segment from the JPEG data. Returns the marker, and the data
    /// following the marker.
    ///
    /// Returns `None` if the end of the JPEG data has been reached.
    pub fn read_segment(&mut self) -> Result<Option<JpegSegment>, JpegDecodingError> {
        // If the current byte is not 0xff, resync to the next marker.
        if self.buf.get(self.position) != Some(&0xff) {
            self.resync();
        }
        // If the current byte is still not 0xff, we've reached the end of the data.
        if self.buf.get(self.position) != Some(&0xff) {
            return Ok(None);
        }
        // If the next byte is 0xd9 (EOI), we've reached the end of the image data.
        if self.buf.get(self.position + 1) == Some(&0xd9) {
            return Ok(None);
        }
        // If we've reached the end of the buffer, return None.
        if self.position + 1 >= self.buf.len() {
            return Ok(None);
        }

        // Read the marker and length of the segment.
        let original_position = self.position;
        let (marker, len) = self.read_marker()?;
        // Check that the length is valid.
        if len < 2 || self.position + len > self.buf.len() {
            return Err(JpegDecodingError::InvalidSegmentLength(len));
        }

        // Extract the data from the segment.
        let data = &self.buf[self.position..self.position + len - 2];
        self.position += len - 2;
        Ok(Some(JpegSegment {
            position: original_position,
            marker,
            data,
        }))
    }

    /// Read a marker from the JPEG data.
    /// Returns the marker, and the length of the data following the marker.
    fn read_marker(&mut self) -> Result<(u16, usize), JpegDecodingError> {
        let marker = u16::from_be_bytes([self.buf[self.position], self.buf[self.position + 1]]);
        if marker == 0xffd8 || marker == 0xffd9 {
            // SOI or EOI
            self.position += 2;
            return Ok((marker, 0));
        }
        let len = u16::from_be_bytes([self.buf[self.position + 2], self.buf[self.position + 3]]);
        self.position += 4;
        Ok((marker, len.into()))
    }

    /// Resync to the next marker.
    /// This is used to recover from errors in the JPEG data.
    fn resync(&mut self) {
        while self.position + 1 < self.buf.len() {
            if self.buf[self.position] == 0xff && self.buf[self.position + 1] > 0x01 {
                // We've found a marker, so we're done.
                break;
            }

            // Not a marker, so search for the next 0xff and keep looking.
            log::warn!("Resyncing to next marker from position {}", self.position);
            if let Some(pos) = memchr::memchr(0xff, &self.buf[self.position + 1..]) {
                self.position += pos + 1;
            } else {
                self.position = self.buf.len();
            }
        }
    }
}

impl<'a> JpegSegment<'a> {
    /// Returns true if this segment is a SOF (Start Of Frame) marker.
    fn is_sof(&self) -> bool {
        self.marker >= 0xffc0
            && self.marker <= 0xffcf
            && self.marker != 0xffc4
            && self.marker != 0xffc8
    }

    fn is_com(&self) -> bool {
        self.marker == 0xfffe
    }

    /// Read the dimensions from a SOF (Start Of Frame) marker.
    fn read_sof(&self) -> Result<(u16, u16), JpegDecodingError> {
        if self.data.len() < 5 {
            return Err(JpegDecodingError::SofDataTooShort {
                position: self.position,
            });
        }
        let height = u16::from_be_bytes([self.data[1], self.data[2]]);
        let width = u16::from_be_bytes([self.data[3], self.data[4]]);
        Ok((width, height))
    }

    fn into_data(self) -> Vec<u8> {
        self.data.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sof_marker() {
        let mut segment = JpegSegment {
            position: 0,
            marker: 0xffc0,
            data: &[],
        };

        segment.marker = 0xffc0;
        assert!(segment.is_sof(), "SOF0");
        segment.marker = 0xffc1;
        assert!(segment.is_sof(), "SOF1");
        segment.marker = 0xffc2;
        assert!(segment.is_sof(), "SOF2");
        segment.marker = 0xffc3;
        assert!(segment.is_sof(), "SOF3");
        segment.marker = 0xffc5;
        assert!(segment.is_sof(), "SOF5");
        segment.marker = 0xffc6;
        assert!(segment.is_sof(), "SOF6");
        segment.marker = 0xffc7;
        assert!(segment.is_sof(), "SOF7");
        segment.marker = 0xffc9;
        assert!(segment.is_sof(), "SOF9");
        segment.marker = 0xffca;
        assert!(segment.is_sof(), "SOF10");
        segment.marker = 0xffcb;
        assert!(segment.is_sof(), "SOF11");
        segment.marker = 0xffcd;
        assert!(segment.is_sof(), "SOF13");
        segment.marker = 0xffce;
        assert!(segment.is_sof(), "SOF14");
        segment.marker = 0xffcf;
        assert!(segment.is_sof(), "SOF15");
        segment.marker = 0xffc4;
        assert!(!segment.is_sof(), "DHT");
        segment.marker = 0xffc8;
        assert!(!segment.is_sof(), "JPG");
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

        let mut context = JpegContext {
            buf: &data,
            position: 0xc4,
            comments: vec![],
            dimensions: None,
        };

        let segment = context.read_segment().unwrap().unwrap();
        assert!(segment.is_sof());
        let dims = segment.read_sof().unwrap();
        assert_eq!(dims, (512, 341));

        // Test some other sizes. The read_sof() function doesn't read past the
        // width field, so we can just omit the rest of the data.
        assert_eq!(read_sof(&[0x08, 0x00, 0x01, 0x00, 0x01]), (1, 1));
        assert_eq!(read_sof(&[0x08, 0x00, 0x02, 0x00, 0x01]), (1, 2));
        assert_eq!(read_sof(&[0x08, 0x00, 0x01, 0x00, 0x02]), (2, 1));
        assert_eq!(read_sof(&[0x08, 0x00, 0x02, 0x00, 0x02]), (2, 2));
        assert_eq!(read_sof(&[0x08, 0x08, 0x00, 0x03, 0xe8]), (1000, 2048));
    }

    #[test]
    fn test_comment_segment() {
        let buf = sample_image();

        // 00000000: ffd8 ffe0 0010 4a46 4946 0001 0100 0000  ......JFIF......
        // 00000010: 0000 0000 fffe 000c 4275 7474 6572 6375  ........Buttercu
        // 00000020: 7073 ffe1 0016 4578 6966 0000 4d4d 002a  ps....Exif..MM.*
        let mut context = JpegContext {
            buf: &buf,
            position: 0x14,
            comments: vec![],
            dimensions: None,
        };
        let segment = context.read_segment().unwrap().unwrap();
        assert_eq!(segment.marker, 0xfffe);
        assert!(segment.is_com());
        assert_eq!(segment.into_data(), b"Buttercups");
    }

    #[test]
    fn test_resync() {
        let buf = sample_image();

        // 00000000: ffd8 ffe0 0010 4a46 4946 0001 0100 0000  ......JFIF......
        // 00000010: 0000 0000 fffe 000c 4275 7474 6572 6375  ........Buttercu
        // 00000020: 7073 ffe1 0016 4578 6966 0000 4d4d 002a  ps....Exif..MM.*

        // Point the context into the middle of the COM segment; it should
        // resync to the next valid position, which is the APP1 segment starting
        // at 0x22.
        let mut context = JpegContext {
            buf: &buf,
            position: 0x18,
            comments: vec![],
            dimensions: None,
        };
        let segment = context.read_segment().unwrap().unwrap();
        assert_eq!(segment.marker, 0xffe1);
        assert_eq!(segment.position, 0x22);
    }

    #[test]
    fn test_resync_near_end() {
        let mut buf = sample_image();

        // The end of the file looks like this:
        //
        // 0000b950: 67b8 d28f 01ce e639 0a1a 9979 1ecb c9ff  g......9...y....
        // 0000b960: d9                                       .

        // Remove the last 2 bytes of the file (the EOI marker)
        buf.truncate(buf.len() - 2);

        // The end of the file now looks like this:
        //
        // 0000b950: 67b8 d28f 01ce e639 0a1a 9979 1ecb c9    g......9...y...
        //
        // Point the context to a few bytes before the end of the file. It
        // should resync to the end of the file, and return None.
        let mut context = JpegContext {
            buf: &buf,
            position: 0xb950,
            comments: vec![],
            dimensions: None,
        };
        let segment = context.read_segment().unwrap();
        assert!(segment.is_none());
    }

    /// Create a SOF0 segment from the given data, and read its dimensions.
    fn read_sof(data: &[u8]) -> (u16, u16) {
        let segment = JpegSegment {
            marker: 0xffc0,
            position: 0,
            data,
        };

        assert!(segment.is_sof());
        segment.read_sof().unwrap()
    }

    /// Read the sample image from disk.
    fn sample_image() -> Vec<u8> {
        std::fs::read("src/buttercups.jpg").unwrap()
    }
}

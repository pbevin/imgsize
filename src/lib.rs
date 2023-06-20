//! Fast reader for JPEG comments and dimensions
//!
//! # Example
//!
//! ```
//! let data = include_bytes!("buttercups.jpg");
//! let metadata = pb_imgsize::read_bytes(data).unwrap();
//! assert_eq!(512, metadata.width);
//! assert_eq!(341, metadata.height);
//! assert_eq!(vec![b"Buttercups".to_vec()], metadata.comments);
//! ```

mod jpeg;
mod png;
use std::io;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Decoding error: {0}")]
    Decoding(#[from] DecodingError),
}

/// An error that occurred while decoding an image.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DecodingError {
    #[error("Unknown magic number in image data: 0x{0:08x}")]
    UnknownMagic(u32),

    #[error(transparent)]
    Jpeg(#[from] jpeg::JpegDecodingError),

    #[error(transparent)]
    Png(#[from] png::PngDecodingError),

    #[error("Image data too short: {0} bytes")]
    TooShort(usize),
}

pub type Result<T, E = DecodingError> = std::result::Result<T, E>;

/// An image's dimensions, along with any comments found in the data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub comments: Vec<Vec<u8>>,
}

/// Reads the dimensions and comments of an image from a file.
///
/// This function reads the dimensions and comments of an image from a file. It
/// returns an `ImageMetadata` struct containing the width and height of the
/// image, as well as any comments found in the image.
///
/// Note: This function works by reading the entire file into memory.
///
/// # Arguments
///
/// * `path` - A path to the image file.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), pb_imgsize::Error> {
/// let metadata = pb_imgsize::read_file("src/buttercups.jpg")?;
/// assert_eq!(metadata, pb_imgsize::ImageMetadata {
///   width: 512,
///   height: 341,
///   comments: vec![b"Buttercups".to_vec()],
/// });
/// # Ok(())
/// # }
pub fn read_file(path: impl AsRef<Path>) -> Result<ImageMetadata, Error> {
    let buf = std::fs::read(path)?;
    Ok(read_bytes(&buf)?)
}

/// Reads the dimensions and comments of an image from a byte slice.
///
/// This function reads the dimensions and comments of an image from a byte
/// slice. It returns an `ImageMetadata` struct containing the width and height
/// of the image, as well as any comments found in the image.
///
/// # Arguments
///
/// * `data` - A byte slice containing the image data.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), pb_imgsize::Error> {
/// use pb_imgsize::read_bytes;
///
/// let data = include_bytes!("buttercups.jpg");
/// let metadata = read_bytes(data)?;
/// assert_eq!(metadata, pb_imgsize::ImageMetadata {
///    width: 512,
///    height: 341,
///    comments: vec![b"Buttercups".to_vec()]
/// });
/// # Ok(())
/// # }
/// ```
pub fn read_bytes(data: &[u8]) -> Result<ImageMetadata, DecodingError> {
    if data.len() < 4 {
        Err(DecodingError::TooShort(0))
    } else if data.starts_with(b"\xff\xd8") {
        Ok(jpeg::read_jpeg_data(data)?)
    } else if data.starts_with(b"\x89PNG") {
        Ok(png::read_png_data(data)?)
    } else {
        Err(DecodingError::UnknownMagic(u32::from_be_bytes([
            data[0], data[1], data[2], data[3],
        ])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jpeg_file() {
        let metadata = read_file(Path::new("src/buttercups.jpg")).unwrap();
        assert_eq!(512, metadata.width);
        assert_eq!(341, metadata.height);
        let comments = metadata
            .comments
            .iter()
            .map(|c| String::from_utf8_lossy(c))
            .collect::<Vec<_>>();
        assert_eq!(vec!["Buttercups".to_string()], comments);
    }

    #[test]
    fn test_png_file() {
        let metadata = read_file(Path::new("src/watercolors.png")).unwrap();
        assert_eq!(400, metadata.width);
        assert_eq!(224, metadata.height);
        let comments = metadata
            .comments
            .iter()
            .map(|c| String::from_utf8_lossy(c))
            .collect::<Vec<_>>();
        assert_eq!(vec!["Abstract watercolors".to_string()], comments);
    }
}

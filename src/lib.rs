//! Fast reader for JPEG and PNG comments and dimensions.
//!
//! The `pb-imgsize` crate provides a reader for JPEG and PNG images that can
//! quickly extract the image's dimensions and any comments embedded in the
//! image.
//!
//! For PNG images, the dimensions are extracted from the IHDR chunk, and the
//! comments are extracted from tEXt chunks with the keyword "comment".
//!
//! For JPEG images, the dimensions are extracted from the SOFx chunk, and the
//! comments are extracted from COM chunks.
//!
//! The reader is fast because it only reads the chunks that are necessary to
//! extract the dimensions and comments. It does not decode the image data.
//!
//! The reader does not attempt to read EXIF data.
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
use std::fmt::Display;
use std::io;
use std::path::Path;

pub use jpeg::JpegDecodingError;
pub use png::PngDecodingError;

/// An error that occurred while reading an image.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Decoding(DecodingError),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<DecodingError> for Error {
    fn from(e: DecodingError) -> Self {
        Error::Decoding(e)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::Decoding(e) => write!(f, "Decoding error: {}", e),
        }
    }
}

impl std::error::Error for Error {}

/// An error that occurred while decoding an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodingError {
    // #[error("Unknown magic number in image data: 0x{0:08x}")]
    UnknownMagic(u32),

    // #[error(transparent)]
    Jpeg(jpeg::JpegDecodingError),

    // #[error(transparent)]
    Png(png::PngDecodingError),

    // #[error("Image data too short: {0} bytes")]
    TooShort(usize),
}

impl From<jpeg::JpegDecodingError> for DecodingError {
    fn from(e: jpeg::JpegDecodingError) -> Self {
        DecodingError::Jpeg(e)
    }
}

impl From<png::PngDecodingError> for DecodingError {
    fn from(e: png::PngDecodingError) -> Self {
        DecodingError::Png(e)
    }
}

impl Display for DecodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            DecodingError::UnknownMagic(magic) => {
                write!(f, "Unknown magic number: 0x{:08x}", magic)
            }
            DecodingError::Jpeg(e) => write!(f, "JPEG decoding error: {}", e),
            DecodingError::Png(e) => write!(f, "PNG decoding error: {}", e),
            DecodingError::TooShort(n) => write!(f, "Image data too short: {} bytes", n),
        }
    }
}

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

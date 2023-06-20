# `pb-imgsize` 
Fast JPEG and PNG image metadata reader in Rust.

This Rust library provides an efficient way to extract image dimensions (width and height) and comments embedded in JPEG and PNG image files without needing to decode the entire image. The primary focus of this library is to perform these operations as quickly as possible.

## Features

- Supports JPEG and PNG image formats.
- Reads image dimensions (width and height).
- Extracts comments from image data.
- Lightweight and efficient, designed for speed.

## Installation

Add `pb-imgsize` to your `Cargo.toml` file:

```toml
[dependencies]
pb-imgsize = "0.1.0"
```

## Usage

Include the library in your Rust file:

```rust
use pb_imgsize as imgsize;
```

### Reading from an Image File

To read metadata from an image file, use the `read_file` function:

```rust
let metadata = imgsize::read_file("path/to/image.jpg").unwrap();
```

### Reading from a Byte Slice

To read metadata from a byte slice, use the `read_bytes` function:

```rust
let data = include_bytes!("path/to/image.jpg");
let metadata = imgsize::read_bytes(data).unwrap();
```

Both functions return an `ImageMetadata` struct containing the `width`, `height` and `comments` fields.

```rust
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub comments: Vec<Vec<u8>>,
}
```

## Example

Here's an example that demonstrates how to use `pb-imgsize` to read metadata from a JPEG file:

```rust
use pb_imgsize as imgsize;

fn main() -> Result<(), imgsize::Error> {
    let metadata = imgsize::read_file("path/to/image.jpg")?;
    println!("Width: {}", metadata.width);
    println!("Height: {}", metadata.height);
    for comment in metadata.comments {
        println!("Comment: {}", String::from_utf8_lossy(&comment));
    }
    Ok(())
}
```

## Error Handling

The library defines an `Error` enum that encapsulates the various errors that can occur when trying to read image data. There are specific error types for I/O errors and decoding errors.

## Testing

To run the tests:

```bash
cargo test
```

## License

MIT

## Contribution

Contributions are always welcome! Please adhere to the project's Code of Conduct during your participation in this project.

Please feel free to open an issue or a pull request if you have any issues or feature requests.

---

For more details, check the Rust documentation for the `pb-imgsize` library.

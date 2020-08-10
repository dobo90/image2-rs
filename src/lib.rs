//! image2 - a high-performance image processing library with wide support for a variety of file
//! formats and data types
//!
//! OpenImageIO is used for encoding and decoding images, it should be installed before
//! building `image2`.
//!
//! ```rust,no_run
//! use image2::*;
//!
//! fn main() -> Result<(), Error> {
//!     // Load an image from disk
//!     let image = Image::<f32, Rgb>::open("images/A.exr")?;
//!
//!     // Apply a `Filter`, in this case using the `Convert` filter to
//!     // convert from `Rgb` to `Gray`
//!     let dest = image.new_like_with_color::<Gray>();
//!     let dest = image.apply(Convert::<Gray>::new(), dest);
//!
//!     // Save an image to disk
//!     dest.save("test.jpg")?;
//!
//!     Ok(())
//! }
//!
//! ```

pub use half::f16;

mod color;
mod error;
mod histogram;
mod image;
mod pixel;
mod r#type;

pub mod filter;
pub mod io;
pub mod kernel;
pub mod transform;

pub use color::{Color, Convert, Gray, Rgb, Rgba, Xyz};
pub use error::Error;
pub use filter::Filter;
pub use histogram::Histogram;
pub use image::{Hash, Image, Meta};
pub use kernel::Kernel;
pub use pixel::Pixel;
pub use r#type::Type;

#[cfg(test)]
mod tests;

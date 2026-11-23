//! Images travel lossless: what one side encodes, the other decodes to the
//! same pixels.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use protocol::{Image, RgbPixels};

/// Pixels no lossy format would keep: every pixel differs from its
/// neighbours in a way no predictor guesses.
fn noise(width: u32, height: u32) -> RgbPixels {
    let mut state: u32 = 0x1234_5678;
    let rgb = (0..width * height * 3)
        .map(|_| {
            // xorshift32
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        })
        .collect();
    RgbPixels { width, height, rgb }
}

#[test]
fn an_image_decodes_to_exactly_the_pixels_it_was_made_from() {
    let pixels = noise(352, 198);

    let image = Image::encode(&pixels).unwrap();

    assert_eq!(image.decode().unwrap(), pixels);
}

#[test]
fn an_image_knows_its_size_without_decoding() {
    let image = Image::encode(&noise(800, 480)).unwrap();

    assert_eq!(image.size(), Some((800, 480)));
}

#[test]
fn pixels_that_do_not_match_the_size_are_not_an_image() {
    let mut pixels = noise(4, 4);
    pixels.rgb.pop();

    assert!(Image::encode(&pixels).is_err());
}

#[test]
fn an_image_decodes_straight_into_a_buffer_of_its_size() {
    let pixels = noise(30, 20);
    let image = Image::encode(&pixels).unwrap();
    let mut buffer = vec![0; 30 * 20 * 3];

    image.decode_into(&mut buffer).unwrap();

    assert_eq!(buffer, pixels.rgb);
}

#[test]
fn an_image_does_not_decode_into_a_buffer_of_another_size() {
    let image = Image::encode(&noise(30, 20)).unwrap();

    for len in [30 * 20 * 3 - 1, 30 * 20 * 3 + 1] {
        let mut buffer = vec![0; len];
        assert!(image.decode_into(&mut buffer).is_err(), "{len} bytes");
    }
}

#[test]
fn bytes_that_are_not_an_image_do_not_decode() {
    // As a peer would send them: any bytes can arrive in a message.
    let garbage = postcard::to_allocvec(&serde_bytes_like(b"not an image")).unwrap();
    let image: Image = postcard::from_bytes(&garbage).unwrap();

    assert!(image.decode().is_err());
    assert_eq!(image.size(), None);
}

/// Encodes `bytes` the way postcard encodes a byte string: a length, then the
/// bytes.
fn serde_bytes_like(bytes: &[u8]) -> impl serde::Serialize + '_ {
    struct Bytes<'a>(&'a [u8]);
    impl serde::Serialize for Bytes<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_bytes(self.0)
        }
    }
    Bytes(bytes)
}

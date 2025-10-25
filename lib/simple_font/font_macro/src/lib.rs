extern crate proc_macro;

use image::{GenericImage, GenericImageView, Pixel, Rgba};
use proc_macro::{Span, TokenStream};
use std::{fs::File, io::Read};

/// A macro to include specified font at compile time.
///
/// # Example
/// ```ignore
/// use simple_font::SimpleFont;
/// use simple_font::font_macro::include_font;
///
/// const MY_FONT: SimpleFont<'static> = include_font!("./path/to/font_image.png", 8, 16);
/// ```
#[proc_macro]
pub fn include_font(item: TokenStream) -> TokenStream {
    let mut items = item.into_iter();

    let span = Span::call_site();
    let Some(source) = span.local_file() else {
        //
        return TokenStream::new();
    };
    let cwd = source.parent().unwrap();

    let Some(proc_macro::TokenTree::Literal(file_name)) = items.next() else {
        panic!("Expected a string literal as font file name");
    };
    let file_name = file_name.to_string();
    let file_name = (file_name.as_str()).trim_matches('"');
    let path = cwd.join(file_name);
    let mut file = match File::open(&path) {
        Ok(f) => f,
        Err(e) => panic!("Failed to open font file {}: {}", path.display(), e),
    };

    let mut source = Vec::new();
    file.read_to_end(&mut source)
        .expect("Failed to read font file");
    let image = image::load_from_memory(&source).expect("Failed to decode font image");
    let (image_width, image_height) = image.dimensions();

    let (cols, rows) = (16, 6);
    let (font_width, font_height) =
        if expect_comma(&mut items, "Expected a comma after file name").is_some() {
            let width = expect_uint(&mut items, "Failed to parse font width")
                .expect("Font width is required");
            expect_comma(&mut items, "Expected a comma after font width")
                .expect("Expected a comma after font width");
            let height = expect_uint(&mut items, "Failed to parse font height")
                .expect("Font height is required");
            (width as u32, height as u32)
        } else {
            (image_width / cols, image_height / rows)
        };
    let (cols, rows) = if expect_comma(&mut items, "Expected a comma after file name").is_some() {
        let cols = expect_uint(&mut items, "Failed to parse cols").expect("Cols is required");
        expect_comma(&mut items, "Expected a comma after cols")
            .expect("Expected a comma after cols");
        let rows = expect_uint(&mut items, "Failed to parse rows").expect("Rows is required");
        (cols as u32, rows as u32)
    } else {
        (cols, rows)
    };

    if image_width != font_width * cols || image_height != font_height * rows {
        panic!(
            "Font image dimensions do not match specified size: expected {}x{}, got {}x{} (font size: {}x{})",
            font_width * cols,
            font_height * rows,
            image_width,
            image_height,
            font_width,
            font_height,
        );
    }

    let mut image = image.into_rgba8();

    // background color is the color of the top-left pixel
    let bg_color = image.get_pixel(0, 0).to_rgba();

    let mut output_data = String::new();
    let font_w8 = font_width / 8;
    let font_w7 = font_width & 7;
    for row in 0..rows {
        for col in 0..cols {
            let x = col * font_width;
            let y = row * font_height;
            let mut glyph_data = String::new();
            let source = image.sub_image(x, y, font_width, font_height);

            for y in 0..font_height {
                for x0 in 0..font_w8 {
                    let mut acc = 0;
                    for x in 0..8 {
                        let pixel = source.get_pixel(x0 * 8 + x, y).to_rgba();
                        if pixel_to_mono(&pixel, &bg_color) {
                            acc |= 1 << (7 - x);
                        }
                    }
                    glyph_data.push_str(&format!("{:#02x}, ", acc));
                }
                {
                    let mut acc = 0;
                    for x in 0..font_w7 {
                        let pixel = source.get_pixel(font_w8 * 8 + x, y).to_rgba();
                        if pixel_to_mono(&pixel, &bg_color) {
                            acc |= 1 << (7 - x);
                        }
                    }
                    if font_w7 > 0 {
                        glyph_data.push_str(&format!("{:#02x}, ", acc));
                    }
                }
            }
            output_data.push_str(&glyph_data);
            output_data.push_str("\n");
        }
    }

    format!(
        "SimpleFont::ascii(&[{}], ({}, {}))",
        &output_data, font_width, font_height
    )
    .parse()
    .unwrap()
}

#[track_caller]
fn expect_comma(
    items: &mut impl Iterator<Item = proc_macro::TokenTree>,
    message: &str,
) -> Option<()> {
    let token = items.next()?;
    match token {
        proc_macro::TokenTree::Punct(punct) => {
            if punct.as_char() != ',' {
                panic!("{}", message);
            }
        }
        _ => {
            panic!("{}", message);
        }
    };
    Some(())
}

#[track_caller]
fn expect_uint(
    items: &mut impl Iterator<Item = proc_macro::TokenTree>,
    message: &str,
) -> Option<usize> {
    let Some(proc_macro::TokenTree::Literal(lit)) = items.next() else {
        return None;
    };
    usize::from_str_radix(lit.to_string().as_str(), 10)
        .expect(message)
        .into()
}

/// Converts a pixel to monochrome based on the reference pixel (background color).
///
/// Returns true if the pixel is considered "on" (foreground), and false if it is "off" (background).
#[inline]
fn pixel_to_mono(pixel: &Rgba<u8>, ref_pixel: &Rgba<u8>) -> bool {
    if ref_pixel[3] < 64 {
        pixel[3] >= 128
    } else {
        let dr = pixel[0].abs_diff(ref_pixel[0]) as usize;
        let dg = pixel[1].abs_diff(ref_pixel[1]) as usize;
        let db = pixel[2].abs_diff(ref_pixel[2]) as usize;
        dr + dg + db > 128
    }
}

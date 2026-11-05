//! Windows has no text next to tray icons, so the count is drawn into the icon.

pub const SIZE: u32 = 32;
const SCALE: usize = 3;
const GLYPH_WIDTH: usize = 5;
const GLYPH_HEIGHT: usize = 7;
const GAP: usize = 2;
const BACKGROUND: [u8; 4] = [14, 107, 111, 255];
const FOREGROUND: [u8; 4] = [255, 255, 255, 255];

fn glyph(character: char) -> [u8; GLYPH_HEIGHT] {
    match character {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        's' => [0x00, 0x00, 0x0E, 0x10, 0x0E, 0x01, 0x1E],
        '+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        _ => [0; GLYPH_HEIGHT],
    }
}

/// RGBA pixels for a `SIZE`×`SIZE` icon; labels past two characters are cut.
pub fn render(label: &str) -> Vec<u8> {
    let size = SIZE as usize;
    let mut pixels: Vec<u8> = BACKGROUND.repeat(size * size);
    let characters: Vec<char> = label.chars().take(2).collect();
    let count = characters.len();
    let width = count * GLYPH_WIDTH * SCALE + count.saturating_sub(1) * GAP;
    let left = size.saturating_sub(width) / 2;
    let top = (size - GLYPH_HEIGHT * SCALE) / 2;

    for (index, character) in characters.into_iter().enumerate() {
        let origin = left + index * (GLYPH_WIDTH * SCALE + GAP);
        for (row, bits) in glyph(character).into_iter().enumerate() {
            for column in 0..GLYPH_WIDTH * SCALE {
                if bits >> (GLYPH_WIDTH - 1 - column / SCALE) & 1 == 0 {
                    continue;
                }
                for y in 0..SCALE {
                    let at = (top + row * SCALE + y) * size + origin + column;
                    pixels[at * 4..at * 4 + 4].copy_from_slice(&FOREGROUND);
                }
            }
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(pixels: &[u8]) -> usize {
        pixels
            .chunks(4)
            .filter(|pixel| *pixel == FOREGROUND)
            .count()
    }

    #[test]
    fn output_is_a_full_rgba_square() {
        assert_eq!(render("43").len(), (SIZE * SIZE * 4) as usize);
        assert_eq!(render("").len(), (SIZE * SIZE * 4) as usize);
    }

    #[test]
    fn every_supported_character_draws_distinct_pixels() {
        let labels = "0123456789s+";
        let images: Vec<_> = labels.chars().map(|c| render(&c.to_string())).collect();
        for (i, image) in images.iter().enumerate() {
            assert!(lit(image) > 0);
            assert!(images[i + 1..].iter().all(|other| other != image));
        }
    }

    #[test]
    fn two_characters_fit_without_wrapping_around_the_row() {
        let pixels = render("88");
        let size = SIZE as usize;
        for row in pixels.chunks(size * 4) {
            let first = row.chunks(4).position(|pixel| pixel == FOREGROUND);
            let last = row.chunks(4).rposition(|pixel| pixel == FOREGROUND);
            if let (Some(first), Some(last)) = (first, last) {
                assert!(last - first < size);
            }
        }
        assert_eq!(lit(&pixels), 2 * lit(&render("8")));
    }

    #[test]
    fn extra_characters_are_ignored_rather_than_overflowing() {
        assert_eq!(render("123"), render("12"));
    }
}

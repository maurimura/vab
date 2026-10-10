//! The backbox's dot-matrix display (DMD): 128 by 32 amber dots, written in a 5 by 7 font (the
//! one the table's art uses, tools/pinball-art/saloon/backbox.js). The backbox's art has every
//! dot dark; this draws the lit ones, and a little glow, into an image laid over it.

/// The display's dots across and down.
pub const COLUMNS: usize = 128;
pub const ROWS: usize = 32;
/// Image pixels per dot.
pub const DOT: usize = 6;

/// Which dots are lit.
#[derive(Clone, PartialEq, Eq)]
pub struct Dots(Vec<bool>);

impl Default for Dots {
    fn default() -> Self {
        Self(vec![false; COLUMNS * ROWS])
    }
}

impl Dots {
    fn set(&mut self, x: i32, y: i32) {
        if (0..COLUMNS as i32).contains(&x) && (0..ROWS as i32).contains(&y) {
            self.0[y as usize * COLUMNS + x as usize] = true;
        }
    }

    /// Writes `text` with its top-left at `x`, `y`, each font pixel `scale` dots.
    pub fn text(&mut self, text: &str, x: i32, y: i32, scale: i32) {
        let mut x = x;
        for ch in text.chars() {
            let (rows, c0, c1) = glyph(ch);
            for (r, row) in rows.iter().enumerate() {
                for c in c0..=c1 {
                    if row.as_bytes()[c] == b'#' {
                        for a in 0..scale {
                            for b in 0..scale {
                                self.set(x + (c - c0) as i32 * scale + b, y + r as i32 * scale + a);
                            }
                        }
                    }
                }
            }
            x += (c1 - c0 + 2) as i32 * scale;
        }
    }

    /// Writes `text` centred on the display, its top at `y`.
    pub fn centred(&mut self, text: &str, y: i32, scale: i32) {
        let x = (COLUMNS as i32 - text_width(text, scale)) / 2;
        self.text(text, x, y, scale);
    }

    /// Writes `text` ending at the display's right edge.
    pub fn right(&mut self, text: &str, y: i32, scale: i32) {
        self.text(text, COLUMNS as i32 - text_width(text, scale), y, scale);
    }

    /// The lit dots as an image's RGBA bytes, `COLUMNS * DOT` by `ROWS * DOT`: each a round
    /// amber dot with a soft glow round it, transparent elsewhere.
    pub fn rgba(&self) -> Vec<u8> {
        let (width, height) = (COLUMNS * DOT, ROWS * DOT);
        let mut data = vec![0; width * height * 4];
        let centre = DOT as f32 / 2.0;
        for (i, lit) in self.0.iter().enumerate() {
            if !lit {
                continue;
            }
            let (cx, cy) = (i % COLUMNS * DOT, i / COLUMNS * DOT);
            for y in 0..DOT {
                for x in 0..DOT {
                    let d = ((x as f32 + 0.5 - centre).powi(2) + (y as f32 + 0.5 - centre).powi(2))
                        .sqrt();
                    let (color, alpha) = if d <= DOT as f32 * 0.36 {
                        ([255, 181, 77], 255)
                    } else if d <= DOT as f32 * 0.55 {
                        ([255, 138, 31], 110)
                    } else {
                        continue;
                    };
                    let p = ((cy + y) * width + cx + x) * 4;
                    data[p..p + 3].copy_from_slice(&color);
                    data[p + 3] = alpha;
                }
            }
        }
        data
    }
}

/// How many dots wide `text` is, written at `scale`.
pub fn text_width(text: &str, scale: i32) -> i32 {
    let w: i32 = text
        .chars()
        .map(|ch| {
            let (_, c0, c1) = glyph(ch);
            (c1 - c0 + 2) as i32 * scale
        })
        .sum();
    w - scale
}

/// A character's seven rows of five, and the columns it uses (narrow for punctuation).
fn glyph(ch: char) -> ([&'static str; 7], usize, usize) {
    let g = FONT
        .iter()
        .find(|(c, _)| *c == ch.to_ascii_uppercase())
        .map_or(FONT[FONT.len() - 2].1, |(_, g)| g);
    let rows: [&str; 7] = std::array::from_fn(|r| &g[r * 5..r * 5 + 5]);
    if ch == ' ' {
        return (rows, 0, 2);
    }
    let blank = |c: usize| rows.iter().all(|r| r.as_bytes()[c] == b'.');
    let (mut c0, mut c1) = (0, 4);
    if ",:.!".contains(ch) {
        while c0 < 4 && blank(c0) {
            c0 += 1;
        }
        while c1 > c0 && blank(c1) {
            c1 -= 1;
        }
    }
    (rows, c0, c1)
}

/// 5 by 7 glyphs, row by row.
const FONT: [(char, &str); 44] = [
    ('A', ".###.#...##...#######...##...##...#"),
    ('B', "####.#...##...#####.#...##...#####."),
    ('C', ".###.#...##....#....#....#...#.###."),
    ('D', "####.#...##...##...##...##...#####."),
    ('E', "######....#....####.#....#....#####"),
    ('F', "######....#....####.#....#....#...."),
    ('G', ".###.#...##....#.####...##...#.####"),
    ('H', "#...##...##...#######...##...##...#"),
    ('I', ".###...#....#....#....#....#...###."),
    ('J', "..###...#....#....#....#.#..#..##.."),
    ('K', "#...##..#.#.#..##...#.#..#..#.#...#"),
    ('L', "#....#....#....#....#....#....#####"),
    ('M', "#...###.###.#.##.#.##...##...##...#"),
    ('N', "#...##...###..##.#.##..###...##...#"),
    ('O', ".###.#...##...##...##...##...#.###."),
    ('P', "####.#...##...#####.#....#....#...."),
    ('Q', ".###.#...##...##...##.#.##..#..##.#"),
    ('R', "####.#...##...#####.#.#..#..#.#...#"),
    ('S', ".#####....#.....###.....#....#####."),
    ('T', "#####..#....#....#....#....#....#.."),
    ('U', "#...##...##...##...##...##...#.###."),
    ('V', "#...##...##...##...##...#.#.#...#.."),
    ('W', "#...##...##...##.#.##.#.##.#.#.#.#."),
    ('X', "#...##...#.#.#...#...#.#.#...##...#"),
    ('Y', "#...##...#.#.#...#....#....#....#.."),
    ('Z', "#####....#...#...#...#...#....#####"),
    ('0', ".###.#...##..###.#.###..##...#.###."),
    ('1', "..#...##....#....#....#....#...###."),
    ('2', ".###.#...#....#...#...#...#...#####"),
    ('3', "#####...#...#.....#.....##...#.###."),
    ('4', "...#...##..#.#.#..#.#####...#....#."),
    ('5', "######....####.....#....##...#.###."),
    ('6', "..##..#...#....####.#...##...#.###."),
    ('7', "#####....#...#...#...#....#....#..."),
    ('8', ".###.#...##...#.###.#...##...#.###."),
    ('9', ".###.#...##...#.####....#...#..##.."),
    (',', "..........................##....#.."),
    (':', "......##...##........##...##......."),
    ('/', ".........#...#...#...#...#........."),
    ('-', "...............#####..............."),
    ('.', "..........................##...##.."),
    ('!', "..#....#....#....#....#.........#.."),
    (' ', "..................................."),
    ('?', ".###.#...#....#...#....#.........#."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_as_wide_as_its_letters() {
        // Five columns and a gap each, the last gap not counted; a space is three.
        assert_eq!(text_width("AB", 1), 11);
        assert_eq!(text_width("A B", 1), 15);
        assert_eq!(text_width("AB", 2), 22);
        let mut dots = Dots::default();
        dots.centred("HI", 0, 1);
        assert!(dots.0.iter().any(|&lit| lit));
    }
}

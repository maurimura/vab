//! Drawing pixel art into an image, pixel by pixel: the pool, air hockey and shuffleboard
//! tables are drawn this way, so lines and circles land on whole pixels at any angle.

use bevy::prelude::*;

/// A canvas's RGBA bytes, row by row.
#[derive(Clone)]
pub struct Pixels {
    data: Vec<u8>,
    size: UVec2,
}

impl Pixels {
    /// A canvas `size` pixels across and down, all clear.
    pub fn new(size: UVec2) -> Self {
        Self {
            data: vec![0; (size.x * size.y * 4) as usize],
            size,
        }
    }

    /// The canvas as an image's bytes, RGBA, row by row.
    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    pub fn set(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if (0..self.size.x as i32).contains(&x) && (0..self.size.y as i32).contains(&y) {
            let i = (y as usize * self.size.x as usize + x as usize) * 4;
            self.data[i..i + 4].copy_from_slice(&color);
        }
    }

    /// The colour at `x`, `y`, clear off the canvas.
    pub fn get(&self, x: i32, y: i32) -> [u8; 4] {
        if (0..self.size.x as i32).contains(&x) && (0..self.size.y as i32).contains(&y) {
            let i = (y as usize * self.size.x as usize + x as usize) * 4;
            self.data[i..i + 4].try_into().unwrap_or_default()
        } else {
            [0; 4]
        }
    }

    pub fn size(&self) -> UVec2 {
        self.size
    }

    /// Draws `other` over this canvas with its top-left corner at `at`, all but its clear
    /// pixels.
    pub fn draw(&mut self, other: &Pixels, at: IVec2) {
        for y in 0..other.size.y as i32 {
            for x in 0..other.size.x as i32 {
                let color = other.get(x, y);
                if color[3] > 0 {
                    self.set(at.x + x, at.y + y, color);
                }
            }
        }
    }

    pub fn rect(&mut self, from: IVec2, to: IVec2, color: [u8; 4]) {
        for y in from.y..to.y {
            for x in from.x..to.x {
                self.set(x, y, color);
            }
        }
    }

    /// A convex polygon, its corners in order either way round.
    pub fn polygon(&mut self, corners: &[Vec2], color: [u8; 4]) {
        let low = corners
            .iter()
            .copied()
            .reduce(Vec2::min)
            .unwrap_or_default()
            .floor();
        let high = corners
            .iter()
            .copied()
            .reduce(Vec2::max)
            .unwrap_or_default()
            .ceil();
        let side = |a: Vec2, b: Vec2, p: Vec2| (b - a).perp_dot(p - a);
        for y in low.y as i32..=high.y as i32 {
            for x in low.x as i32..=high.x as i32 {
                let middle = Vec2::new(x as f32, y as f32) + 0.5;
                let sides = corners
                    .iter()
                    .zip(corners.iter().cycle().skip(1))
                    .map(|(&a, &b)| side(a, b, middle));
                let (mut left, mut right) = (false, false);
                for s in sides {
                    left |= s < 0.0;
                    right |= s > 0.0;
                }
                if !(left && right) {
                    self.set(x, y, color);
                }
            }
        }
    }

    /// Every pixel within `reach` of the line from `from` to `to`.
    pub fn thick_line(&mut self, from: Vec2, to: Vec2, reach: f32, color: [u8; 4]) {
        let (low, high) = (
            (from.min(to) - reach).floor(),
            (from.max(to) + reach).ceil(),
        );
        let along = to - from;
        for y in low.y as i32..=high.y as i32 {
            for x in low.x as i32..=high.x as i32 {
                let middle = Vec2::new(x as f32, y as f32) + 0.5;
                let t = ((middle - from).dot(along) / along.length_squared()).clamp(0.0, 1.0);
                if middle.distance(from + along * t) <= reach {
                    self.set(x, y, color);
                }
            }
        }
    }

    /// A circle one pixel thick.
    pub fn ring(&mut self, center: Vec2, radius: f32, color: [u8; 4]) {
        let (low, high) = (
            (center - radius - 1.0).floor(),
            (center + radius + 1.0).ceil(),
        );
        for y in low.y as i32..=high.y as i32 {
            for x in low.x as i32..=high.x as i32 {
                let distance = (Vec2::new(x as f32, y as f32) + 0.5).distance(center);
                if (radius - 0.5..radius + 0.5).contains(&distance) {
                    self.set(x, y, color);
                }
            }
        }
    }

    pub fn disc(&mut self, center: Vec2, radius: f32, color: [u8; 4]) {
        let (low, high) = ((center - radius).floor(), (center + radius).ceil());
        for y in low.y as i32..=high.y as i32 {
            for x in low.x as i32..=high.x as i32 {
                let middle = Vec2::new(x as f32, y as f32) + 0.5;
                if middle.distance(center) <= radius {
                    self.set(x, y, color);
                }
            }
        }
    }
}

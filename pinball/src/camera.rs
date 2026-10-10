//! Where a point on (or above) the playfield appears on screen: the same camera the table's art is
//! drawn with (tools/pinball-art/saloon/lib.js, `makeCamera`), so the ball lands exactly where the
//! art puts things. Screen coordinates are the art's: 1920x1080 pixels, y down.

use glam::{Vec2, Vec3};

use crate::layout::CameraParams;

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// The camera's position, and its up and forward directions, in table inches (z up).
    eye: Vec3,
    up: Vec3,
    forward: Vec3,
    /// Screen pixels per projected unit, and where the projection's origin lands on screen.
    scale: f32,
    origin: Vec2,
}

impl Camera {
    /// The camera looking at a playfield `size` (width, length) inches.
    pub fn new(params: CameraParams, size: [f32; 2]) -> Self {
        let [width, length] = size;
        let tilt = params.tilt.to_radians();
        let target_y = length / 2.0;
        let eye = Vec3::new(
            width / 2.0,
            target_y + params.dist * tilt.sin(),
            params.dist * tilt.cos(),
        );
        let up = Vec3::new(0.0, -tilt.cos(), tilt.sin());
        let forward = Vec3::new(0.0, -tilt.sin(), -tilt.cos());
        let mut camera = Self {
            eye,
            up,
            forward,
            scale: 1.0,
            origin: Vec2::ZERO,
        };
        // Fitted so the far end lands on `fit_top` and the near end on `fit_bottom`.
        let far = camera.raw(Vec3::new(width / 2.0, 0.0, 0.0));
        let near = camera.raw(Vec3::new(width / 2.0, length, 0.0));
        camera.scale = (params.fit_bottom - params.fit_top) / (near.y - far.y);
        camera.origin = Vec2::new(params.center_x, params.fit_top - camera.scale * far.y);
        camera
    }

    fn raw(&self, at: Vec3) -> Vec2 {
        let q = at - self.eye;
        let along_up = q.dot(self.up);
        let depth = q.dot(self.forward);
        Vec2::new(q.x / depth, -along_up / depth)
    }

    /// Where `at` (x, y on the playfield, z above it) is on screen.
    pub fn project(&self, at: Vec3) -> Vec2 {
        self.origin + self.scale * self.raw(at)
    }

    /// Screen pixels per inch across the table at `at`: how big things there look.
    pub fn pixels_per_inch(&self, at: Vec3) -> f32 {
        let half = Vec3::new(0.5, 0.0, 0.0);
        self.project(at + half).x - self.project(at - half).x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saloon() -> Camera {
        let params = CameraParams {
            tilt: 39.0,
            dist: 84.0,
            fit_top: 112.0,
            fit_bottom: 932.0,
            center_x: 520.0,
        };
        Camera::new(params, [20.25, 42.0])
    }

    #[test]
    fn the_ends_land_where_the_art_put_them() {
        let camera = saloon();
        let far = camera.project(Vec3::new(10.125, 0.0, 0.0));
        let near = camera.project(Vec3::new(10.125, 42.0, 0.0));
        assert!((far - Vec2::new(520.0, 112.0)).length() < 0.01, "{far}");
        assert!((near - Vec2::new(520.0, 932.0)).length() < 0.01, "{near}");
        // Nearer is bigger, and higher is up the screen.
        let middle = Vec3::new(10.0, 20.0, 0.0);
        assert!(
            camera.pixels_per_inch(Vec3::new(10.0, 40.0, 0.0)) > camera.pixels_per_inch(middle)
        );
        assert!(camera.project(middle + Vec3::Z).y < camera.project(middle).y);
    }
}

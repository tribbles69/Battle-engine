//! Minimal 2D vector maths for the ground plane.
//!
//! The simulation is purely 2D (x = east, y = north, metres). The renderer maps
//! it onto the 3D ground plane. Keeping our own tiny type means the sim has no
//! engine dependency and can run headless, in tests, or on a server.

use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Vec2 { x, y }
    }

    /// Unit vector pointing along `angle` radians (0 = east, PI/2 = north).
    pub fn from_angle(angle: f32) -> Self {
        Vec2::new(angle.cos(), angle.sin())
    }

    pub fn angle(self) -> f32 {
        self.y.atan2(self.x)
    }

    pub fn dot(self, o: Vec2) -> f32 {
        self.x * o.x + self.y * o.y
    }

    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }

    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    pub fn distance(self, o: Vec2) -> f32 {
        (self - o).length()
    }

    pub fn distance_sq(self, o: Vec2) -> f32 {
        (self - o).length_sq()
    }

    /// Normalised copy, or zero if the vector is (nearly) zero.
    pub fn normalize_or_zero(self) -> Vec2 {
        let l = self.length();
        if l > 1e-6 {
            self * (1.0 / l)
        } else {
            Vec2::ZERO
        }
    }

    /// Rotate 90 degrees clockwise: the "right-hand" side when facing along self.
    pub fn perp_right(self) -> Vec2 {
        Vec2::new(self.y, -self.x)
    }

    /// Clamp the length to at most `max`.
    pub fn clamp_length(self, max: f32) -> Vec2 {
        let l = self.length();
        if l > max && l > 1e-6 {
            self * (max / l)
        } else {
            self
        }
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        self.x += o.x;
        self.y += o.y;
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl SubAssign for Vec2 {
    fn sub_assign(&mut self, o: Vec2) {
        self.x -= o.x;
        self.y -= o.y;
    }
}
impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f32) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}
impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}

/// Shortest signed difference between two angles, in (-PI, PI].
pub fn angle_diff(from: f32, to: f32) -> f32 {
    let mut d = (to - from) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d <= -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

/// Rotate `current` towards `target` by at most `max_step` radians.
pub fn turn_towards(current: f32, target: f32, max_step: f32) -> f32 {
    let d = angle_diff(current, target);
    if d.abs() <= max_step {
        target
    } else {
        current + max_step * d.signum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn angle_diff_wraps() {
        assert!((angle_diff(0.1, -0.1) + 0.2).abs() < 1e-5);
        assert!((angle_diff(PI - 0.1, -PI + 0.1) - 0.2).abs() < 1e-5);
    }

    #[test]
    fn perp_right_is_clockwise() {
        let north = Vec2::new(0.0, 1.0);
        assert_eq!(north.perp_right(), Vec2::new(1.0, 0.0));
    }
}

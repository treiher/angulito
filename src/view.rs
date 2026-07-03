//! The [`View`] type, holding the zoom and pan of the editor viewport
//! together with the clamping that keeps the image filling it.

/// Zoom and pan state of the editor viewport.
///
/// The stage is transformed with `translate(pan) scale(zoom)` and origin
/// `0 0`, so a stage-local point `p` appears at `pan + zoom * p` in viewport
/// coordinates. Pan is clamped so the image always fills the viewport.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct View {
    pub zoom: f64,
    pub pan: (f64, f64),
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: (0.0, 0.0),
        }
    }
}

impl View {
    pub const MIN_ZOOM: f64 = 1.0;
    pub const MAX_ZOOM: f64 = 8.0;

    /// General two-finger gesture: the viewport point that was at `old_mid`
    /// follows the fingers to `new_mid` while zooming by `factor`.
    pub fn pinch(
        &mut self,
        old_mid: (f64, f64),
        new_mid: (f64, f64),
        factor: f64,
        size: (f64, f64),
    ) {
        let new_zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let ratio = new_zoom / self.zoom;
        self.pan.0 = new_mid.0 - ratio * (old_mid.0 - self.pan.0);
        self.pan.1 = new_mid.1 - ratio * (old_mid.1 - self.pan.1);
        self.zoom = new_zoom;
        self.clamp_pan(size);
    }

    /// Zooms by `factor`, keeping the viewport point `focal` fixed.
    pub fn zoom_at(&mut self, focal: (f64, f64), factor: f64, size: (f64, f64)) {
        self.pinch(focal, focal, factor, size);
    }

    pub fn pan_by(&mut self, delta: (f64, f64), size: (f64, f64)) {
        self.pan.0 += delta.0;
        self.pan.1 += delta.1;
        self.clamp_pan(size);
    }

    /// Pulls the pan back so the image still fills a viewport of size `size`.
    /// Every zoom and pan applies this on its own. Callers need it after the
    /// viewport was resized while zoomed in, where the old pan would leave
    /// the image offset outside it.
    pub fn clamp_pan(&mut self, size: (f64, f64)) {
        self.pan.0 = self.pan.0.clamp(size.0 * (1.0 - self.zoom), 0.0);
        self.pan.1 = self.pan.1.clamp(size.1 * (1.0 - self.zoom), 0.0);
    }
}

// The asserted values are exact. They come from arithmetic on round numbers,
// not from accumulated floating-point error.
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::View;

    const SIZE: (f64, f64) = (800.0, 600.0);

    #[test]
    fn zoom_keeps_focal_point_fixed() {
        let mut view = View::default();
        // The viewport point (400, 300) corresponds to stage point (400, 300).
        view.zoom_at((400.0, 300.0), 2.0, SIZE);
        assert_eq!(view.zoom, 2.0);
        // The same stage point must still appear at (400, 300).
        assert_eq!(view.pan.0 + view.zoom * 400.0, 400.0);
        assert_eq!(view.pan.1 + view.zoom * 300.0, 300.0);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut view = View::default();
        view.zoom_at((0.0, 0.0), 100.0, SIZE);
        assert_eq!(view.zoom, View::MAX_ZOOM);
        view.zoom_at((0.0, 0.0), 1e-9, SIZE);
        assert_eq!(view.zoom, View::MIN_ZOOM);
        assert_eq!(view.pan, (0.0, 0.0));
    }

    #[test]
    fn pan_cannot_reveal_space_outside_the_image() {
        let mut view = View::default();
        view.zoom_at((0.0, 0.0), 2.0, SIZE);
        view.pan_by((1e6, 1e6), SIZE);
        assert_eq!(view.pan, (0.0, 0.0));
        view.pan_by((-1e6, -1e6), SIZE);
        assert_eq!(view.pan, (-800.0, -600.0));
    }

    #[test]
    fn pan_is_noop_at_min_zoom() {
        let mut view = View::default();
        view.pan_by((50.0, -30.0), SIZE);
        assert_eq!(view.pan, (0.0, 0.0));
    }

    #[test]
    fn clamp_pan_pulls_image_back_after_resize() {
        let mut view = View::default();
        view.zoom_at((0.0, 0.0), 2.0, SIZE);
        view.pan_by((-1e6, -1e6), SIZE);
        assert_eq!(view.pan, (-800.0, -600.0));
        // The viewport shrank. The old pan now reveals space beyond the
        // bottom-right corner of the image until it is clamped again.
        view.clamp_pan((400.0, 300.0));
        assert_eq!(view.pan, (-400.0, -300.0));
    }

    #[test]
    fn pinch_moves_midpoint_with_fingers() {
        let mut view = View::default();
        view.zoom_at((0.0, 0.0), 4.0, SIZE);
        view.pan_by((-100.0, -100.0), SIZE);
        let before = view;
        // Pure two-finger drag (factor 1): pan follows the midpoint.
        view.pinch((400.0, 300.0), (390.0, 280.0), 1.0, SIZE);
        assert_eq!(view.pan.0, before.pan.0 - 10.0);
        assert_eq!(view.pan.1, before.pan.1 - 20.0);
    }
}

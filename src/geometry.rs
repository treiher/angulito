//! Angle math shared by the live overlay and the PNG export.

/// Angle at `vertex` between the rays towards `a` and `c`, in degrees (0..=180).
pub fn angle_deg(a: (f64, f64), vertex: (f64, f64), c: (f64, f64)) -> f64 {
    let v1 = (a.0 - vertex.0, a.1 - vertex.1);
    let v2 = (c.0 - vertex.0, c.1 - vertex.1);
    let angle = (v2.1.atan2(v2.0) - v1.1.atan2(v1.0)).to_degrees().abs();
    if angle > 180.0 { 360.0 - angle } else { angle }
}

/// Scales points normalized to 0..=1 in both axes so that angles computed
/// from them match the image they were placed on. `aspect` is the image width
/// divided by its height. Without the correction a non-square image would
/// skew every measured angle.
pub fn aspect_correct(points: [(f64, f64); 3], aspect: f64) -> [(f64, f64); 3] {
    points.map(|(x, y)| (x * aspect, y))
}

/// Position of the angle label for the points `p`, which are an end point,
/// the vertex and the other end point. The label sits `dist` from the vertex
/// along the angle bisector, inside the opening, and stays at least `margin`
/// away from the edges of an area of size `bounds`.
pub fn label_position(
    p: [(f64, f64); 3],
    dist: f64,
    margin: (f64, f64),
    bounds: (f64, f64),
) -> (f64, f64) {
    let dir = label_direction(p[0], p[1], p[2]);
    // `clamp` panics when the lower bound exceeds the upper one. That happens
    // once the area is narrower than twice the margin, so raise the upper
    // bound to the lower one in that case.
    (
        (p[1].0 + dir.0 * dist).clamp(margin.0, (bounds.0 - margin.0).max(margin.0)),
        (p[1].1 + dir.1 * dist).clamp(margin.1, (bounds.1 - margin.1).max(margin.1)),
    )
}

/// Unit vector from `vertex` towards where the angle label should be placed,
/// which is along the angle bisector, inside the opening.
fn label_direction(a: (f64, f64), vertex: (f64, f64), c: (f64, f64)) -> (f64, f64) {
    let u1 = unit_towards(vertex, a);
    let u2 = unit_towards(vertex, c);
    let bisector = (u1.0 + u2.0, u1.1 + u2.1);
    if bisector.0.hypot(bisector.1) < 1e-6 {
        // Straight line: the bisector is undefined, any perpendicular works.
        (-u1.1, u1.0)
    } else {
        normalize(bisector)
    }
}

/// Whether the wedge from the ray towards `a` to the ray towards `c` sweeps in
/// the positive-angle direction, which is clockwise on screen because the y
/// axis grows downwards. SVG's arc sweep-flag is 1 for that direction, and the
/// canvas `arc` call wants the negation as its `anticlockwise` argument. The
/// collinear case is a tie, so the choice is arbitrary, but it has to come out
/// the same for the live overlay and the PNG export.
pub fn wedge_is_clockwise(a: (f64, f64), vertex: (f64, f64), c: (f64, f64)) -> bool {
    let u1 = unit_towards(vertex, a);
    let u2 = unit_towards(vertex, c);
    u1.0 * u2.1 - u1.1 * u2.0 >= 0.0
}

/// Unit vector pointing from `from` towards `to`.
pub fn unit_towards(from: (f64, f64), to: (f64, f64)) -> (f64, f64) {
    normalize((to.0 - from.0, to.1 - from.1))
}

fn normalize(v: (f64, f64)) -> (f64, f64) {
    let len = v.0.hypot(v.1);
    if len == 0.0 {
        (0.0, -1.0)
    } else {
        (v.0 / len, v.1 / len)
    }
}

#[cfg(test)]
mod tests {
    use super::{angle_deg, aspect_correct, label_direction, label_position, wedge_is_clockwise};

    #[test]
    fn right_angle() {
        assert!((angle_deg((1.0, 0.0), (0.0, 0.0), (0.0, 1.0)) - 90.0).abs() < 1e-9);
    }

    #[test]
    fn straight_line() {
        assert!((angle_deg((-1.0, 0.0), (0.0, 0.0), (1.0, 0.0)) - 180.0).abs() < 1e-9);
    }

    #[test]
    fn reflex_is_mirrored_into_range() {
        let a = angle_deg((1.0, 0.0), (0.0, 0.0), (-1.0, -1.0));
        assert!((a - 135.0).abs() < 1e-9);
    }

    #[test]
    fn zero_angle() {
        assert!(angle_deg((1.0, 1.0), (0.0, 0.0), (2.0, 2.0)).abs() < 1e-9);
    }

    fn assert_close(actual: (f64, f64), expected: (f64, f64)) {
        assert!(
            (actual.0 - expected.0).abs() < 1e-9 && (actual.1 - expected.1).abs() < 1e-9,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn aspect_correction_scales_only_the_x_axis() {
        let p = aspect_correct([(0.25, 0.75), (0.5, 0.35), (0.75, 0.75)], 4.0 / 3.0);
        assert_close(p[0], (0.25 * 4.0 / 3.0, 0.75));
        assert_close(p[1], (0.5 * 4.0 / 3.0, 0.35));
        assert_close(p[2], (1.0, 0.75));
    }

    #[test]
    fn aspect_correction_changes_the_measured_angle() {
        let [a, vertex, c] = aspect_correct([(0.25, 0.75), (0.5, 0.35), (0.75, 0.75)], 4.0 / 3.0);
        // The default points on a 4:3 frame measure 79.6°, which the app
        // displays rounded as 80°.
        let angle = angle_deg(a, vertex, c);
        assert!((angle - 79.61).abs() < 0.01, "got {angle}");
    }

    #[test]
    fn label_inside_wide_angle() {
        // 90° opening towards +x/+y: label goes into the opening.
        let dir = label_direction((1.0, 0.0), (0.0, 0.0), (0.0, 1.0));
        let d = std::f64::consts::FRAC_1_SQRT_2;
        assert_close(dir, (d, d));
    }

    #[test]
    fn label_inside_narrow_angle() {
        // 2° opening around +x: label stays inside the opening.
        let a = (1.0, 0.017_455);
        let c = (1.0, -0.017_455);
        let dir = label_direction(a, (0.0, 0.0), c);
        assert!(dir.0 > 0.99, "got {dir:?}");
    }

    #[test]
    fn label_perpendicular_for_straight_line() {
        let dir = label_direction((-1.0, 0.0), (0.0, 0.0), (1.0, 0.0));
        assert!(dir.0.abs() < 1e-9 && dir.1.abs() > 0.99, "got {dir:?}");
    }

    #[test]
    fn label_sits_on_the_bisector_at_the_given_distance() {
        // 90° opening towards +x/+y, far enough from every edge to be left
        // where the bisector puts it.
        let p = [(100.0, 0.0), (0.0, 0.0), (0.0, 100.0)];
        let pos = label_position(p, 10.0, (5.0, 5.0), (1000.0, 1000.0));
        let d = 10.0 * std::f64::consts::FRAC_1_SQRT_2;
        assert_close(pos, (d, d));
    }

    #[test]
    fn label_is_pulled_back_to_the_margins() {
        // Vertex in a corner with the opening pointing out of the area.
        let p = [(0.0, -100.0), (0.0, 0.0), (-100.0, 0.0)];
        let pos = label_position(p, 50.0, (24.0, 14.0), (200.0, 100.0));
        assert_close(pos, (24.0, 14.0));
    }

    #[test]
    fn label_near_an_edge_stops_at_a_zoomed_margin() {
        // The editor shrinks the distance and the margins together as the
        // user zooms in, here by a factor of 8. A vertex on the left edge
        // with the bisector pointing further left has to end up exactly on
        // the shrunken margin, not on some larger fixed inset.
        let p = [(0.0, 200.0), (0.0, 300.0), (-100.0, 300.0)];
        let pos = label_position(p, 64.0 / 8.0, (24.0 / 8.0, 14.0 / 8.0), (800.0, 600.0));
        let dy = 8.0 * std::f64::consts::FRAC_1_SQRT_2;
        assert_close(pos, (3.0, 300.0 - dy));
    }

    #[test]
    fn collinear_points_pick_a_definite_sweep() {
        // Dragging or nudging all three handles onto the bottom edge clamps
        // them to y = 1.0, which makes the cross product exactly zero. Both
        // the live overlay and the PNG export read the direction from here, so
        // the wedge ends up on the same side in both.
        assert!(wedge_is_clockwise((0.25, 1.0), (0.5, 1.0), (0.75, 1.0)));
    }

    #[test]
    fn sweep_follows_the_orientation_of_the_rays() {
        // 90° opening towards +x/+y. Going from +x to +y is the positive-angle
        // direction with the y axis pointing down.
        assert!(wedge_is_clockwise((1.0, 0.0), (0.0, 0.0), (0.0, 1.0)));
        assert!(!wedge_is_clockwise((0.0, 1.0), (0.0, 0.0), (1.0, 0.0)));
    }

    #[test]
    fn label_survives_an_area_narrower_than_its_margins() {
        // The clamp bounds would come out inverted, which would panic.
        let p = [(1.0, 0.0), (0.0, 0.0), (0.0, 1.0)];
        let pos = label_position(p, 64.0, (24.0, 14.0), (10.0, 10.0));
        assert_close(pos, (24.0, 14.0));
    }
}

use maploom::{document::Point, geometry, render};
fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<Vec<Point>> {
    vec![vec![
        Point::new(x, y),
        Point::new(x + w, y),
        Point::new(x + w, y + h),
        Point::new(x, y + h),
    ]]
}
#[test]
fn overlapping_rooms_have_one_outline_and_no_internal_wall() {
    let rings = geometry::union_rooms(&[rect(0., 0., 4., 4.), rect(2., 2., 4., 4.)]).unwrap();
    assert_eq!(rings.len(), 1);
    assert_eq!(rings[0].len(), 8);
    assert!((geometry::area(&rings[0]) - 28.).abs() < 0.001);
    assert!(!rings[0]
        .windows(2)
        .any(|w| w[0].x == 2. && w[1].x == 2. && w[0].y == 2. && w[1].y == 4.));
}
#[test]
fn shared_edges_merge_without_overlap_area() {
    let rings = geometry::union_rooms(&[rect(0., 0., 4., 4.), rect(4., 0., 3., 4.)]).unwrap();
    assert_eq!(rings.len(), 1);
    assert_eq!(rings[0].len(), 4);
    assert_eq!(geometry::area(&rings[0]), 28.);
}
fn circle(x: f32, y: f32, radius: f32) -> Vec<Vec<Point>> {
    vec![(0..512)
        .map(|n| {
            let angle = n as f32 * std::f32::consts::TAU / 512.;
            Point::new(x + radius * angle.cos(), y + radius * angle.sin())
        })
        .collect()]
}
#[test]
fn circular_and_rectangular_rooms_keep_their_curved_outer_boundary() {
    let a = rect(0., 0., 4., 4.);
    let b = circle(4., 2., 2.);
    let expected = 16. + geometry::area(&b[0]) / 2.;
    for regions in [vec![a.clone(), b.clone()], vec![b.clone(), a.clone()]] {
        let rings = geometry::union_rooms(&regions).unwrap();
        assert_eq!(rings.len(), 1);
        assert!((geometry::area(&rings[0]) - expected).abs() < 0.001);
        assert!(rings[0].len() > 250);
        // Every remaining wall faces the exterior, including the arc.
        for (n, p) in rings[0].iter().enumerate() {
            let q = rings[0][(n + 1) % rings[0].len()];
            let midpoint = Point::new((p.x + q.x) / 2., (p.y + q.y) / 2.);
            assert!(
                !(midpoint.x > 0.01 && midpoint.x < 3.99 && midpoint.y > 0.01 && midpoint.y < 3.99)
            );
        }
        let filled: f32 = render::triangulate(&rings[0])
            .iter()
            .map(|t| geometry::area(&[rings[0][t[0]], rings[0][t[1]], rings[0][t[2]]]).abs())
            .sum();
        assert!((filled - expected).abs() < 0.001);
    }
}
#[test]
fn overlapping_circles_union_without_an_internal_arc() {
    let rings = geometry::union_rooms(&[circle(0., 0., 2.), circle(2., 0., 2.)]).unwrap();
    assert_eq!(rings.len(), 1);
    let expected = 8. * std::f32::consts::PI - (8. * std::f32::consts::PI / 3. - 2. * 3f32.sqrt());
    assert!((geometry::area(&rings[0]) - expected).abs() < 0.002);
    assert!(geometry::inside(&rings, Point::new(1., 0.)));
}
#[test]
fn crossing_diagonal_edges_are_split_at_their_actual_intersections() {
    let diamond = vec![vec![
        Point::new(0., -2.),
        Point::new(2., 0.),
        Point::new(0., 2.),
        Point::new(-2., 0.),
    ]];
    let rings = geometry::union_rooms(&[diamond, rect(-3., -0.5, 6., 1.)]).unwrap();
    assert_eq!(rings.len(), 1);
    assert!((geometry::area(&rings[0]) - 10.5).abs() < 0.001);
    assert!(rings[0]
        .iter()
        .any(|p| (p.x - 1.5).abs() < 0.00001 && (p.y - 0.5).abs() < 0.00001));
}
#[test]
fn merging_a_curved_room_preserves_uncovered_holes() {
    let mut a = rect(0., 0., 8., 8.);
    let mut hole = rect(2., 2., 2., 2.).remove(0);
    hole.reverse();
    a.push(hole);
    let rings = geometry::union_rooms(&[a, circle(8., 4., 2.)]).unwrap();
    assert_eq!(rings.len(), 2);
    assert_eq!(rings.iter().filter(|r| geometry::area(r) < 0.).count(), 1);
    assert!(!geometry::inside(&rings, Point::new(3., 3.)));
    assert!(geometry::inside(&rings, Point::new(9., 4.)));
}
#[test]
fn separate_rooms_remain_separate() {
    let a = rect(0., 0., 2., 2.);
    let b = rect(4., 0., 2., 2.);
    assert!(!geometry::touches(&a, &b));
    let rings = geometry::union_rooms(&[a, b]).unwrap();
    assert_eq!(rings.len(), 2);
}
#[test]
fn courtyard_union_preserves_the_hole() {
    let rings = geometry::union_rooms(&[
        rect(0., 0., 8., 2.),
        rect(0., 6., 8., 2.),
        rect(0., 2., 2., 4.),
        rect(6., 2., 2., 4.),
    ])
    .unwrap();
    assert_eq!(rings.len(), 2);
    assert!(!geometry::inside(&rings, Point::new(4., 4.)));
    assert!(geometry::inside(&rings, Point::new(1., 4.)));
    let polygons = geometry::fill_polygons(&rings);
    let mut area = 0.;
    for p in polygons {
        for t in render::triangulate(&p) {
            area += geometry::area(&[p[t[0]], p[t[1]], p[t[2]]]).abs();
        }
    }
    assert!((area - 48.).abs() < 0.001, "area={area}");
}
#[test]
fn terrain_stamps_form_a_connected_flexible_outline() {
    let rings = geometry::terrain_contours(&[(Point::new(0., 0.), 1.), (Point::new(1.5, 0.), 1.)]);
    assert_eq!(rings.len(), 1);
    assert!(rings[0].len() > 12);
    assert!(geometry::inside(&rings, Point::new(0., 0.)));
    assert!(geometry::inside(&rings, Point::new(1.5, 0.)));
    assert!(!geometry::inside(&rings, Point::new(0.75, 2.)));
}
#[test]
fn disconnected_terrain_regions_are_not_joined() {
    let rings = geometry::terrain_contours(&[(Point::new(0., 0.), 1.), (Point::new(5., 0.), 1.)]);
    assert_eq!(rings.len(), 2);
}

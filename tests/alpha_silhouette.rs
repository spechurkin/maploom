use image::{Rgba, RgbaImage};
use maploom::{
    assets::{alpha_boundary, alpha_silhouette},
    document::Point,
};

#[test]
fn light_boundary_preserves_padding_concavities_holes_and_separate_parts() {
    let mut image = RgbaImage::new(8, 8);
    for y in 1..7 {
        for x in 1..7 {
            // An L-shaped cutout, a hole, and an isolated translucent pixel.
            if !(x >= 4 && y <= 3 || x == 2 && y == 4) {
                image.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
    }
    image.put_pixel(7, 0, Rgba([0, 0, 0, 1]));
    let edges = alpha_boundary(&image);
    // Compare every edge midpoint to its adjacent cells: the boundary must
    // separate transparent and occupied pixels, never bridge empty space.
    for [a, b] in &edges {
        let mid = Point::new((a.x + b.x) * 4., (a.y + b.y) * 4.);
        let vertical = a.x == b.x;
        let occupied = |side: f32| {
            let x = (mid.x + if vertical { side } else { 0. }).floor() as i32;
            let y = (mid.y + if vertical { 0. } else { side }).floor() as i32;
            (0..8).contains(&x) && (0..8).contains(&y) && image.get_pixel(x as u32, y as u32)[3] > 0
        };
        assert_ne!(occupied(-0.25), occupied(0.25), "{a:?}..{b:?}");
    }
    let length: f32 = edges.iter().map(|[a, b]| a.distance(*b) * 8.).sum();
    assert_eq!(length, 32.); // outer L: 24, hole: 4, isolated pixel: 4.
    assert!(alpha_boundary(&RgbaImage::new(8, 8)).is_empty());
    assert_eq!(
        alpha_boundary(&RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255]))).len(),
        4
    );
}

#[test]
fn alpha_outline_ignores_transparent_padding_and_retains_pixel_edges() {
    let mut image = RgbaImage::new(100, 80);
    for y in 20..60 {
        for x in 30..70 {
            image.put_pixel(x, y, Rgba([40, 80, 20, 255]));
        }
    }
    assert_eq!(
        alpha_silhouette(&image),
        vec![
            Point::new(0.3, 0.25),
            Point::new(0.7, 0.25),
            Point::new(0.7, 0.75),
            Point::new(0.3, 0.75)
        ]
    );
}

#[test]
fn outline_preserves_diagonal_support_and_translucent_pixels() {
    let mut image = RgbaImage::new(10, 10);
    image.put_pixel(1, 1, Rgba([0, 0, 0, 1]));
    image.put_pixel(8, 8, Rgba([255, 255, 255, 255]));
    let hull = alpha_silhouette(&image);
    assert_eq!(hull.len(), 6);
    assert!(!hull.contains(&Point::new(0.9, 0.1)));
    assert!(hull.contains(&Point::new(0.1, 0.1)));
    assert!(hull.contains(&Point::new(0.9, 0.9)));
}

#[test]
fn empty_and_opaque_images_have_unambiguous_outlines() {
    assert!(alpha_silhouette(&RgbaImage::new(10, 10)).is_empty());
    assert!(alpha_silhouette(&RgbaImage::new(0, 0)).is_empty());
    let hull = alpha_silhouette(&RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255])));
    assert_eq!(
        hull,
        vec![
            Point::new(0., 0.),
            Point::new(1., 0.),
            Point::new(1., 1.),
            Point::new(0., 1.)
        ]
    );
}

//! Curved paper geometry and a deferred-commit turn state machine.
use eframe::egui::{
    self,
    epaint::{Mesh, Vertex},
    Color32, Pos2, Rect, Vec2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PagePosition {
    pub chapter: usize,
    pub page: usize,
}

pub(crate) fn destination(
    from: PagePosition,
    pages: usize,
    previous_pages: usize,
    chapters: usize,
    dual: bool,
    forward: bool,
) -> Option<PagePosition> {
    let step = if dual { 2 } else { 1 };
    if forward {
        if from.page + step < pages {
            Some(PagePosition {
                page: from.page + step,
                ..from
            })
        } else if from.chapter + 1 < chapters {
            Some(PagePosition {
                chapter: from.chapter + 1,
                page: 0,
            })
        } else {
            None
        }
    } else if from.page >= step {
        Some(PagePosition {
            page: from.page - step,
            ..from
        })
    } else if from.chapter > 0 {
        Some(PagePosition {
            chapter: from.chapter - 1,
            page: (previous_pages.max(1) - 1) / step * step,
        })
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Motion {
    Drag {
        origin: Pos2,
        last: Pos2,
        moved: bool,
        base_progress: f32,
        base_tilt: f32,
    },
    Settle {
        from: f32,
        to: f32,
        start: f64,
        duration: f64,
    },
}
impl Motion {
    pub fn settle(progress: f32, commit: bool, now: f64, speed: f32) -> Self {
        let to = if commit { 1. } else { 0. };
        let full_duration = (0.06 / speed.clamp(0.04, 0.4)).clamp(0.20, 1.5);
        Self::Settle {
            from: progress,
            to,
            start: now,
            duration: (full_duration * (to - progress).abs()) as f64,
        }
    }
    /// Returns a final decision only when settling has reached its endpoint.
    pub fn sample(self, now: f64) -> (f32, Option<bool>) {
        if let Self::Settle {
            from,
            to,
            start,
            duration,
        } = self
        {
            let t = if duration <= 0.001 {
                1.
            } else {
                ((now - start) / duration).clamp(0., 1.) as f32
            };
            let eased = t * t * (3. - 2. * t);
            (from + (to - from) * eased, (t >= 1.).then_some(to == 1.))
        } else {
            (0., None)
        }
    }
}

pub(crate) fn release_commits(progress: f32, moved: bool, velocity: f32) -> bool {
    !moved || progress >= 0.35 || (progress >= 0.08 && velocity > 0.65)
}

#[derive(Clone, Copy)]
pub(crate) struct CurlGeometry {
    pub rect: Rect,
    pub progress: f32,
    pub tilt: f32,
    pub anchor_y: f32,
}
impl CurlGeometry {
    pub fn shadow_point(self, y: f32) -> Pos2 {
        let (n, fold, radius) = self.parameters();
        let local_y = y - self.rect.top() - self.anchor_y;
        let x = (fold + radius * std::f32::consts::FRAC_PI_2 - local_y * n.y) / n.x;
        self.project(egui::pos2(self.rect.left() + x, y), false).0
    }
    fn parameters(self) -> (Vec2, f32, f32) {
        let t = self.progress.clamp(0., 1.);
        let bend = (std::f32::consts::PI * t).sin().max(0.);
        let n = Vec2::new(1., self.tilt * bend).normalized();
        (
            n,
            self.rect.width() * (1. - t) * n.x,
            self.rect.width() * 0.12 * bend,
        )
    }
    pub fn face_distance(self, point: Pos2, back_coordinates: bool) -> f32 {
        let (n, fold, radius) = self.parameters();
        let mut x = point.x - self.rect.left();
        if back_coordinates {
            x = self.rect.width() - x;
        }
        let y = point.y - self.rect.top() - self.anchor_y;
        x * n.x + y * n.y - fold - radius * std::f32::consts::FRAC_PI_2
    }
    pub fn project(self, point: Pos2, back_coordinates: bool) -> (Pos2, f32) {
        let (n, fold, radius) = self.parameters();
        let mut x = point.x - self.rect.left();
        if back_coordinates {
            x = self.rect.width() - x;
        }
        let y = point.y - self.rect.top() - self.anchor_y;
        let u = x * n.x + y * n.y;
        let v = -x * n.y + y * n.x;
        let (projected, z, shade) = if u <= fold {
            (u, 0., 1.)
        } else if radius < 0.001 {
            (2. * fold - u, 0., 1.)
        } else {
            let angle = (u - fold) / radius;
            if angle < std::f32::consts::PI {
                (
                    fold + radius * angle.sin(),
                    radius * (1. - angle.cos()),
                    0.82 + 0.18 * angle.cos().abs(),
                )
            } else {
                (
                    2. * fold + std::f32::consts::PI * radius - u,
                    2. * radius,
                    1.,
                )
            }
        };
        let scale = 1. + z / (self.rect.width() * 6.);
        let px = (n.x * projected - n.y * v) * scale;
        let py = (n.y * projected + n.x * v) * scale;
        (
            egui::pos2(self.rect.left() + px, self.rect.top() + self.anchor_y + py),
            shade,
        )
    }
}

fn interpolate(a: Vertex, b: Vertex, t: f32) -> Vertex {
    let ca = a.color.to_array();
    let cb = b.color.to_array();
    let c: [u8; 4] =
        std::array::from_fn(|i| (ca[i] as f32 + (cb[i] as f32 - ca[i] as f32) * t).round() as u8);
    Vertex {
        pos: a.pos + (b.pos - a.pos) * t,
        uv: a.uv + (b.uv - a.uv) * t,
        color: Color32::from_rgba_premultiplied(c[0], c[1], c[2], c[3]),
    }
}
fn clip(poly: Vec<Vertex>, distance: impl Fn(Pos2) -> f32) -> Vec<Vertex> {
    if poly.is_empty() {
        return poly;
    }
    let mut out = Vec::with_capacity(poly.len() + 2);
    let mut previous = *poly.last().unwrap();
    let mut dp = distance(previous.pos);
    for current in poly {
        let dc = distance(current.pos);
        if (dp >= 0.) != (dc >= 0.) {
            out.push(interpolate(previous, current, dp / (dp - dc)));
        }
        if dc >= 0. {
            out.push(current);
        }
        previous = current;
        dp = dc;
    }
    out
}

/// Clip before bending: page bounds and front/back visibility remain exact, including glyph UVs.
pub(crate) fn bend_mesh(
    source: &Mesh,
    source_clip: Rect,
    geometry: CurlGeometry,
    back: bool,
    shade_factor: f32,
) -> Mesh {
    let mut out = Mesh::with_texture(source.texture_id);
    for tri in source.indices.as_chunks::<3>().0 {
        let mut poly = tri.iter().map(|&i| source.vertices[i as usize]).collect();
        poly = clip(poly, |p| p.x - source_clip.left());
        poly = clip(poly, |p| source_clip.right() - p.x);
        poly = clip(poly, |p| p.y - source_clip.top());
        poly = clip(poly, |p| source_clip.bottom() - p.y);
        poly = clip(poly, |p| {
            geometry.face_distance(p, back) * if back { 1. } else { -1. }
        });
        if poly.len() < 3 {
            continue;
        }
        let base = out.vertices.len() as u32;
        for mut vertex in poly.iter().copied() {
            let (pos, shade) = geometry.project(vertex.pos, back);
            vertex.pos = pos;
            let factor = (shade * shade_factor).clamp(0., 1.);
            let c = vertex.color.to_array();
            vertex.color = Color32::from_rgba_premultiplied(
                (c[0] as f32 * factor).round() as u8,
                (c[1] as f32 * factor).round() as u8,
                (c[2] as f32 * factor).round() as u8,
                c[3],
            );
            out.vertices.push(vertex);
        }
        for i in 1..poly.len() - 1 {
            out.indices
                .extend([base, base + i as u32, base + i as u32 + 1]);
        }
    }
    out
}

pub(crate) fn paper_mesh(rect: Rect, color: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    let nx = 64;
    let ny = 12;
    for y in 0..=ny {
        for x in 0..=nx {
            mesh.vertices.push(Vertex {
                pos: egui::pos2(
                    egui::lerp(rect.x_range(), x as f32 / nx as f32),
                    egui::lerp(rect.y_range(), y as f32 / ny as f32),
                ),
                uv: egui::epaint::WHITE_UV,
                color,
            });
        }
    }
    for y in 0..ny {
        for x in 0..nx {
            let i = (y * (nx + 1) + x) as u32;
            let row = (nx + 1) as u32;
            mesh.indices
                .extend([i, i + 1, i + row, i + 1, i + row + 1, i + row]);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect() -> Rect {
        Rect::from_min_size(egui::pos2(0., 0.), egui::vec2(600., 800.))
    }
    #[test]
    fn turn_boundaries_and_odd_spreads() {
        let p = PagePosition {
            chapter: 1,
            page: 0,
        };
        assert_eq!(
            destination(p, 4, 5, 3, true, false),
            Some(PagePosition {
                chapter: 0,
                page: 4
            })
        );
        assert_eq!(
            destination(PagePosition { page: 2, ..p }, 3, 0, 3, true, true),
            Some(PagePosition {
                chapter: 2,
                page: 0
            })
        );
        assert_eq!(
            destination(
                PagePosition {
                    chapter: 0,
                    page: 0
                },
                1,
                0,
                1,
                false,
                false
            ),
            None
        );
        assert_eq!(
            destination(
                PagePosition {
                    chapter: 0,
                    page: 0
                },
                1,
                0,
                1,
                true,
                true
            ),
            None
        );
        assert_eq!(
            destination(p, 1, 0, 3, false, false),
            Some(PagePosition {
                chapter: 0,
                page: 0
            })
        );
        assert_eq!(
            destination(PagePosition { page: 2, ..p }, 5, 0, 3, true, true),
            Some(PagePosition { page: 4, ..p })
        );
    }
    #[test]
    fn settle_defers_commit_and_is_time_based() {
        let m = Motion::settle(0.2, true, 10., 0.14);
        assert_eq!(m.sample(10.), (0.2, None));
        assert_eq!(m.sample(11.), (1., Some(true)));
        let m = Motion::settle(0.2, false, 10., 0.14);
        assert_eq!(m.sample(11.), (0., Some(false)));
        assert!(!release_commits(0.1, true, 0.));
        assert!(release_commits(0., false, 0.));
        assert!(release_commits(0.5, true, -2.));
        assert!(release_commits(0.2, true, 1.));
    }
    #[test]
    fn geometry_has_exact_flat_endpoints_for_both_faces() {
        let front = CurlGeometry {
            rect: rect(),
            progress: 0.,
            tilt: 0.3,
            anchor_y: 800.,
        };
        let end = CurlGeometry {
            progress: 1.,
            ..front
        };
        for p in [
            egui::pos2(0., 0.),
            egui::pos2(600., 800.),
            egui::pos2(150., 340.),
        ] {
            assert!((front.project(p, false).0 - p).length() < 0.001);
            let target = egui::pos2(p.x - 600., p.y);
            assert!((end.project(p, true).0 - target).length() < 0.01);
        }
    }
    #[test]
    fn curved_meshes_are_finite_and_faces_cover_paper() {
        let r = rect();
        let paper = paper_mesh(r, Color32::WHITE);
        for tilt in [-0.4, 0., 0.4] {
            for t in [0., 0.01, 0.2, 0.5, 0.8, 0.99, 1.] {
                let g = CurlGeometry {
                    rect: r,
                    progress: t,
                    tilt,
                    anchor_y: 700.,
                };
                let f = bend_mesh(&paper, r, g, false, 1.);
                let b = bend_mesh(&paper, r, g, true, 1.);
                assert!(f.is_valid() && b.is_valid());
                assert!(f
                    .vertices
                    .iter()
                    .chain(&b.vertices)
                    .all(|v| v.color.a() == 255));
                assert!(!f.is_empty() || !b.is_empty());
                assert!(f
                    .vertices
                    .iter()
                    .chain(&b.vertices)
                    .all(|v| v.pos.x.is_finite() && v.pos.y.is_finite()));
            }
        }
    }
    #[test]
    fn clipping_preserves_uvs_and_bounds() {
        let r = rect();
        let paper = paper_mesh(r, Color32::WHITE);
        let clip = Rect::from_min_max(egui::pos2(100., 100.), egui::pos2(300., 400.));
        let g = CurlGeometry {
            rect: r,
            progress: 0.,
            tilt: 0.,
            anchor_y: 0.,
        };
        let m = bend_mesh(&paper, clip, g, false, 1.);
        assert!(!m.is_empty());
        assert!(m
            .vertices
            .iter()
            .all(|v| clip.expand(0.001).contains(v.pos)));
    }
}

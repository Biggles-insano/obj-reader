use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Instant;

use minifb::{Key, Window, WindowOptions};

const WIDTH: usize = 800;
const HEIGHT: usize = 600;

/* ==== Estructuras básicas ==== */
#[derive(Clone, Copy, Debug)]
pub struct Vec3(pub f32, pub f32, pub f32);
#[derive(Clone, Copy, Debug)]
pub struct Vec2(pub f32, pub f32);

#[derive(Debug, Clone)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    pub texcoords: Vec<Vec2>,
    pub normals:   Vec<Vec3>,
    // índices triangulados: (v_idx, vt_idx?, vn_idx?)
    pub indices:   Vec<(u32, Option<u32>, Option<u32>)>,
}

impl Mesh {
    pub fn new() -> Self {
        Self { positions: vec![], texcoords: vec![], normals: vec![], indices: vec![] }
    }
}

/* ==== Lector OBJ mínimo (v, vt, vn, f) ==== */
fn resolve_idx(obj_idx: i32, len: usize) -> Option<u32> {
    if len == 0 { return None; }
    let idx0 = if obj_idx > 0 {(obj_idx - 1) as isize} else {(len as isize) + (obj_idx as isize)};
    if idx0 < 0 || (idx0 as usize) >= len { return None; }
    Some(idx0 as u32)
}

fn parse_face_vertex(token: &str, vlen: usize, vtlen: usize, vnlen: usize)
    -> Option<(u32, Option<u32>, Option<u32>)>
{
    let parts: Vec<&str> = token.split('/').collect();
    let v  = resolve_idx(parts.get(0)?.parse::<i32>().ok()?, vlen)?;
    let vt = if parts.len() >= 2 && !parts[1].is_empty() {
        resolve_idx(parts[1].parse::<i32>().ok()?, vtlen)
    } else { None };
    let vn = if parts.len() >= 3 && !parts[2].is_empty() {
        resolve_idx(parts[2].parse::<i32>().ok()?, vnlen)
    } else { None };
    Some((v, vt, vn))
}

fn triangulate_fan<T: Copy>(poly: &[T]) -> Vec<[T;3]> {
    let mut tris = Vec::new();
    for i in 2..poly.len() { tris.push([poly[0], poly[i-1], poly[i]]); }
    tris
}

fn load_obj<P: AsRef<Path>>(path: P) -> Result<Mesh, String> {
    let file = File::open(path.as_ref()).map_err(|e| format!("No se pudo abrir: {e}"))?;
    let reader = BufReader::new(file);

    let mut mesh = Mesh::new();

    for (lineno, line_res) in reader.lines().enumerate() {
        let line = line_res.map_err(|e| format!("Error L{}: {e}", lineno+1))?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }

        let mut it = line.split_whitespace();
        let tag = it.next().unwrap_or_default();

        match tag {
            "v" => {
                let xs: Vec<&str> = it.collect();
                if xs.len() < 3 { return Err(format!("v inválido L{}", lineno+1)); }
                mesh.positions.push(Vec3(xs[0].parse().unwrap(), xs[1].parse().unwrap(), xs[2].parse().unwrap()));
            }
            "vt" => {
                let xs: Vec<&str> = it.collect();
                if xs.len() < 2 { return Err(format!("vt inválido L{}", lineno+1)); }
                mesh.texcoords.push(Vec2(xs[0].parse().unwrap(), xs[1].parse().unwrap()));
            }
            "vn" => {
                let xs: Vec<&str> = it.collect();
                if xs.len() < 3 { return Err(format!("vn inválido L{}", lineno+1)); }
                mesh.normals.push(Vec3(xs[0].parse().unwrap(), xs[1].parse().unwrap(), xs[2].parse().unwrap()));
            }
            "f" => {
                let face_tokens: Vec<String> = it.map(|s| s.to_string()).collect();
                if face_tokens.len() < 3 { return Err(format!("f < 3 vértices L{}", lineno+1)); }
                let mut poly: Vec<(u32, Option<u32>, Option<u32>)> = Vec::new();
                for t in &face_tokens {
                    poly.push(parse_face_vertex(t, mesh.positions.len(), mesh.texcoords.len(), mesh.normals.len())
                        .ok_or_else(|| format!("Índice inválido L{}", lineno+1))?);
                }
                for tri in triangulate_fan(&poly) {
                    mesh.indices.push(tri[0]);
                    mesh.indices.push(tri[1]);
                    mesh.indices.push(tri[2]);
                }
            }
            _ => {}
        }
    }

    Ok(mesh)
}

/* ==== Utilidades matemáticas y proyección ==== */
fn center_and_scale_to_unit(positions: &[Vec3]) -> (Vec<Vec3>, f32, f32, f32) {
    // bbox
    let (mut minx, mut miny, mut minz) = (f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let (mut maxx, mut maxy, mut maxz) = (f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &Vec3(x,y,z) in positions {
        if x<minx {minx=x} if y<miny {miny=y} if z<minz {minz=z}
        if x>maxx {maxx=x} if y>maxy {maxy=y} if z>maxz {maxz=z}
    }
    let cx = 0.5*(minx+maxx); let cy = 0.5*(miny+maxy); let cz = 0.5*(minz+maxz);
    let sx = maxx-minx; let sy = maxy-miny; let sz = maxz-minz;
    let max_extent = sx.max(sy).max(sz).max(1e-6);
    // quepa razonable en perspectiva
    let s = 2.0 / max_extent;

    let out = positions.iter().map(|&Vec3(x,y,z)| Vec3((x-cx)*s, (y-cy)*s, (z-cz)*s)).collect();
    (out, cx, cy, s)
}

fn project_perspective_to_screen(
    pts: &[Vec3],
    angle_y: f32,   // yaw
    angle_x: f32,   // pitch
    fov_deg: f32,
    cam_dist: f32,
) -> (Vec<(f32,f32)>, Vec<f32>) {
    let (cw, ch) = (WIDTH as f32, HEIGHT as f32);
    let half_min = 0.5 * cw.min(ch);
    let f = 1.0 / (0.5 * fov_deg.to_radians()).tan();

    let (cy, sy) = (angle_y.cos(), angle_y.sin());
    let (cx, sx) = (angle_x.cos(), angle_x.sin());

    let mut out = Vec::with_capacity(pts.len());
    let mut depths = Vec::with_capacity(pts.len());

    for &Vec3(x,y,z) in pts {
        // Rotación en Y (yaw)
        let xr = x*cy + z*sy;
        let yr = y;
        let zr = -x*sy + z*cy;

        // Rotación en X (pitch) sobre el resultado anterior
        let xrx = xr;
        let yrx = yr*cx - zr*sx;
        let zrx = yr*sx + zr*cx;

        // Traslación hacia cámara (cámara en origen mirando +Z)
        let zc = zrx + cam_dist; // > 0
        depths.push(zc);

        // Proyección perspectiva
        let px = (xrx * f) / zc;
        let py = (yrx * f) / zc;

        // A coordenadas de pantalla
        let sx = px * half_min + cw*0.5;
        let sy = -py * half_min + ch*0.5;
        out.push((sx, sy));
    }
    (out, depths)
}

/* ==== Framebuffer con z-buffer y ventana ==== */
struct Frame {
    w: usize,
    h: usize,
    color: Vec<u32>, // 0x00RRGGBB
    depth: Vec<f32>, // z-buffer (menor = más cerca)
}

impl Frame {
    fn new(w: usize, h: usize) -> Self {
        Self { w, h, color: vec![0x101014; w*h], depth: vec![f32::INFINITY; w*h] }
    }
    fn clear(&mut self, rgb: u32) {
        self.color.fill(rgb);
        self.depth.fill(f32::INFINITY);
    }
    #[inline]
    fn put_pixel_z(&mut self, x: i32, y: i32, z: f32, rgb: u32) {
        if x<0 || y<0 {return;}
        let (x, y) = (x as usize, y as usize);
        if x>=self.w || y>=self.h {return;}
        let idx = y*self.w + x;
        if z < self.depth[idx] {
            self.depth[idx] = z;
            self.color[idx] = rgb;
        }
    }
}

/* ==== Raster de triángulo con z (bary) ==== */
#[inline] fn edge(ax:f32, ay:f32, bx:f32, by:f32, px:f32, py:f32) -> f32 {
    (px-ax)*(by-ay) - (py-ay)*(bx-ax)
}

// v: (x,y,z_cam) – z_cam para z-buffer
fn fill_triangle_z(
    fb: &mut Frame,
    v0: (f32,f32,f32),
    v1: (f32,f32,f32),
    v2: (f32,f32,f32),
    rgb: u32,
) {
    let (x0,y0,z0) = v0; let (x1,y1,z1) = v1; let (x2,y2,z2) = v2;

    let min_x = x0.min(x1).min(x2).floor().max(0.0) as i32;
    let max_x = x0.max(x1).max(x2).ceil().min((fb.w-1) as f32) as i32;
    let min_y = y0.min(y1).min(y2).floor().max(0.0) as i32;
    let max_y = y0.max(y1).max(y2).ceil().min((fb.h-1) as f32) as i32;

    let area = edge(x0,y0, x1,y1, x2,y2);
    if area == 0.0 { return; }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            let w0 = edge(x1,y1, x2,y2, px,py);
            let w1 = edge(x2,y2, x0,y0, px,py);
            let w2 = edge(x0,y0, x1,y1, px,py);

            if (w0>=0.0 && w1>=0.0 && w2>=0.0 && area>0.0) ||
               (w0<=0.0 && w1<=0.0 && w2<=0.0 && area<0.0) {

                // bary normalizadas
                let b0 = w0/area;
                let b1 = w1/area;
                let b2 = w2/area;

                // z_cam interpolada (correcto para z-buffer)
                let z = b0*z0 + b1*z1 + b2*z2;

                fb.put_pixel_z(x, y, z, rgb);
            }
        }
    }
}

/* ==== App: ventana + loop ==== */
fn main() {
    let obj_path = "tie.obj";
    if !Path::new(obj_path).exists() {
        eprintln!("No se encontró '{}'. Colócalo en la raíz del proyecto.", obj_path);
        std::process::exit(1);
    }
    let mesh = load_obj(obj_path).expect("Error leyendo OBJ");

    // Normaliza a unidad
    let (model_unit, _, _, _) = center_and_scale_to_unit(&mesh.positions);

    // Ventana
    let mut window = Window::new("OBJ Viewer (A/D rotar Y, ↑/↓ rotar X, W/S zoom, ESC salir)",
                                 WIDTH, HEIGHT,
                                 WindowOptions::default())
                     .expect("No se pudo crear ventana");
    window.limit_update_rate(Some(std::time::Duration::from_micros(16_666))); // ~60 FPS

    // Parámetros de cámara
    let mut angle_y: f32 = 0.6;
    let mut fov_deg: f32 = 60.0;
    let mut cam_dist: f32 = 3.0;
    let mut angle_x: f32 = 0.0;

    let mut frame = Frame::new(WIDTH, HEIGHT);
    let yellow: u32 = 0x808080; // 0xRRGGBB

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Input
        if window.is_key_down(Key::A) { angle_y -= 0.02; }
        if window.is_key_down(Key::D) { angle_y += 0.02; }
        if window.is_key_down(Key::W) { cam_dist -= 0.05; if cam_dist < 1.5 { cam_dist = 1.5; } }
        if window.is_key_down(Key::S) { cam_dist += 0.05; }
        if window.is_key_down(Key::Up) { angle_x += 0.02; }
        if window.is_key_down(Key::Down) { angle_x -= 0.02; }

        // Proyección + depths (z_cam)
        let (screen_pts, depths) = project_perspective_to_screen(&model_unit, angle_y, angle_x, fov_deg, cam_dist);

        // Render
        frame.clear(0x101014);

        for tri in mesh.indices.chunks_exact(3) {
            let i0 = tri[0].0 as usize;
            let i1 = tri[1].0 as usize;
            let i2 = tri[2].0 as usize;

            // descartar si algún vértice está detrás/near
            if depths[i0] <= 0.001 || depths[i1] <= 0.001 || depths[i2] <= 0.001 { continue; }

            let (x0,y0) = screen_pts[i0];
            let (x1,y1) = screen_pts[i1];
            let (x2,y2) = screen_pts[i2];

            // backface culling 2D opcional:
            let ax = x1 - x0; let ay = y1 - y0;
            let bx = x2 - x0; let by = y2 - y0;
            let cross = ax*by - ay*bx;
            if cross <= 0.0 { continue; }

            fill_triangle_z(
                &mut frame,
                (x0, y0, depths[i0]),
                (x1, y1, depths[i1]),
                (x2, y2, depths[i2]),
                yellow
            );
        }

        // minifb espera un buffer u32 0x00RRGGBB
        window.update_with_buffer(&frame.color, WIDTH, HEIGHT).unwrap();
    }
}
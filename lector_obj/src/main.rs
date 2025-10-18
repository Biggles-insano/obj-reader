use std::env;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::io::{self, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct Vec3(pub f32, pub f32, pub f32);
#[derive(Debug, Clone, Copy)]
pub struct Vec2(pub f32, pub f32);

#[derive(Debug, Clone)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    pub texcoords: Vec<Vec2>,
    pub normals:   Vec<Vec3>,
    /// Índices triangulados: cada tri guarda índices a posiciones/tex/normals por separado
    pub indices:   Vec<(u32, Option<u32>, Option<u32>)>, // v_idx, vt_idx?, vn_idx?
}

impl Mesh {
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            texcoords: Vec::new(),
            normals:   Vec::new(),
            indices:   Vec::new(),
        }
    }
}

/// Convierte un índice OBJ (1-based, permite negativos) a 0-based en Rust.
/// `len` es el largo del array al que apunta (v, vt o vn).
fn resolve_idx(obj_idx: i32, len: usize) -> Option<u32> {
    if len == 0 { return None; }
    let idx0 = if obj_idx > 0 {
        (obj_idx - 1) as isize
    } else {
        (len as isize) + (obj_idx as isize) // negativo: relativo al final
    };
    if idx0 < 0 || idx0 as usize >= len { return None; }
    Some(idx0 as u32)
}

/// Parsea una cara `f` con vértices tipo:
/// v, v/vt, v//vn, v/vt/vn
fn parse_face_vertex(token: &str, vlen: usize, vtlen: usize, vnlen: usize)
    -> Option<(u32, Option<u32>, Option<u32>)>
{
    let parts: Vec<&str> = token.split('/').collect();
    let v = resolve_idx(parts.get(0)?.parse::<i32>().ok()?, vlen)?;
    let vt = if parts.len() >= 2 && !parts[1].is_empty() {
        resolve_idx(parts[1].parse::<i32>().ok()?, vtlen)
    } else { None };
    let vn = if parts.len() >= 3 && !parts[2].is_empty() {
        resolve_idx(parts[2].parse::<i32>().ok()?, vnlen)
    } else { None };
    Some((v, vt, vn))
}

/// Triangula un polígono por “fan”: (0, i-1, i)
fn triangulate_fan<T: Copy>(poly: &[T]) -> Vec<[T; 3]> {
    let mut tris = Vec::new();
    for i in 2..poly.len() {
        tris.push([poly[0], poly[i - 1], poly[i]]);
    }
    tris
}

/// Lee un archivo .obj (solo geometría)
pub fn load_obj<P: AsRef<Path>>(path: P) -> Result<Mesh, String> {
    let file = File::open(path.as_ref())
        .map_err(|e| format!("No se pudo abrir: {e}"))?;
    let reader = BufReader::new(file);

    let mut mesh = Mesh::new();

    for (lineno, line_res) in reader.lines().enumerate() {
        let line = line_res.map_err(|e| format!("Error de lectura L{}: {e}", lineno + 1))?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }

        let mut it = line.split_whitespace();
        let tag = it.next().unwrap_or_default();

        match tag {
            "v" => {
                // v x y z
                let xs: Vec<&str> = it.collect();
                if xs.len() < 3 { return Err(format!("v inválido en L{}", lineno + 1)); }
                let x = xs[0].parse::<f32>().map_err(|_| format!("v.x inválido L{}", lineno + 1))?;
                let y = xs[1].parse::<f32>().map_err(|_| format!("v.y inválido L{}", lineno + 1))?;
                let z = xs[2].parse::<f32>().map_err(|_| format!("v.z inválido L{}", lineno + 1))?;
                mesh.positions.push(Vec3(x, y, z));
            }
            "vt" => {
                // vt u v (w opcional)
                let xs: Vec<&str> = it.collect();
                if xs.len() < 2 { return Err(format!("vt inválido en L{}", lineno + 1)); }
                let u = xs[0].parse::<f32>().map_err(|_| format!("vt.u inválido L{}", lineno + 1))?;
                let v = xs[1].parse::<f32>().map_err(|_| format!("vt.v inválido L{}", lineno + 1))?;
                mesh.texcoords.push(Vec2(u, v));
            }
            "vn" => {
                // vn x y z
                let xs: Vec<&str> = it.collect();
                if xs.len() < 3 { return Err(format!("vn inválido en L{}", lineno + 1)); }
                let x = xs[0].parse::<f32>().map_err(|_| format!("vn.x inválido L{}", lineno + 1))?;
                let y = xs[1].parse::<f32>().map_err(|_| format!("vn.y inválido L{}", lineno + 1))?;
                let z = xs[2].parse::<f32>().map_err(|_| format!("vn.z inválido L{}", lineno + 1))?;
                mesh.normals.push(Vec3(x, y, z));
            }
            "f" => {
                // f v1 v2 v3 [v4 ...]
                let face_tokens: Vec<String> = it.map(|s| s.to_string()).collect();
                if face_tokens.len() < 3 {
                    return Err(format!("f con menos de 3 vértices en L{}", lineno + 1));
                }
                // Parsear todos los vértices de la cara
                let mut poly: Vec<(u32, Option<u32>, Option<u32>)> = Vec::new();
                for t in &face_tokens {
                    let v = parse_face_vertex(
                        t,
                        mesh.positions.len(),
                        mesh.texcoords.len(),
                        mesh.normals.len()
                    ).ok_or_else(|| format!("Índice de cara inválido en L{}", lineno + 1))?;
                    poly.push(v);
                }
                // Triangular si es necesario
                for tri in triangulate_fan(&poly) {
                    mesh.indices.push(tri[0]);
                    mesh.indices.push(tri[1]);
                    mesh.indices.push(tri[2]);
                }
            }
            // Ignorados comunes: g, o, s, mtllib, usemtl, etc.
            _ => { /* no-op */ }
        }
    }

    Ok(mesh)
}

// ====================== RASTER =======================
const WIDTH: usize = 800;
const HEIGHT: usize = 600;

struct Framebuffer {
    w: usize,
    h: usize,
    /// RGB8, fila invertida (y=0 arriba)
    data: Vec<u8>,
}

impl Framebuffer {
    fn new(w: usize, h: usize) -> Self {
        Self { w, h, data: vec![0; w * h * 3] }
    }

    #[inline]
    fn put_pixel(&mut self, x: i32, y: i32, r: u8, g: u8, b: u8) {
        if x < 0 || y < 0 { return; }
        let (x, y) = (x as usize, y as usize);
        if x >= self.w || y >= self.h { return; }
        let idx = (y * self.w + x) * 3;
        self.data[idx + 0] = r;
        self.data[idx + 1] = g;
        self.data[idx + 2] = b;
    }

    fn clear(&mut self, r: u8, g: u8, b: u8) {
        for px in self.data.chunks_mut(3) {
            px[0] = r; px[1] = g; px[2] = b;
        }
    }

    /// Guarda un BMP 24-bit sin compresión (fácil, sin dependencias)
    fn save_bmp(&self, path: &str) -> std::io::Result<()> {
        let row_stride = ((self.w * 3 + 3) / 4) * 4; // padding a múltiplos de 4
        let pixel_array_size = row_stride * self.h;
        let file_size = 14 + 40 + pixel_array_size; // BMP header + DIB header + data

        let mut f = File::create(path)?;
        // BMP Header
        f.write_all(&[b'B', b'M'])?;                 // Signature
        f.write_all(&(file_size as u32).to_le_bytes().as_ref())?; // File size
        f.write_all(&[0, 0, 0, 0])?;                  // Reserved
        let offset = 14 + 40;                         // Pixel data offset
        f.write_all(&(offset as u32).to_le_bytes().as_ref())?;

        // DIB Header (BITMAPINFOHEADER)
        f.write_all(&40u32.to_le_bytes())?;           // Header size
        f.write_all(&(self.w as i32).to_le_bytes())?; // Width
        f.write_all(&(self.h as i32).to_le_bytes())?; // Height (positivo = bottom-up)
        f.write_all(&1u16.to_le_bytes())?;            // Planes
        f.write_all(&24u16.to_le_bytes())?;           // Bits per pixel
        f.write_all(&0u32.to_le_bytes())?;            // Compression (BI_RGB)
        f.write_all(&(pixel_array_size as u32).to_le_bytes())?; // Image size
        f.write_all(&[0, 0, 0, 0])?;                  // X ppm
        f.write_all(&[0, 0, 0, 0])?;                  // Y ppm
        f.write_all(&[0, 0, 0, 0])?;                  // Colors in color table
        f.write_all(&[0, 0, 0, 0])?;                  // Important colors

        // Pixel data (bottom-up): escribimos filas invertidas
        let mut row = vec![0u8; row_stride];
        for y in (0..self.h).rev() {
            // Convertimos RGB -> BGR por píxel
            for x in 0..self.w {
                let idx = (y * self.w + x) * 3;
                row[x * 3 + 0] = self.data[idx + 2]; // B
                row[x * 3 + 1] = self.data[idx + 1]; // G
                row[x * 3 + 2] = self.data[idx + 0]; // R
            }
            // Padding ya está en 0
            f.write_all(&row)?;
        }
        Ok(())
    }
}

#[inline]
fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (px - ax) * (by - ay) - (py - ay) * (bx - ax)
}

fn fill_triangle(
    fb: &mut Framebuffer,
    x0: f32, y0: f32,
    x1: f32, y1: f32,
    x2: f32, y2: f32,
    r: u8, g: u8, b: u8,
) {
    let min_x = (x0.min(x1).min(x2)).floor().max(0.0) as i32;
    let max_x = (x0.max(x1).max(x2)).ceil().min((fb.w - 1) as f32) as i32;
    let min_y = (y0.min(y1).min(y2)).floor().max(0.0) as i32;
    let max_y = (y0.max(y1).max(y2)).ceil().min((fb.h - 1) as f32) as i32;

    let area = edge(x0, y0, x1, y1, x2, y2);
    if area == 0.0 { return; }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let w0 = edge(x1, y1, x2, y2, px, py);
            let w1 = edge(x2, y2, x0, y0, px, py);
            let w2 = edge(x0, y0, x1, y1, px, py);
            if (w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 && area > 0.0) ||
               (w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0 && area < 0.0) {
                fb.put_pixel(x, y, r, g, b);
            }
        }
    }
}

fn normalize_to_screen(positions: &[Vec3]) -> (Vec<(f32, f32)>, f32, f32, f32) {
    // Bounding box
    let (mut minx, mut miny, mut minz) = (f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let (mut maxx, mut maxy, mut maxz) = (f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &Vec3(x, y, z) in positions {
        if x < minx { minx = x; } if y < miny { miny = y; } if z < minz { minz = z; }
        if x > maxx { maxx = x; } if y > maxy { maxy = y; } if z > maxz { maxz = z; }
    }
    let cx = 0.5 * (minx + maxx);
    let cy = 0.5 * (miny + maxy);
    let sx = maxx - minx;
    let sy = maxy - miny;
    let max_extent = sx.max(sy).max(1e-6);
    let scale = 0.9 * (WIDTH.min(HEIGHT) as f32) * 0.5 / max_extent;

    let mut out = Vec::with_capacity(positions.len());
    for &Vec3(x, y, _z) in positions {
        let xs = (x - cx) * scale + (WIDTH as f32) * 0.5;
        let ys = (y - cy) * scale + (HEIGHT as f32) * 0.5;
        // y de pantalla hacia abajo
        out.push((xs, (HEIGHT as f32) - ys));
    }
    (out, cx, cy, scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_min() {
        let content = r#"
            v 0 0 0
            v 1 0 0
            v 0 1 0
            f 1 2 3
        "#;
        // Simular archivo usando cursor
        let mut mesh = Mesh::new();
        // Reutilizamos la lógica leyendo línea a línea del string
        let reader = BufReader::new(content.as_bytes());
        for (lineno, line_res) in reader.lines().enumerate() {
            let line = line_res.unwrap();
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let mut it = line.split_whitespace();
            let tag = it.next().unwrap();
            match tag {
                "v" => {
                    let xs: Vec<&str> = it.collect();
                    mesh.positions.push(Vec3(xs[0].parse().unwrap(), xs[1].parse().unwrap(), xs[2].parse().unwrap()));
                }
                "f" => {
                    let face_tokens: Vec<String> = it.map(|s| s.to_string()).collect();
                    let mut poly = Vec::new();
                    for t in &face_tokens {
                        poly.push(parse_face_vertex(t, mesh.positions.len(), 0, 0).unwrap());
                    }
                    for tri in triangulate_fan(&poly) {
                        mesh.indices.push(tri[0]); mesh.indices.push(tri[1]); mesh.indices.push(tri[2]);
                    }
                }
                _ => {}
            }
        }
        assert_eq!(mesh.positions.len(), 3);
        assert_eq!(mesh.indices.len(), 3); // un triángulo
    }
}

fn main() {
    // Mostrar directorio actual y listar .obj disponibles
    if let Ok(dir) = env::current_dir() {
        println!("Directorio actual: {}", dir.display());
        let mut found_any = false;
        if let Ok(entries) = fs::read_dir(&dir) {
            println!("Archivos .obj disponibles:");
            for e in entries.flatten() {
                if let Ok(ft) = e.file_type() {
                    if ft.is_file() {
                        if let Some(name) = e.file_name().to_str() {
                            if name.to_ascii_lowercase().ends_with(".obj") {
                                println!("  - {}", name);
                                found_any = true;
                            }
                        }
                    }
                }
            }
        }
        if !found_any {
            println!("(No se encontraron .obj en este directorio)");
        }
    }

    let path = loop {
        print!("Ingrese el nombre del archivo .obj (ENTER para salir): ");
        io::stdout().flush().unwrap(); // Forzar a mostrar el prompt

        let mut path = String::new();
        if let Err(e) = io::stdin().read_line(&mut path) {
            eprintln!("No se pudo leer la entrada: {e}");
            return;
        }
        let path = path.trim(); // Quitar salto de línea y espacios

        if path.is_empty() {
            println!("Salida solicitada.");
            return;
        }

        // Validar existencia exacta, sin adivinar extensión
        if !Path::new(path).exists() {
            eprintln!("No existe el archivo \"{}\" en el directorio actual.", path);
            if !path.ends_with(".obj") {
                eprintln!("Tip: si te referías a \"{}.obj\", escribe el nombre completo con la extensión.", path);
            }
            continue;
        }

        break path.to_string();
    };

    match load_obj(&path) {
        Ok(mesh) => {
            println!("Modelo cargado desde {}", &path);
            println!(
                "Vértices: {} | Coordenadas de textura: {} | Normales: {} | Triángulos: {}",
                mesh.positions.len(),
                mesh.texcoords.len(),
                mesh.normals.len(),
                mesh.indices.len() / 3
            );

            // === Raster ===
            let (screen_pts, _cx, _cy, _scale) = normalize_to_screen(&mesh.positions);
            let mut fb = Framebuffer::new(WIDTH, HEIGHT);
            fb.clear(16, 16, 20); // fondo oscuro

            // color amarillo
            let (r, g, b) = (255u8, 255u8, 0u8);

            for tri in mesh.indices.chunks_exact(3) {
                // Cada entrada de `indices` es (v_idx, vt_idx?, vn_idx?)
                let i0 = tri[0].0 as usize;
                let i1 = tri[1].0 as usize;
                let i2 = tri[2].0 as usize;
                let (x0, y0) = screen_pts[i0];
                let (x1, y1) = screen_pts[i1];
                let (x2, y2) = screen_pts[i2];

                // Backface culling 2D (opcional):
                let ax = x1 - x0; let ay = y1 - y0;
                let bx = x2 - x0; let by = y2 - y0;
                let cross = ax * by - ay * bx;
                // if cross <= 0.0 { continue; }

                fill_triangle(&mut fb, x0, y0, x1, y1, x2, y2, r, g, b);
            }

            if let Err(e) = fb.save_bmp("out.bmp") {
                eprintln!("No se pudo guardar out.bmp: {e}");
            } else {
                println!("Listo: se generó out.bmp ({}x{})", WIDTH, HEIGHT);
            }
        }
        Err(e) => {
            eprintln!("Error: {}.", e);
            eprintln!("Sugerencia: verifica permisos y formato del archivo.");
        }
    }
}
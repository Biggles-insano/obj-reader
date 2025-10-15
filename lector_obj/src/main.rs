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

    loop {
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

        match load_obj(path) {
            Ok(mesh) => {
                println!("Modelo cargado desde {}", path);
                println!(
                    "Vértices: {} | Coordenadas de textura: {} | Normales: {} | Triángulos: {}",
                    mesh.positions.len(),
                    mesh.texcoords.len(),
                    mesh.normals.len(),
                    mesh.indices.len() / 3
                );
            }
            Err(e) => {
                eprintln!("Error: {}.", e);
                eprintln!("Sugerencia: verifica permisos y formato del archivo.");
            }
        }
        break;
    }
}
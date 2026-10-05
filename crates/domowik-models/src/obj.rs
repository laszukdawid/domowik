use super::*;
use domowik_format::Point;
use domowik_geometry::{orientation, Orientation};
use std::collections::BTreeMap;
use std::io::{Cursor, Read};

pub(crate) fn archive(bytes: &[u8]) -> Result<Vec<Triangle>, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    if zip.len() > 4096 {
        return Err("Model archive has too many entries".into());
    }
    let mut objects = Vec::new();
    let mut materials = BTreeMap::new();
    let mut expanded = 0usize;
    // Never extract a path or follow OBJ references into the filesystem.
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_ascii_lowercase();
        if !name.ends_with(".obj") && !name.ends_with(".mtl") {
            continue;
        }
        if entry.size() > (MAX_BYTES - expanded) as u64 {
            return Err("Expanded model archive exceeds 64 MiB".into());
        }
        let mut data = Vec::new();
        entry
            .by_ref()
            .take((MAX_BYTES - expanded + 1) as u64)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        expanded += data.len();
        if expanded > MAX_BYTES {
            return Err("Expanded model archive exceeds 64 MiB".into());
        }
        if name.ends_with(".obj") {
            objects.push((entry.name().to_owned(), data));
        } else {
            materials.extend(mtl(&data)?);
        }
    }
    objects.sort_by(|a, b| a.0.cmp(&b.0));
    if objects.len() != 1 {
        return Err("Model ZIP must contain exactly one OBJ mesh".into());
    }
    decode(&objects[0].1, &materials)
}

fn number(v: Option<&str>) -> Result<f64, String> {
    let n: f64 = v
        .ok_or("Missing model coordinate")?
        .parse()
        .map_err(|_| "Invalid model coordinate")?;
    if !n.is_finite() || n.abs() > 1e12 {
        return Err("Model coordinate is non-finite or too large".into());
    }
    Ok(n)
}

fn mtl(bytes: &[u8]) -> Result<BTreeMap<String, [f64; 4]>, String> {
    let mut result = BTreeMap::new();
    let mut name = String::new();
    for line in String::from_utf8_lossy(bytes).lines() {
        let mut words = line.split('#').next().unwrap_or("").split_whitespace();
        match words.next() {
            Some("newmtl") => {
                name = words.collect::<Vec<_>>().join(" ");
                result.insert(name.clone(), [0.7, 0.7, 0.7, 1.0]);
            }
            Some("Kd") => {
                let rgb = [
                    number(words.next())?,
                    number(words.next())?,
                    number(words.next())?,
                ];
                if let Some(c) = result.get_mut(&name) {
                    c[..3].copy_from_slice(&rgb);
                }
            }
            Some("d") => {
                let opacity = number(words.next())?;
                if let Some(c) = result.get_mut(&name) {
                    c[3] = opacity;
                }
            }
            _ => {}
        }
    }
    Ok(result)
}

pub(crate) fn decode(
    bytes: &[u8],
    materials: &BTreeMap<String, [f64; 4]>,
) -> Result<Vec<Triangle>, String> {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut color = [0.64, 0.68, 0.72, 1.0];
    for line in String::from_utf8_lossy(bytes).lines() {
        let mut words = line.split('#').next().unwrap_or("").split_whitespace();
        match words.next() {
            Some("v") => {
                if vertices.len() >= MAX_VERTICES {
                    return Err("Model exceeds the vertex limit".into());
                }
                vertices.push([
                    number(words.next())?,
                    number(words.next())?,
                    number(words.next())?,
                ]);
            }
            Some("usemtl") => {
                color = materials
                    .get(&words.collect::<Vec<_>>().join(" "))
                    .copied()
                    .unwrap_or([0.64, 0.68, 0.72, 1.0]);
            }
            Some("f") => {
                let mut face = Vec::new();
                for word in words {
                    if face.len() >= 4096 {
                        return Err("OBJ face exceeds the vertex limit".into());
                    }
                    let index: i64 = word
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .parse()
                        .map_err(|_| "Invalid OBJ face index")?;
                    let index = if index > 0 {
                        index - 1
                    } else {
                        vertices.len() as i64 + index
                    };
                    if index < 0 || index as usize >= vertices.len() {
                        return Err("OBJ face index is outside the vertex list".into());
                    }
                    face.push(vertices[index as usize]);
                }
                if face.len() < 3 {
                    return Err("OBJ face has fewer than three vertices".into());
                }
                for points in triangulate(&face)? {
                    if triangles.len() >= MAX_TRIANGLES {
                        return Err("Model exceeds the triangle limit".into());
                    }
                    triangles.push(Triangle { points, color });
                }
            }
            // Metadata, normals and texture references are inert. No call/csh,
            // filesystem reads, remote downloads or script execution.
            _ => {}
        }
    }
    Ok(triangles)
}

fn triangulate(face: &[[f64; 3]]) -> Result<Vec<[[f64; 3]; 3]>, String> {
    if face.len() == 3 {
        return Ok(vec![[face[0], face[1], face[2]]]);
    }
    let mut normal = [0.0f64; 3];
    for i in 0..face.len() {
        let a = face[i];
        let b = face[(i + 1) % face.len()];
        for k in 0..3 {
            normal[k] += (a[(k + 1) % 3] - b[(k + 1) % 3]) * (a[(k + 2) % 3] + b[(k + 2) % 3]);
        }
    }
    let drop_axis = (0..3)
        .max_by(|a, b| normal[*a].abs().total_cmp(&normal[*b].abs()))
        .unwrap();
    let points = face
        .iter()
        .map(|p| Point::new(p[(drop_axis + 1) % 3], p[(drop_axis + 2) % 3]))
        .collect::<Vec<_>>();
    let mut ring = (0..face.len()).collect::<Vec<_>>();
    let winding = loop {
        if ring.len() < 3 {
            return Ok(Vec::new());
        }
        let a = (0..ring.len())
            .min_by(|a, b| {
                points[ring[*a]]
                    .x
                    .total_cmp(&points[ring[*b]].x)
                    .then(points[ring[*a]].y.total_cmp(&points[ring[*b]].y))
            })
            .unwrap();
        let direction = orientation(
            points[ring[(a + ring.len() - 1) % ring.len()]],
            points[ring[a]],
            points[ring[(a + 1) % ring.len()]],
        );
        if direction != Orientation::Collinear {
            break direction;
        }
        ring.remove(a);
    };
    let mut result = Vec::new();
    let mut work = 0usize;
    while ring.len() > 3 {
        let mut found = false;
        for i in 0..ring.len() {
            let [a, b, c] = [
                ring[(i + ring.len() - 1) % ring.len()],
                ring[i],
                ring[(i + 1) % ring.len()],
            ];
            if orientation(points[a], points[b], points[c]) != winding {
                continue;
            }
            let mut occupied = false;
            for &v in &ring {
                work += 1;
                if work > 2_000_000 {
                    return Err("OBJ face triangulation exceeds work budget".into());
                }
                if v == a || v == b || v == c {
                    continue;
                }
                if [(a, b), (b, c), (c, a)].iter().all(|&(x, y)| {
                    let o = orientation(points[x], points[y], points[v]);
                    o == winding || o == Orientation::Collinear
                }) {
                    occupied = true;
                    break;
                }
            }
            if !occupied {
                result.push([face[a], face[b], face[c]]);
                ring.remove(i);
                found = true;
                break;
            }
        }
        if !found {
            return Err("OBJ face is degenerate or self-intersecting".into());
        }
    }
    result.push([face[ring[0]], face[ring[1]], face[ring[2]]]);
    Ok(result)
}

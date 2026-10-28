//! OpenUSD models (`.usda`, `.usdc`, `.usdz`) through the pinned `openusd`
//! reader. Composition is served from the input bytes alone: references that
//! leave a `.usdz` package, or any layer besides the root of a loose file, are
//! refused rather than fetched. See docs/third-party/openusd.md.
//!
//! Output follows the GLB convention: metres, +Y up. `metersPerUnit` (USD
//! default 0.01) and `upAxis` (default Y) are applied; Z-up maps (x, y, z) to
//! (x, z, -y). Time-varying values use the earliest authored sample.
use super::{appearance, texture, Triangle};
use openusd::{ar, sdf, usd};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

const MAX_PRIMS: usize = 100_000;
const MAX_DEPTH: usize = 128;
const MAX_CONNECTION_HOPS: usize = 8;
const MAX_ENTRIES: usize = 4096;
const MAX_LAYERS: usize = 128;

type Matrix = [[f64; 4]; 4];
const IDENTITY: Matrix = [
    [1., 0., 0., 0.],
    [0., 1., 0., 0.],
    [0., 0., 1., 0.],
    [0., 0., 0., 1.],
];

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    Text,
    Binary,
    Package,
}

impl Kind {
    fn root(self) -> &'static str {
        match self {
            Kind::Text => "model.usda",
            Kind::Binary => "model.usdc",
            Kind::Package => "model.usdz",
        }
    }
}

/// Recognises USD bytes by content, not file name. A ZIP is a package only
/// when its first entry is a USD layer, which the USDZ rules require; any
/// other ZIP stays with the ZIP-OBJ path.
pub(crate) fn detect(bytes: &[u8]) -> Option<Kind> {
    if bytes.starts_with(b"PXR-USDC") {
        return Some(Kind::Binary);
    }
    if bytes.trim_ascii_start().starts_with(b"#usda ") {
        return Some(Kind::Text);
    }
    if !bytes.starts_with(b"PK") {
        return None;
    }
    let mut archive = zip::ZipArchive::new(io::Cursor::new(bytes)).ok()?;
    let first = archive.by_index_raw(0).ok()?;
    // Package limits are enforced by `preflight`, not by detection.
    let name = first.name().to_ascii_lowercase();
    [".usd", ".usda", ".usdc"]
        .iter()
        .any(|e| name.ends_with(e))
        .then_some(Kind::Package)
}

/// Serves the one input asset, and entries of it when it is a package.
#[derive(Clone)]
struct Memory {
    kind: Kind,
    bytes: Arc<[u8]>,
}

impl Memory {
    /// A flat entry of the root package; nested packages are refused.
    fn inside_root(&self, path: &str) -> bool {
        ar::split_package_relative_path_outer(path).is_some_and(|(p, inner)| {
            p == self.kind.root() && !ar::is_package_relative_path(&inner)
        })
    }
}

/// Rooted, drive, UNC and URI spellings name something outside the input,
/// whatever entry of the same name a package holds.
fn external(asset_path: &str) -> bool {
    if asset_path.starts_with(['/', '\\']) {
        return true;
    }
    let scheme = asset_path.split_once(':').is_some_and(|(s, _)| {
        s.starts_with(|c: char| c.is_ascii_alphabetic())
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    });
    scheme
}

/// Refuses, before the reader runs, what Domowik's own traversal limits
/// cannot bound: more than [`MAX_ENTRIES`] package entries or [`MAX_LAYERS`]
/// layers (sublayers load recursively), nested, compressed or non-relative
/// package entries, and text layers nesting `(`, `[` or `{` more than
/// [`MAX_DEPTH`] deep (the reader's parser recurses on them) or declaring more
/// than [`MAX_PRIMS`] prims in total.
fn preflight(bytes: &[u8], kind: Kind) -> Result<(), String> {
    let mut prims = 0usize;
    if kind == Kind::Text {
        return text_layer(bytes, &mut prims);
    }
    if kind != Kind::Package {
        // Binary layers are bounded only by openusd itself; see "Known
        // limitation: binary layers" in docs/third-party/openusd.md.
        return Ok(());
    }
    let mut archive = zip::ZipArchive::new(io::Cursor::new(bytes)).map_err(err)?;
    if archive.len() > MAX_ENTRIES {
        return Err(err(format!("package has more than {MAX_ENTRIES} entries")));
    }
    let mut layers = 0usize;
    // Stored entries cannot add up to more than the input; directory records
    // aliasing one payload would otherwise multiply the work below.
    let mut read = 0usize;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(err)?;
        let name = entry.name().to_ascii_lowercase();
        if external(&name) || name.split(['/', '\\']).any(|s| s == "..") {
            return Err(err(format!("package entry {name} is not a relative path")));
        }
        if name.ends_with(".usdz") {
            return Err(err("nested USDZ packages are not read"));
        }
        if entry.compression() != zip::CompressionMethod::Stored {
            return Err(err("USDZ entries must be stored uncompressed"));
        }
        read = read.saturating_add(usize::try_from(entry.size()).unwrap_or(usize::MAX));
        if read > bytes.len() {
            return Err(err("package entries overlap or exceed the package"));
        }
        let mut data = Vec::new();
        io::Read::read_to_end(&mut entry, &mut data).map_err(err)?;
        let text = data.trim_ascii_start().starts_with(b"#usda");
        if text
            || data.starts_with(b"PXR-USDC")
            || [".usd", ".usda", ".usdc"].iter().any(|e| name.ends_with(e))
        {
            layers += 1;
            if layers > MAX_LAYERS {
                return Err(err(format!("package has more than {MAX_LAYERS} layers")));
            }
        }
        if text {
            text_layer(&data, &mut prims)?;
        }
    }
    Ok(())
}

fn text_layer(text: &[u8], prims: &mut usize) -> Result<(), String> {
    use logos::Logos;
    use openusd::usda::token::Token;
    // Non-UTF-8 text is the reader's to reject; it never reaches its parser.
    let Ok(source) = std::str::from_utf8(text) else {
        return Ok(());
    };
    let mut depth = 0usize;
    for token in Token::lexer(source).flatten() {
        match token {
            Token::Punctuation('(' | '[' | '{') => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(err(format!("text layer nests deeper than {MAX_DEPTH}")));
                }
            }
            Token::Punctuation(')' | ']' | '}') => depth = depth.saturating_sub(1),
            Token::Def | Token::Over | Token::Class => {
                *prims += 1;
                if *prims > MAX_PRIMS {
                    return Err(err("too many prims"));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Joins a package-relative reference, refusing paths that climb out.
fn join(dir: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = if relative.starts_with('/') {
        Vec::new()
    } else {
        dir.split('/').filter(|s| !s.is_empty()).collect()
    };
    for part in relative.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

impl ar::Resolver for Memory {
    fn create_identifier(&self, asset_path: &str, anchor: Option<&ar::ResolvedPath>) -> String {
        if ar::is_package_relative_path(asset_path)
            || asset_path == self.kind.root()
            || external(asset_path)
        {
            return asset_path.to_owned();
        }
        let Some(anchor) = anchor.map(|a| a.to_string_lossy().into_owned()) else {
            return asset_path.to_owned();
        };
        let (package, dir) =
            if let Some((package, inner)) = ar::split_package_relative_path_inner(&anchor) {
                let dir = inner.rsplit_once('/').map_or("", |(d, _)| d).to_owned();
                (package, dir)
            } else if anchor == self.kind.root() && self.kind == Kind::Package {
                (anchor, String::new())
            } else {
                return asset_path.to_owned();
            };
        match join(&dir, asset_path) {
            Some(inner) => ar::join_package_relative_path(&package, &inner),
            None => asset_path.to_owned(),
        }
    }

    fn resolve(&self, asset_path: &str) -> Option<ar::ResolvedPath> {
        if asset_path == self.kind.root() {
            return Some(ar::ResolvedPath::new(asset_path));
        }
        if self.kind != Kind::Package || !self.inside_root(asset_path) {
            return None;
        }
        let (package, inner) = ar::split_package_relative_path_outer(asset_path)?;
        // An authored `model.usdz[/x]` or `[../x]` is no more inside than `/x`.
        if external(&inner) || inner.split(['/', '\\']).any(|s| s == "..") {
            return None;
        }
        openusd::usdz::package_contains(self, &package, &inner)
            .then(|| ar::ResolvedPath::new(asset_path))
    }

    fn resolve_for_new_asset(&self, _asset_path: &str) -> Option<ar::ResolvedPath> {
        None
    }

    fn open_asset(&self, resolved: &ar::ResolvedPath) -> io::Result<Box<dyn ar::Asset>> {
        let path = resolved.to_string_lossy();
        if path == self.kind.root() {
            return Ok(Box::new(io::Cursor::new(ar::AssetBuffer::from(
                self.bytes.clone(),
            ))));
        }
        if !self.inside_root(&path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "USD asset outside the model",
            ));
        }
        let (package, inner) = ar::split_package_relative_path_outer(&path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid packaged path"))?;
        let bytes =
            openusd::usdz::package_entry(self, &package, &inner).map_err(io::Error::other)?;
        Ok(Box::new(io::Cursor::new(bytes)))
    }

    fn in_memory_identifier<'a>(&self, asset_path: &'a str) -> Cow<'a, str> {
        Cow::Borrowed(asset_path)
    }

    fn identity(&self) -> String {
        "domowik-memory".into()
    }
}

fn err(e: impl std::fmt::Display) -> String {
    format!("USD: {e}")
}

fn value(attr: &usd::Attribute) -> Result<Option<sdf::Value>, String> {
    attr.get_at::<sdf::Value>(usd::TimeCode::EARLIEST)
        .map_err(err)
}

fn token(prim: &usd::Prim, name: &str) -> Result<Option<String>, String> {
    Ok(match value(&prim.attribute(name))? {
        Some(sdf::Value::Token(t)) => Some(t.as_str().to_owned()),
        Some(sdf::Value::String(s)) => Some(s),
        _ => None,
    })
}

fn scalar(v: &sdf::Value) -> Option<f64> {
    Some(match v {
        sdf::Value::Float(f) => f64::from(*f),
        sdf::Value::Double(d) => *d,
        sdf::Value::Half(h) => f64::from(*h),
        _ => return None,
    })
}

fn vec3(v: &sdf::Value) -> Option<[f64; 3]> {
    Some(match v {
        sdf::Value::Vec3f(p) => [p.x, p.y, p.z].map(f64::from),
        sdf::Value::Vec3d(p) => [p.x, p.y, p.z],
        sdf::Value::Vec3h(p) => [p.x, p.y, p.z].map(f64::from),
        _ => return None,
    })
}

fn vec3s(v: sdf::Value) -> Option<Vec<[f64; 3]>> {
    Some(match v {
        sdf::Value::Vec3fVec(p) => p.iter().map(|p| [p.x, p.y, p.z].map(f64::from)).collect(),
        sdf::Value::Vec3dVec(p) => p.iter().map(|p| [p.x, p.y, p.z]).collect(),
        sdf::Value::Vec3hVec(p) => p.iter().map(|p| [p.x, p.y, p.z].map(f64::from)).collect(),
        _ => return None,
    })
}

fn vec2s(v: sdf::Value) -> Option<Vec<[f64; 2]>> {
    Some(match v {
        sdf::Value::Vec2fVec(p) => p.iter().map(|p| [p.x, p.y].map(f64::from)).collect(),
        sdf::Value::Vec2dVec(p) => p.iter().map(|p| [p.x, p.y]).collect(),
        sdf::Value::Vec2hVec(p) => p.iter().map(|p| [p.x, p.y].map(f64::from)).collect(),
        _ => return None,
    })
}

fn scalars(v: sdf::Value) -> Option<Vec<f64>> {
    Some(match v {
        sdf::Value::FloatVec(p) => p.into_iter().map(f64::from).collect(),
        sdf::Value::DoubleVec(p) => p,
        sdf::Value::HalfVec(p) => p.into_iter().map(f64::from).collect(),
        _ => return None,
    })
}

fn ints(v: Option<sdf::Value>) -> Option<Vec<i32>> {
    match v? {
        sdf::Value::IntVec(v) => Some(v),
        _ => None,
    }
}

fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|k| a[r][k] * b[k][c]).sum()))
}

fn apply(m: &Matrix, p: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|r| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3])
}

fn rotation(axis: usize, degrees: f64) -> Matrix {
    let (s, c) = degrees.to_radians().sin_cos();
    let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut m = IDENTITY;
    m[a][a] = c;
    m[a][b] = -s;
    m[b][a] = s;
    m[b][b] = c;
    m
}

/// General 4x4 inverse by Gauss-Jordan elimination with partial pivoting.
fn invert(m: &Matrix) -> Option<Matrix> {
    let mut a = *m;
    let mut inv = IDENTITY;
    for col in 0..4 {
        let pivot = (col..4).max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))?;
        if a[pivot][col].abs() < 1e-300 {
            return None;
        }
        a.swap(col, pivot);
        inv.swap(col, pivot);
        let d = a[col][col];
        for k in 0..4 {
            a[col][k] /= d;
            inv[col][k] /= d;
        }
        for row in 0..4 {
            if row != col {
                let f = a[row][col];
                for k in 0..4 {
                    a[row][k] -= f * a[col][k];
                    inv[row][k] -= f * inv[col][k];
                }
            }
        }
    }
    Some(inv)
}

/// One `xformOp`, as a column-vector matrix. USD stores matrices for row
/// vectors, so an authored `transform` is transposed.
fn op(name: &str, v: &sdf::Value) -> Result<Matrix, String> {
    let kind = name
        .strip_prefix("xformOp:")
        .ok_or_else(|| err(format!("unsupported xformOpOrder entry {name}")))?;
    let kind = kind.split(':').next().unwrap_or_default();
    let bad = || err(format!("invalid value for {name}"));
    let mut m = IDENTITY;
    match kind {
        "translate" => {
            let t = vec3(v).ok_or_else(bad)?;
            for k in 0..3 {
                m[k][3] = t[k];
            }
        }
        "scale" => {
            let s = vec3(v).ok_or_else(bad)?;
            for k in 0..3 {
                m[k][k] = s[k];
            }
        }
        "rotateX" | "rotateY" | "rotateZ" => {
            let axis = usize::from(kind.as_bytes()[6] - b'X');
            m = rotation(axis, scalar(v).ok_or_else(bad)?);
        }
        "orient" => {
            let (w, x, y, z) = match v {
                sdf::Value::Quatf(q) => (q.w, q.x, q.y, q.z).map_f64(),
                sdf::Value::Quatd(q) => (q.w, q.x, q.y, q.z),
                sdf::Value::Quath(q) => (q.w, q.x, q.y, q.z).map_f64(),
                _ => return Err(bad()),
            };
            let n = (w * w + x * x + y * y + z * z).sqrt();
            if n.is_nan() || n <= 0. {
                return Err(bad());
            }
            let (w, x, y, z) = (w / n, x / n, y / n, z / n);
            m = [
                [
                    1. - 2. * (y * y + z * z),
                    2. * (x * y - z * w),
                    2. * (x * z + y * w),
                    0.,
                ],
                [
                    2. * (x * y + z * w),
                    1. - 2. * (x * x + z * z),
                    2. * (y * z - x * w),
                    0.,
                ],
                [
                    2. * (x * z - y * w),
                    2. * (y * z + x * w),
                    1. - 2. * (x * x + y * y),
                    0.,
                ],
                [0., 0., 0., 1.],
            ];
        }
        "transform" => {
            let sdf::Value::Matrix4d(t) = v else {
                return Err(bad());
            };
            m = std::array::from_fn(|r| std::array::from_fn(|c| t.0[c * 4 + r]));
        }
        _ if kind.len() == 9 && kind.starts_with("rotate") => {
            // rotateABC rotates about A first, then B, then C. The value is
            // always (X, Y, Z) angles whatever the order.
            let angles = vec3(v).ok_or_else(bad)?;
            for axis in kind[6..].bytes() {
                if !(b'X'..=b'Z').contains(&axis) {
                    return Err(bad());
                }
                let axis = usize::from(axis - b'X');
                m = multiply(&rotation(axis, angles[axis]), &m);
            }
        }
        _ => return Err(err(format!("unsupported transform operation {name}"))),
    }
    Ok(m)
}

trait MapF64 {
    fn map_f64(self) -> (f64, f64, f64, f64);
}
impl<T: Into<f64>> MapF64 for (T, T, T, T) {
    fn map_f64(self) -> (f64, f64, f64, f64) {
        (self.0.into(), self.1.into(), self.2.into(), self.3.into())
    }
}

/// The prim's local transform and whether it resets its parent's.
fn local(prim: &usd::Prim) -> Result<(Matrix, bool), String> {
    let order = match value(&prim.attribute("xformOpOrder"))? {
        Some(sdf::Value::TokenVec(t)) => t,
        _ => return Ok((IDENTITY, false)),
    };
    let mut m = IDENTITY;
    let mut reset = false;
    for entry in order {
        let entry = entry.as_str();
        if entry == "!resetXformStack!" {
            reset = true;
            m = IDENTITY;
            continue;
        }
        let (name, inverse) = match entry.strip_prefix("!invert!") {
            Some(n) => (n, true),
            None => (entry, false),
        };
        let v = value(&prim.attribute(name))?.ok_or_else(|| err(format!("missing {name}")))?;
        let mut o = op(name, &v)?;
        if inverse {
            o = invert(&o).ok_or_else(|| err(format!("singular {name}")))?;
        }
        m = multiply(&m, &o);
    }
    Ok((m, reset))
}

/// Primvar values with optional indices, expanded per lookup.
struct Primvar<T> {
    values: Vec<T>,
    indices: Option<Vec<i32>>,
    interpolation: String,
}

impl<T: Copy> Primvar<T> {
    fn read(
        prim: &usd::Prim,
        name: &str,
        convert: impl Fn(sdf::Value) -> Option<Vec<T>>,
    ) -> Result<Option<Self>, String> {
        let attr = prim.attribute(format!("primvars:{name}").as_str());
        let Some(raw) = value(&attr)? else {
            return Ok(None);
        };
        let Some(values) = convert(raw) else {
            return Ok(None);
        };
        let interpolation = attr
            .get_metadata::<sdf::Value>("interpolation")
            .map_err(err)?
            .and_then(|v| match v {
                sdf::Value::Token(t) => Some(t.as_str().to_owned()),
                _ => None,
            })
            .unwrap_or_else(|| "constant".into());
        let indices = ints(value(
            &prim.attribute(format!("primvars:{name}:indices").as_str()),
        )?);
        Ok(Some(Self {
            values,
            indices,
            interpolation,
        }))
    }

    /// Value for one face corner: `face` index, flat `corner` index into
    /// `faceVertexIndices`, and the corner's `point` index.
    fn at(&self, face: usize, corner: usize, point: usize) -> Option<T> {
        let i = match self.interpolation.as_str() {
            "constant" => 0,
            "uniform" => face,
            "vertex" | "varying" => point,
            "faceVarying" => corner,
            _ => return None,
        };
        let i = match &self.indices {
            Some(ix) => usize::try_from(*ix.get(i)?).ok()?,
            None => i,
        };
        self.values.get(i).copied()
    }
}

struct Surface {
    material: Arc<appearance::ModelMaterial>,
    color: [f64; 4],
    texture: Option<(texture::Surface, String)>,
}

struct Decoder {
    stage: usd::Stage,
    resolver: Memory,
    images: BTreeMap<String, Arc<texture::Image>>,
    materials: BTreeMap<String, Option<Arc<Surface>>>,
    triangles: Vec<Triangle>,
    vertices: usize,
    prims: usize,
    /// Stage units and up axis to Domowik space. Not part of any prim's
    /// transform stack, so `!resetXformStack!` keeps it.
    world: Matrix,
}

/// Follows an input's connection (to a shader output or a material interface
/// input) and returns the final attribute.
fn follow(stage: &usd::Stage, attr: usd::Attribute) -> Result<usd::Attribute, String> {
    let mut attr = attr;
    for _ in 0..MAX_CONNECTION_HOPS {
        let Some(source) = attr.connections().map_err(err)?.into_iter().next() else {
            return Ok(attr);
        };
        attr = stage.attribute(source).map_err(err)?;
        if attr.name().starts_with("outputs:") {
            return Ok(attr);
        }
    }
    Err(err("shader connections are too deep"))
}

/// `UsdUVTexture.inputs:scale`, when authored.
fn texture_scale(stage: &usd::Stage, shader: &usd::Prim) -> Result<Option<[f64; 4]>, String> {
    let attr = follow(stage, shader.attribute("inputs:scale"))?;
    Ok(match value(&attr)? {
        Some(sdf::Value::Vec4f(v)) => Some([v.x, v.y, v.z, v.w].map(f64::from)),
        Some(sdf::Value::Vec4d(v)) => Some([v.x, v.y, v.z, v.w]),
        _ => None,
    })
}

fn shader_id(prim: &usd::Prim) -> Result<Option<String>, String> {
    token(prim, "info:id")
}

fn linear_to_display(c: [f64; 3]) -> [f64; 3] {
    // Exact endpoints, as `texture::linear` keeps them on the way in.
    c.map(|v| match v.clamp(0., 1.) {
        v if v == 0. || v == 1. => v,
        v => texture::srgb(v),
    })
}

impl Decoder {
    fn bound_material(&self, prim: &usd::Prim) -> Result<Option<sdf::Path>, String> {
        let mut current = Some(prim.clone());
        while let Some(p) = current {
            let rel = p.relationship("material:binding");
            if let Some(target) = rel.targets().map_err(err)?.into_iter().next() {
                return Ok(Some(target));
            }
            current = p.parent().filter(|p| !p.path().is_abs_root());
        }
        Ok(None)
    }

    fn surface(&mut self, material: &sdf::Path) -> Result<Option<Arc<Surface>>, String> {
        let key = material.as_str().to_owned();
        if let Some(found) = self.materials.get(&key) {
            return Ok(found.clone());
        }
        let found = self.read_surface(material)?.map(Arc::new);
        self.materials.insert(key, found.clone());
        Ok(found)
    }

    fn read_surface(&mut self, material: &sdf::Path) -> Result<Option<Surface>, String> {
        let prim = self.stage.prim(material.clone()).map_err(err)?;
        let name = material.name().unwrap_or("Material").to_owned();
        let key = format!("usd:{}", material.as_str());
        let output = follow(&self.stage, prim.attribute("outputs:surface"))?;
        let shader = self.stage.prim(output.path().prim_path()).map_err(err)?;
        let mut color = [0.18, 0.18, 0.18];
        let mut opacity = 1.;
        let mut texture = None;
        if shader_id(&shader)?.as_deref() == Some("UsdPreviewSurface") {
            let diffuse = follow(&self.stage, shader.attribute("inputs:diffuseColor"))?;
            if diffuse.name().starts_with("outputs:") {
                let source = self.stage.prim(diffuse.path().prim_path()).map_err(err)?;
                if shader_id(&source)?.as_deref() == Some("UsdUVTexture") {
                    texture = self.uv_texture(&source)?;
                    color = texture_scale(&self.stage, &source)?
                        .map_or([1.; 3], |s| [s[0], s[1], s[2]]);
                }
            } else if let Some(c) = value(&diffuse)?.as_ref().and_then(vec3) {
                color = c;
            }
            let o = follow(&self.stage, shader.attribute("inputs:opacity"))?;
            if o.name() == "outputs:a" {
                // Texel alpha scaled by the texture's alpha scale; the texel
                // part is applied by texture sampling.
                let source = self.stage.prim(o.path().prim_path()).map_err(err)?;
                if shader_id(&source)?.as_deref() == Some("UsdUVTexture") {
                    opacity =
                        texture_scale(&self.stage, &source)?.map_or(1., |s| s[3].clamp(0., 1.));
                }
            } else if !o.name().starts_with("outputs:") {
                if let Some(v) = value(&o)?.as_ref().and_then(scalar) {
                    opacity = v.clamp(0., 1.);
                }
            }
        }
        let [r, g, b] = linear_to_display(color);
        let display = [r, g, b, opacity];
        Ok(Some(Surface {
            material: Arc::new(appearance::ModelMaterial::new(key, name, display)),
            color: display,
            texture,
        }))
    }

    fn uv_texture(
        &mut self,
        shader: &usd::Prim,
    ) -> Result<Option<(texture::Surface, String)>, String> {
        let file = follow(&self.stage, shader.attribute("inputs:file"))?;
        let Some(sdf::Value::AssetPath(asset)) = value(&file)? else {
            return Ok(None);
        };
        let resolved = asset
            .resolved_path()
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| err(format!("texture {} is outside the model", asset.as_str())))?;
        let image = match self.images.get(&resolved) {
            Some(image) => image.clone(),
            None => {
                let lower = resolved.trim_end_matches(']').to_ascii_lowercase();
                let mime = if lower.ends_with(".png") {
                    "image/png"
                } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
                    "image/jpeg"
                } else if lower.ends_with(".webp") {
                    "image/webp"
                } else {
                    return Err("Unsupported embedded base-colour image format".into());
                };
                let mut asset =
                    ar::Resolver::open_asset(&self.resolver, &ar::ResolvedPath::new(&resolved))
                        .map_err(err)?;
                let bytes = asset.read_all().map_err(err)?;
                let image = texture::decode(&bytes, mime)?;
                self.images.insert(resolved.clone(), image.clone());
                if self.images.len() > texture::MAX_LAYERS
                    || self.images.values().map(|i| i.rgba.len()).sum::<usize>()
                        > texture::MAX_TEXTURE_BYTES
                {
                    return Err("Decoded base-colour images exceed model budget".into());
                }
                image
            }
        };
        let wrap = |name: &str| -> Result<u8, String> {
            let attr = follow(&self.stage, shader.attribute(name))?;
            Ok(match value(&attr)? {
                Some(sdf::Value::Token(t)) => match t.as_str() {
                    "clamp" | "black" => 1,
                    "mirror" => 2,
                    _ => 0,
                },
                _ => 0,
            })
        };
        let wrap = [wrap("inputs:wrapS")?, wrap("inputs:wrapT")?];
        let st = follow(&self.stage, shader.attribute("inputs:st"))?;
        let mut primvar = "st".to_owned();
        if st.name().starts_with("outputs:") {
            let reader = self.stage.prim(st.path().prim_path()).map_err(err)?;
            let name = follow(&self.stage, reader.attribute("inputs:varname"))?;
            match value(&name)? {
                Some(sdf::Value::Token(t)) => primvar = t.as_str().to_owned(),
                Some(sdf::Value::String(s)) => primvar = s,
                _ => {}
            }
        }
        Ok(Some((texture::Surface { image, wrap }, primvar)))
    }

    fn walk(&mut self, prim: usd::Prim, parent: Matrix, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err(err("prim hierarchy is too deep"));
        }
        self.prims += 1;
        if self.prims > MAX_PRIMS {
            return Err(err("too many prims"));
        }
        if !prim.is_active().map_err(err)?
            || prim.is_abstract().map_err(err)?
            || !prim.is_defined().map_err(err)?
        {
            return Ok(());
        }
        if token(&prim, "visibility")?.as_deref() == Some("invisible") {
            return Ok(());
        }
        if matches!(token(&prim, "purpose")?.as_deref(), Some("proxy" | "guide")) {
            return Ok(());
        }
        let ty = prim
            .type_name()
            .map_err(err)?
            .map(|t| t.as_str().to_owned())
            .unwrap_or_default();
        let (local, reset) = local(&prim)?;
        let matrix = if reset {
            multiply(&self.world, &local)
        } else {
            multiply(&parent, &local)
        };
        if ty == "Mesh" {
            self.mesh(&prim, &matrix)?;
        }
        // Children of an instance are instance proxies, so bindings inherited
        // from the instance still apply to its prototype's geometry.
        for child in prim.children().map_err(err)? {
            self.walk(child, matrix, depth + 1)?;
        }
        Ok(())
    }

    fn mesh(&mut self, prim: &usd::Prim, matrix: &Matrix) -> Result<(), String> {
        let points = value(&prim.attribute("points"))?
            .and_then(vec3s)
            .unwrap_or_default();
        let counts = ints(value(&prim.attribute("faceVertexCounts"))?).unwrap_or_default();
        let indices = ints(value(&prim.attribute("faceVertexIndices"))?).unwrap_or_default();
        if points.is_empty() || counts.is_empty() {
            return Ok(());
        }
        self.vertices += points.len();
        if self.vertices > super::MAX_VERTICES {
            return Err("Model exceeds one million vertices".into());
        }
        let left = token(prim, "orientation")?.as_deref() == Some("leftHanded");
        let points: Vec<[f64; 3]> = points.into_iter().map(|p| apply(matrix, p)).collect();
        let display = Primvar::read(prim, "displayColor", vec3s)?;
        let display_opacity = Primvar::read(prim, "displayOpacity", scalars)?;

        let mut face_surface: Vec<Option<Arc<Surface>>> = vec![None; counts.len()];
        let base = match self.bound_material(prim)? {
            Some(path) => self.surface(&path)?,
            None => None,
        };
        face_surface.iter_mut().for_each(|s| s.clone_from(&base));
        for child in prim.children().map_err(err)? {
            if child
                .type_name()
                .map_err(err)?
                .map(|t| t.as_str() == "GeomSubset")
                != Some(true)
                || token(&child, "familyName")?.as_deref() != Some("materialBind")
            {
                continue;
            }
            let Some(path) = child
                .relationship("material:binding")
                .targets()
                .map_err(err)?
                .into_iter()
                .next()
            else {
                continue;
            };
            let surface = self.surface(&path)?;
            for f in ints(value(&child.attribute("indices"))?).unwrap_or_default() {
                if let Some(slot) = usize::try_from(f)
                    .ok()
                    .and_then(|f| face_surface.get_mut(f))
                {
                    slot.clone_from(&surface);
                }
            }
        }
        let mut uv_sets: BTreeMap<String, Option<Primvar<[f64; 2]>>> = BTreeMap::new();
        for s in face_surface.iter().flatten() {
            let Some((_, name)) = &s.texture else {
                continue;
            };
            if !uv_sets.contains_key(name) {
                uv_sets.insert(name.clone(), Primvar::read(prim, name, vec2s)?);
            }
        }
        let fallback = Arc::new(appearance::ModelMaterial::new(
            "default".into(),
            "Default".into(),
            [0.8, 0.8, 0.8, 1.],
        ));

        let mut corner = 0usize;
        for (face, &count) in counts.iter().enumerate() {
            let count = usize::try_from(count).map_err(|_| err("negative face vertex count"))?;
            let end = corner
                .checked_add(count)
                .filter(|&e| e <= indices.len())
                .ok_or_else(|| err("face vertex indices are truncated"))?;
            if count >= 3 {
                let surface = face_surface[face].as_deref();
                let uv = surface
                    .and_then(|s| s.texture.as_ref())
                    .and_then(|(_, n)| uv_sets.get(n))
                    .and_then(Option::as_ref);
                for k in 1..count - 1 {
                    let mut tri = [corner, corner + k, corner + k + 1];
                    if left {
                        tri.swap(1, 2);
                    }
                    let point_index = |c: usize| -> Result<usize, String> {
                        usize::try_from(indices[c])
                            .ok()
                            .filter(|&p| p < points.len())
                            .ok_or_else(|| err("face vertex index out of range"))
                    };
                    let pts = [
                        point_index(tri[0])?,
                        point_index(tri[1])?,
                        point_index(tri[2])?,
                    ];
                    let (color, material, texture) = match surface {
                        Some(s) => (
                            s.color,
                            s.material.clone(),
                            s.texture.as_ref().map(|(t, _)| t.clone()),
                        ),
                        None => {
                            // Triangles carry one colour: corner values are
                            // averaged in linear space.
                            let corners = |i: usize| {
                                display.as_ref().and_then(|d| d.at(face, tri[i], pts[i]))
                            };
                            let c = match [corners(0), corners(1), corners(2)] {
                                [Some(a), Some(b), Some(c)] => {
                                    linear_to_display(std::array::from_fn(|k| {
                                        (a[k] + b[k] + c[k]) / 3.
                                    }))
                                }
                                [Some(a), ..] => linear_to_display(a),
                                _ => [0.8; 3],
                            };
                            let a = (0..3)
                                .map(|i| {
                                    display_opacity
                                        .as_ref()
                                        .and_then(|d| d.at(face, tri[i], pts[i]))
                                        .unwrap_or(1.)
                                })
                                .sum::<f64>()
                                .clamp(0., 3.)
                                / 3.;
                            ([c[0], c[1], c[2], a], fallback.clone(), None)
                        }
                    };
                    let uvs = std::array::from_fn(|i| {
                        uv.and_then(|uv| uv.at(face, tri[i], pts[i]))
                            .map_or([0., 0.], |[s, t]| [s, 1. - t])
                    });
                    self.triangles.push(Triangle {
                        points: pts.map(|p| points[p]),
                        color,
                        uv: uvs,
                        texture,
                        material,
                    });
                    if self.triangles.len() > super::MAX_TRIANGLES {
                        return Err("Model exceeds one million triangles".into());
                    }
                }
            }
            corner = end;
        }
        Ok(())
    }
}

pub(crate) fn decode(bytes: &[u8], kind: Kind) -> Result<Vec<Triangle>, String> {
    let resolver = Memory {
        kind,
        bytes: Arc::from(bytes),
    };
    preflight(bytes, kind)?;
    let _scope = ar::CacheScope::begin();
    let stage = usd::Stage::builder()
        .resolver(resolver.clone())
        .open(kind.root())
        .map_err(err)?;
    let root = stage.prim(sdf::Path::abs_root()).map_err(err)?;
    let (meters, up) = {
        let layer = stage.root_layer();
        let pseudo = layer.pseudo_root();
        let meters = pseudo
            .as_ref()
            .and_then(|p| p.get::<f64>("metersPerUnit"))
            .unwrap_or(0.01);
        let up = pseudo
            .as_ref()
            .and_then(|p| p.get::<sdf::Value>("upAxis"))
            .and_then(|v| match v {
                sdf::Value::Token(t) => Some(t.as_str().to_owned()),
                _ => None,
            })
            .unwrap_or_else(|| "Y".into());
        (meters, up)
    };
    if !(meters.is_finite() && meters > 0.) {
        return Err(err("invalid metersPerUnit"));
    }
    let mut world = IDENTITY;
    for (k, row) in world.iter_mut().enumerate().take(3) {
        row[k] = meters;
    }
    if up == "Z" {
        world = multiply(&rotation(0, -90.), &world);
    }
    let mut decoder = Decoder {
        stage,
        resolver,
        images: BTreeMap::new(),
        materials: BTreeMap::new(),
        triangles: Vec::new(),
        vertices: 0,
        prims: 0,
        world,
    };
    for child in root.children().map_err(err)? {
        decoder.walk(child, world, 0)?;
    }
    if let Some(e) = decoder.stage.composition_errors().first() {
        return Err(err(format!("{e} (only self-contained models are read)")));
    }
    Ok(decoder.triangles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_refuses_escape() {
        assert_eq!(join("a/b", "../c.png").as_deref(), Some("a/c.png"));
        assert_eq!(join("", "./t/x.png").as_deref(), Some("t/x.png"));
        assert_eq!(join("", "../x.png"), None);
    }

    #[test]
    fn rotate_xyz_applies_x_first() {
        let v = sdf::Value::Vec3f(openusd::gf::Vec3f {
            x: 90.,
            y: 90.,
            z: 0.,
        });
        let m = op("xformOp:rotateXYZ", &v).unwrap();
        // X then Y: +Y -> +Z (about X) -> +X (about Y).
        let p = apply(&m, [0., 1., 0.]);
        assert!(
            (p[0] - 1.).abs() < 1e-12 && p[1].abs() < 1e-12 && p[2].abs() < 1e-12,
            "{p:?}"
        );
    }
}

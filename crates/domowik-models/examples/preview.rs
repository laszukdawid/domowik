//! Local developer smoke tool; user content and output stay outside the repo.
use std::io::Read;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--dimensions" {
        let file = std::fs::File::open(&args[2]).unwrap();
        let mut bytes = Vec::new();
        file.take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        let size = domowik_models::dimensions(&bytes).unwrap();
        println!("{}", serde_json::to_string(&size).unwrap());
        return;
    }
    if args.len() == 3
        && matches!(
            args[1].as_str(),
            "--check-directory" | "--check-dimensions-directory"
        )
    {
        let measure = args[1] == "--check-dimensions-directory";
        fn walk(path: &std::path::Path, ok: &mut usize, failed: &mut usize, measure: bool) {
            for entry in std::fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if entry.file_type().unwrap().is_dir() {
                    walk(&path, ok, failed, measure);
                } else if matches!(
                    path.extension().and_then(|s| s.to_str()),
                    Some("zip" | "glb" | "obj")
                ) {
                    let bytes = std::fs::read(&path).unwrap();
                    let result = if measure {
                        domowik_models::dimensions(&bytes)
                            .map(|d| serde_json::to_string(&d).unwrap())
                    } else {
                        domowik_models::preview(&bytes)
                            .map(|p| format!("{} triangles", p.triangles))
                    };
                    match result {
                        Ok(description) => {
                            *ok += 1;
                            println!("ok {}: {}", path.display(), description);
                        }
                        Err(e) => {
                            *failed += 1;
                            eprintln!("FAILED {}: {e}", path.display());
                        }
                    }
                }
            }
        }
        let (mut ok, mut failed) = (0, 0);
        walk(
            std::path::Path::new(&args[2]),
            &mut ok,
            &mut failed,
            measure,
        );
        println!(
            "{ok} {}, {failed} failed",
            if measure { "measured" } else { "rendered" }
        );
        if failed > 0 {
            std::process::exit(1)
        }
        return;
    }
    if args.len() != 3 {
        eprintln!("usage: preview MODEL OUTPUT.png");
        std::process::exit(2);
    }
    let file = std::fs::File::open(&args[1]).unwrap();
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    match domowik_models::preview(&bytes) {
        Ok(p) => {
            std::fs::write(&args[2], &p.png).unwrap();
            println!(
                "{} triangles, {} covered pixels",
                p.triangles, p.covered_pixels
            );
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

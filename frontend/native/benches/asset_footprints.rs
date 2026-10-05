//! Original synthetic footprint corpus, no third-party model content. Measures
//! warm spatial query + JSON/FFI ownership, not decoded rendering or disk I/O.
use domowik_bridge::*;
use serde_json::{json, Value};
use std::hint::black_box;
use std::time::Instant;
use uuid::Uuid;

fn call(handle: u64, request: &Value, assets: bool) -> Vec<u8> {
    let bytes = serde_json::to_vec(request).unwrap();
    let mut out = DomowikBuf {
        data: std::ptr::null_mut(),
        len: 0,
        token: 0,
    };
    let code = unsafe {
        if assets {
            domowik_plan_assets(handle, bytes.as_ptr(), bytes.len() as u64, &mut out)
        } else {
            domowik_plan_execute(handle, bytes.as_ptr(), bytes.len() as u64, &mut out)
        }
    };
    assert_eq!(code, status::OK);
    let result = unsafe { std::slice::from_raw_parts(out.data, out.len as usize).to_vec() };
    assert_eq!(domowik_buffer_free(out.token), status::OK);
    result
}

fn main() {
    let mut handle = 0;
    assert_eq!(unsafe { domowik_plan_create(&mut handle) }, status::OK);
    let level = Uuid::now_v7();
    let mut operations = vec![
        json!({"kind":"createEntity", "operationId":Uuid::now_v7(), "entityId":level,
        "entity":{"type":"level", "id":level, "name":"Synthetic", "elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2500}}),
    ];
    for i in 0..1000 {
        let id = Uuid::now_v7();
        operations.push(json!({"kind":"createEntity", "operationId":Uuid::now_v7(), "entityId":id,
            "entity":{"type":"assetInstance", "id":id, "levelId":level, "digest":format!("sha256:{}", "0".repeat(64)),
            "transform":{"translationMm":[(i % 40) * 2000,(i / 40) * 2000],"scaleUniform":1,"rotationDeg":30},
            "editorFootprintMm":[600,400]}}));
    }
    call(handle, &json!({"operations":operations}), false);
    let request = json!({"kind":"hit", "levelId":level,"pointMm":[2000,2000],"radiusMm":120});
    call(handle, &request, true); // Populate revision-keyed spatial cache.
    let count = 100_000;
    let start = Instant::now();
    for _ in 0..count {
        black_box(call(handle, black_box(&request), true));
    }
    println!(
        "1000 rotated footprints, warm native query + JSON/ownership: {:.0} ns/op ({} iterations)",
        start.elapsed().as_nanos() as f64 / count as f64,
        count
    );
    assert_eq!(domowik_plan_close(handle), status::OK);
}

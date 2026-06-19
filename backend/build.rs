use std::path::Path;

fn main() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../frontend/dist");
    let index = dist.join("index.html");
    if !index.exists() {
        let _ = std::fs::create_dir_all(&dist);
        let _ = std::fs::write(
            &index,
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>dockui</title></head>\
             <body style=\"font-family:sans-serif;background:#0d1117;color:#c9d1d9;padding:2rem\">\
             <h1>dockui</h1><p>Frontend not built. Run <code>npm --prefix frontend run build</code>.</p>\
             </body></html>",
        );
    }
    println!("cargo:rerun-if-changed=../frontend/dist");
}

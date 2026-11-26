//! `web-bundle CONTENT_DIR ASSETS_DIR OUT`: a game's `content/` and
//! `assets/` in one file, the one the browser client fetches at start.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [content, assets, out] = args.as_slice() else {
        eprintln!("usage: web-bundle CONTENT_DIR ASSETS_DIR OUT");
        return std::process::ExitCode::from(2);
    };
    let mut files = client_files::bundle::Files::new();
    for (dir, prefix) in [(content, "content"), (assets, "assets")] {
        if let Err(e) = client_files::bundle::add_dir(&mut files, std::path::Path::new(dir), prefix) {
            eprintln!("{dir}: {e}");
            return std::process::ExitCode::FAILURE;
        }
    }
    let bytes = client_files::bundle::encode(&files);
    if let Err(e) = std::fs::write(out, &bytes) {
        eprintln!("{out}: {e}");
        return std::process::ExitCode::FAILURE;
    }
    println!("{out}: {} files, {} KiB", files.len(), bytes.len() / 1024);
    std::process::ExitCode::SUCCESS
}

#[cfg(target_arch = "wasm32")]
fn main() {}

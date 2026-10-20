fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;

    let shared_root = root.join("proto");
    let apps_root = root.join("apps");

    // 收集 proto 文件（绝对路径）
    let mut protos: Vec<std::path::PathBuf> = vec![
        shared_root.join("common/v1/common.proto"),
        shared_root.join("internal/v1/internal.proto"),
    ];
    // 各应用自有 proto
    let mut include_dirs: Vec<std::path::PathBuf> = vec![shared_root.clone()];

    if let Ok(entries) = std::fs::read_dir(&apps_root) {
        for e in entries.flatten() {
            let proto_dir = e.path().join("proto");
            if proto_dir.is_dir() {
                include_dirs.push(proto_dir.clone());
                collect_protos(&proto_dir, &mut protos);
            }
        }
    }

    for p in &protos {
        println!("cargo:rerun-if-changed={}", p.display());
    }

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("cool_admin_reflection.bin"))
        .compile_protos(&protos, &include_dirs)?;

    Ok(())
}

fn collect_protos(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_protos(&p, out);
            } else if p.extension().map(|x| x == "proto").unwrap_or(false) {
                out.push(p);
            }
        }
    }
}

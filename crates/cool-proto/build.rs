fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../proto")
        .canonicalize()?;

    let protos = [
        proto_root.join("common/v1/common.proto"),
        proto_root.join("internal/v1/internal.proto"),
        proto_root.join("demo/v1/demo_sample.proto"),
        proto_root.join("base/v1/base_open.proto"),
        proto_root.join("base/v1/base_sys_user.proto"),
        proto_root.join("base/v1/base_sys.proto"),
        proto_root.join("base/v1/base_comm.proto"),
        proto_root.join("dict/v1/dict.proto"),
        proto_root.join("space/v1/space.proto"),
        proto_root.join("task/v1/task.proto"),
    ];

    for p in &protos {
        println!("cargo:rerun-if-changed={}", p.display());
    }

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("cool_admin_reflection.bin"))
        .compile_protos(&protos, &[proto_root])?;

    Ok(())
}

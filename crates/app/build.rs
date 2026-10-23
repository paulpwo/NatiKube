//! Windows only: embeds the application icon and the version information
//! (file properties) in `natikube.exe`. On every other target this does nothing.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/icon/natikube.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    embed();
}

// The build dependency exists only when the build runs on Windows, so the
// embedding is compiled only there (CI builds natively on windows-latest).
#[cfg(not(windows))]
fn embed() {}

#[cfg(windows)]
fn embed() {
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("out dir"));
    let icon = manifest_dir
        .join("../../assets/icon/natikube.ico")
        .canonicalize()
        .expect("assets/icon/natikube.ico is missing");
    // The resource compiler wants forward slashes (backslashes are escapes).
    let icon = icon
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");

    let version = std::env::var("CARGO_PKG_VERSION").expect("package version");
    let mut parts = version
        .split(|c: char| !c.is_ascii_digit())
        .map(|p| p.parse::<u16>().unwrap_or(0));
    let (major, minor, patch) = (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    );

    let rc = format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Paul Osinga"
      VALUE "FileDescription", "NatiKube"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "natikube"
      VALUE "LegalCopyright", "Copyright (c) 2026 Paul Osinga. MIT License."
      VALUE "OriginalFilename", "natikube.exe"
      VALUE "ProductName", "NatiKube"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 0x04B0
  END
END
"#
    );
    let rc_path = out_dir.join("natikube.rc");
    std::fs::write(&rc_path, rc).expect("write natikube.rc");
    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed the Windows resources");
}

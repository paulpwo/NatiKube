use std::path::PathBuf;

/// Opens the native open panel (NSOpenPanel through rfd on macOS) for kubeconfig files
/// or folders. Returns the chosen paths, empty when the user cancels.
///
/// Await it from a GPUI task (`cx.spawn`); the dialog runs on the main thread
/// and does not block the UI thread's event loop.
#[cfg(target_os = "macos")]
pub async fn pick_kubeconfig_paths() -> Vec<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Add kubeconfig")
        .set_directory(super::default_kube_dir().unwrap_or_default())
        .pick_files_or_folders()
        .await
        .map(|handles| {
            handles
                .into_iter()
                .map(|h| h.path().to_path_buf())
                .collect()
        })
        .unwrap_or_default()
}

/// Windows (and Linux) native dialogs cannot mix files and folders, so the
/// picker offers kubeconfig files; folders are still reachable through the
/// `KUBECONFIG` variable and `~/.kube`. Returns the chosen paths, empty when
/// the user cancels.
#[cfg(not(target_os = "macos"))]
pub async fn pick_kubeconfig_paths() -> Vec<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Add kubeconfig")
        .set_directory(super::default_kube_dir().unwrap_or_default())
        .pick_files()
        .await
        .map(|handles| {
            handles
                .into_iter()
                .map(|h| h.path().to_path_buf())
                .collect()
        })
        .unwrap_or_default()
}

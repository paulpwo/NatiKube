//! Platform-specific wording in the Settings window.

/// Placeholder of the Terminal → Shell field.
pub fn shell_placeholder(windows: bool) -> &'static str {
    if windows {
        "pwsh.exe, else powershell.exe, else cmd.exe"
    } else {
        "$SHELL, else /bin/zsh"
    }
}

/// Help text of the kubeconfig sources list.
pub fn kubeconfig_help(windows: bool) -> &'static str {
    if windows {
        "%USERPROFILE%\\.kube\\config and %KUBECONFIG% are always scanned. Add more kubeconfig files below."
    } else {
        "~/.kube/config and $KUBECONFIG are always scanned. Add more files or folders below."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wording_follows_the_platform() {
        assert!(shell_placeholder(true).contains("pwsh.exe"));
        assert!(shell_placeholder(false).contains("/bin/zsh"));
        assert!(kubeconfig_help(true).contains("%USERPROFILE%"));
        assert!(kubeconfig_help(false).starts_with("~/.kube/config"));
    }
}

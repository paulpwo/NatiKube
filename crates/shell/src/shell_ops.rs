//! Shell operations behind the appearance, palette and kubeconfig commands.

use std::path::PathBuf;

use gpui_kit::*;
use natikube_clusters::ImportSummary;
use natikube_core::{Appearance, ClusterInfo};

use crate::menus::install_menus;
use crate::nav_layout::palette_context;
use crate::shell_view::ShellView;

impl ShellView {
    /// Applies `appearance` live and keeps the menu checkmark in sync. The
    /// choice is saved unless `--appearance` overrides this run.
    pub(crate) fn set_appearance(&mut self, appearance: Appearance, cx: &mut Context<Self>) {
        if self.options.appearance_override.is_none() {
            self.settings.appearance = appearance;
            self.persist();
        }
        natikube_ui_kit::set_appearance(appearance, cx);
        install_menus(appearance, cx);
        cx.notify();
    }

    /// Hands the palette what it can offer right now.
    pub(crate) fn refresh_palette(&mut self, cx: &mut Context<Self>) {
        let resources = self
            .session
            .as_ref()
            .map(|s| s.resources.read(cx).loaded_refs(cx))
            .unwrap_or_default();
        let context = palette_context(
            &self.clusters,
            self.active.as_ref(),
            self.nav.sections(),
            resources,
        );
        self.palette
            .update(cx, |palette, cx| palette.set_context(context, cx));
    }

    pub(crate) fn set_notice(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.notice = Some(message.into());
        cx.notify();
    }

    /// "Import from Freelens…": reads Freelens' preference files and its
    /// `kubeconfigs/` folder off the main thread, adds the new paths to the
    /// settings, reloads the catalog and shows a one-line summary.
    pub(crate) fn import_freelens(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.options.demo {
            return self.set_notice("Not available in the demo.", cx);
        }
        let settings = self.settings.clone();
        self._import = Some(cx.spawn_in(window, async move |this, cx| {
            let summary = match natikube_clusters::freelens_data_dir().filter(|dir| dir.is_dir()) {
                Some(_) => Some(natikube_clusters::import_freelens_async(settings).await),
                None => None,
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_freelens_import(summary, window, cx)
            });
        }));
    }

    fn finish_freelens_import(
        &mut self,
        summary: Option<ImportSummary>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(summary) = summary else {
            return self.set_notice("Freelens data folder not found.", cx);
        };
        if !summary.imported.is_empty() {
            summary.apply(&mut self.settings);
            self.persist();
            self.reload_clusters(window, cx);
        }
        self.set_notice(format!("Freelens: {}.", summary.message()), cx);
    }

    /// Opens the native file (or folder) panel and adds the choice.
    pub(crate) fn pick_kubeconfigs(
        &mut self,
        folders: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.options.demo {
            return self.set_notice("Not available in the demo.", cx);
        }
        self._dialog = Some(cx.spawn_in(window, async move |this, cx| {
            let mut dialog = rfd::AsyncFileDialog::new().set_title(if folders {
                "Add kubeconfig folder"
            } else {
                "Add kubeconfig"
            });
            if let Some(home) = dirs::home_dir() {
                dialog = dialog.set_directory(home.join(".kube"));
            }
            let picked = if folders {
                dialog.pick_folders().await
            } else {
                dialog.pick_files().await
            };
            let paths: Vec<PathBuf> = picked
                .unwrap_or_default()
                .into_iter()
                .map(|handle| handle.path().to_path_buf())
                .collect();
            if paths.is_empty() {
                return;
            }
            let _ = this.update_in(cx, |this, window, cx| {
                this.add_kubeconfigs(paths, window, cx)
            });
        }));
    }

    fn add_kubeconfigs(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let summary =
            natikube_clusters::kubeconfig_add_summary(&self.settings.extra_kubeconfigs, &paths);
        let added = summary.imported.len();
        if added == 0 {
            return self.set_notice("Already added.", cx);
        }
        summary.apply(&mut self.settings);
        self.persist();
        self.reload_clusters(window, cx);
        self.set_notice(
            if added == 1 {
                "Added 1 kubeconfig.".to_string()
            } else {
                format!("Added {added} kubeconfigs.")
            },
            cx,
        );
    }

    /// Re-reads every kubeconfig source off the main thread. Contexts that
    /// were injected by name (`--context`) and are not in any file stay listed.
    pub(crate) fn reload_clusters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.options.fake_rows.is_some() || self.options.demo {
            return;
        }
        let settings = self.settings.clone();
        self._reload = Some(cx.spawn_in(window, async move |this, cx| {
            let catalog = natikube_clusters::load_catalog_async(settings).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.apply_catalog(catalog, window, cx)
            });
        }));
    }

    fn apply_catalog(
        &mut self,
        mut catalog: Vec<ClusterInfo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let injected: Vec<ClusterInfo> = self
            .clusters
            .iter()
            .filter(|c| c.kubeconfig.is_none() && !catalog.iter().any(|n| n.id == c.id))
            .cloned()
            .collect();
        catalog.extend(injected);
        self.clusters = catalog;

        let clusters = self.clusters.clone();
        self.rail
            .update(cx, |rail, cx| rail.set_clusters(clusters.clone(), cx));
        self.header
            .update(cx, |header, cx| header.set_clusters(clusters.clone(), cx));
        match self.active.clone() {
            Some(id) => {
                let info = self.clusters.iter().find(|c| c.id == id).cloned();
                self.header
                    .update(cx, |header, cx| header.set_cluster(info, cx));
            }
            None => {
                if let Some(first) = self.clusters.first().map(|c| c.id.clone()) {
                    self.select_cluster(first, window, cx);
                }
            }
        }
        self.apply_alerts();
        self.sync_alerts_settings(cx);
        self.refresh_palette(cx);
        cx.notify();
    }
}

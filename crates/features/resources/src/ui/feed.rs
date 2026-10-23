//! Bridges a tokio-side stream into a GPUI view with coalesced redraws.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::StreamExt as _;
use gpui_kit::*;
use natikube_kube_client::spawn_stream;

use crate::domain::{
    ObjectSource, RowEvent, SourceError, UsageMap, UsageSource, age_tick_interval,
};

/// Minimum time between two redraws caused by incoming events.
pub(crate) const BATCH_WINDOW: Duration = Duration::from_millis(80);

pub(crate) type RowItem = Result<RowEvent, SourceError>;

pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// Watches `source` on the shared tokio runtime and hands batches of events to
/// `apply` on the UI thread. The returned task owns the stream handle:
/// dropping it (with the view) cancels the watcher.
pub(crate) fn pump_rows<V: 'static>(
    source: &dyn ObjectSource,
    cx: &mut Context<V>,
    mut apply: impl FnMut(&mut V, Vec<RowItem>, &mut Context<V>) + 'static,
) -> Task<()> {
    let mut events = spawn_stream(source.watch());
    cx.spawn(async move |this, cx| {
        while let Some(first) = events.next().await {
            let mut batch = vec![first];
            while let Some(next) = events.try_recv() {
                batch.push(next);
            }
            if this.update(cx, |view, cx| apply(view, batch, cx)).is_err() {
                break;
            }
            // Coalesce: whatever arrives during this window is drawn in one go.
            cx.background_executor().timer(BATCH_WINDOW).await;
        }
    })
}

/// Same for the metrics poller: `apply` receives the latest usage map.
pub(crate) fn pump_usage<V: 'static>(
    source: &dyn UsageSource,
    cx: &mut Context<V>,
    mut apply: impl FnMut(&mut V, UsageMap, &mut Context<V>) + 'static,
) -> Task<()> {
    let mut polls = spawn_stream(source.poll());
    cx.spawn(async move |this, cx| {
        while let Some(usage) = polls.next().await {
            if this.update(cx, |view, cx| apply(view, usage, cx)).is_err() {
                break;
            }
        }
    })
}

/// Longest sleep between checks, so a row that appears while the view is
/// idle gets its seconds counter within a few seconds.
const MAX_SLEEP: Duration = Duration::from_secs(5);

/// Refreshes relative ages (Age, Last Seen) only as often as their text can
/// change. `newest` returns the most recent timestamp the view shows; while
/// everything is older than ten minutes the view redraws once a minute (or
/// hour) instead of once a second, so an idle window does no redraw work.
pub(crate) fn tick_ages<V: 'static>(
    cx: &mut Context<V>,
    newest: impl Fn(&V, &App) -> Option<i64> + 'static,
    mut tick: impl FnMut(&mut V, &mut Context<V>) + 'static,
) -> Task<()> {
    let wanted = move |this: &WeakEntity<V>, cx: &mut AsyncApp| {
        this.read_with(cx, |view, cx| {
            age_tick_interval(newest(view, cx).map(|t| unix_now() - t))
        })
        .ok()
    };
    cx.spawn(async move |this, cx| {
        let mut waited = Duration::ZERO;
        while let Some(wait) = wanted(&this, cx) {
            let nap = wait.saturating_sub(waited).min(MAX_SLEEP);
            cx.background_executor().timer(nap).await;
            waited += nap;
            let Some(wait) = wanted(&this, cx) else { break };
            if waited >= wait {
                waited = Duration::ZERO;
                if this.update(cx, |view, cx| tick(view, cx)).is_err() {
                    break;
                }
            }
        }
    })
}

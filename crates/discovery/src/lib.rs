//! Validated version 2 local service discovery. Filesystem events are only hints.
use anyhow::Result;
pub use omochi_local::{DiscoveredService, Manifest};
use std::{collections::BTreeMap, path::Path};
/// Wake a scanner on changes, never treating a filesystem event as trusted data.
/// Keep periodic scans: notifications can be unavailable or lost on some filesystems.
pub fn watch(
    root: &Path,
    changed: impl Fn() + Send + 'static,
) -> Result<notify::RecommendedWatcher> {
    use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    scan(root)?;
    let mut watcher = RecommendedWatcher::new(
        move |event: notify::Result<Event>| {
            // Our own manifest reads must not cause a scan/notification loop.
            if !matches!(
                event,
                Ok(Event {
                    kind: EventKind::Access(_),
                    ..
                })
            ) {
                changed();
            }
        },
        Config::default().with_follow_symlinks(false),
    )?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok(watcher)
}

pub fn scan(root: &Path) -> Result<BTreeMap<String, DiscoveredService>> {
    Ok(omochi_local::LocalRuntime::open(root.to_path_buf())?.scan()?)
}

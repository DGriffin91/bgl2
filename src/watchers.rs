use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub struct Watchers {
    has_changes: Arc<AtomicBool>,
    _watchers: Vec<notify::RecommendedWatcher>,
}

impl Watchers {
    pub fn new<I, P>(paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let has_changes = Arc::new(AtomicBool::new(false));
        let _watchers = paths
            .into_iter()
            .map(|path| {
                let watcher_has_changes = has_changes.clone();
                let mut _watcher =
                    notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                        let event =
                            event.unwrap_or_else(|_| notify::Event::new(notify::EventKind::Any));
                        if matches!(
                            event.kind,
                            notify::EventKind::Any
                                | notify::EventKind::Modify(_)
                                | notify::EventKind::Other
                        ) {
                            watcher_has_changes.store(true, Ordering::Relaxed);
                        }
                    })
                    .unwrap();
                notify::Watcher::watch(
                    &mut _watcher,
                    path.as_ref(),
                    notify::RecursiveMode::NonRecursive,
                )
                .unwrap();
                _watcher
            })
            .collect::<Vec<_>>();
        Self {
            has_changes,
            _watchers,
        }
    }

    pub fn check(&self) -> bool {
        self.has_changes.swap(false, Ordering::Relaxed)
    }
}

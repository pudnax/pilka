use ahash::{AHashMap, AHashSet};
use anyhow::Result;
use notify_debouncer_mini::{DebounceEventResult, DebouncedEventKind};
use winit::event_loop::EventLoopProxy;

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::{ShaderSource, UserEvent};

use parking_lot::Mutex;

#[derive(Clone)]
pub struct Watcher {
    pub watcher: Arc<Mutex<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>>>,
    pub include_mapping: Arc<Mutex<AHashMap<PathBuf, AHashSet<ShaderSource>>>>,
}

impl Watcher {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let watcher = notify_debouncer_mini::new_debouncer(
            Duration::from_millis(350),
            watch_callback(proxy),
        )?;

        Ok(Self {
            watcher: Arc::new(Mutex::new(watcher)),
            include_mapping: Arc::new(Mutex::new(AHashMap::new())),
        })
    }

    pub fn unwatch_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let mut watcher = self.watcher.lock();
        watcher.watcher().unwatch(path.as_ref())?;
        Ok(())
    }

    pub fn watch_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let mut watcher = self.watcher.lock();
        watcher
            .watcher()
            .watch(path.as_ref(), notify::RecursiveMode::NonRecursive)?;
        Ok(())
    }
}

fn watch_callback(proxy: EventLoopProxy<UserEvent>) -> impl FnMut(DebounceEventResult) {
    move |event| match event {
        Ok(events) => {
            if let Some(path) = events
                .into_iter()
                .filter(|e| e.kind == DebouncedEventKind::Any)
                .map(|event| event.path)
                .next()
            {
                if path.extension() == Some(OsStr::new("glsl"))
                    || path.extension() == Some(OsStr::new("frag"))
                    || path.extension() == Some(OsStr::new("vert"))
                    || path.extension() == Some(OsStr::new("comp"))
                {
                    let _ = proxy
                        .send_event(UserEvent::Glsl {
                            path: path.canonicalize().unwrap(),
                        })
                        .map_err(|err| eprintln!("Event Loop has been dropped: {err}"));
                }
            }
        }
        Err(errors) => eprintln!("File watcher error: {errors}"),
    }
}

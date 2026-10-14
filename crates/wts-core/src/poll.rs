use crate::{config, git, Cancel, Error, ErrorKind, Service, Snapshot};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct MonitorUpdate {
    pub snapshot: Arc<Snapshot>,
    pub interval_seconds: u64,
}

pub struct Monitor {
    cancel: Cancel,
    latest: Arc<Mutex<Option<Arc<MonitorUpdate>>>>,
    worker: Option<JoinHandle<()>>,
}

impl Monitor {
    pub(crate) fn start(service: Service) -> Result<Self, Error> {
        let cancel = Cancel::new();
        let latest = Arc::new(Mutex::new(None));
        let worker_cancel = cancel.clone();
        let worker_latest = latest.clone();
        let worker = thread::Builder::new().name("wts-monitor".into()).spawn(move || {
            worker(service, worker_cancel, worker_latest);
        }).map_err(|error| Error::io("Cannot start monitor", error))?;
        Ok(Self { cancel, latest, worker: Some(worker) })
    }

    pub fn latest(&self) -> Option<Arc<MonitorUpdate>> {
        self.latest.lock().unwrap_or_else(|error| error.into_inner()).clone()
    }

    pub fn stop(&mut self) -> Result<(), Error> {
        self.cancel.cancel();
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| Error::new(ErrorKind::Io, "Monitor worker failed"))?;
        }
        Ok(())
    }
}

impl Drop for Monitor {
    fn drop(&mut self) { let _ = self.stop(); }
}

#[derive(Default)]
pub(crate) struct Backoff {
    previous: Option<Snapshot>,
    seconds: u64,
}

impl Backoff {
    pub(crate) fn observe(&mut self, snapshot: &Snapshot) -> u64 {
        self.seconds = if self.previous.as_ref() == Some(snapshot) {
            (self.seconds * 2).min(30)
        } else { 5 };
        self.previous = Some(snapshot.clone());
        self.seconds
    }
}

#[derive(Default)]
pub(crate) struct Registrations {
    pub repos: Vec<PathBuf>,
    pub error: Option<Error>,
    bytes: Option<Result<Vec<u8>, Error>>,
}

impl Registrations {
    pub(crate) fn reload(&mut self, bytes: Result<Vec<u8>, Error>) -> bool {
        if self.bytes.as_ref() == Some(&bytes) { return false; }
        match bytes.as_ref().map_err(Clone::clone).and_then(|bytes| config::parse(bytes)) {
            Ok(repos) => { self.repos = repos; self.error = None; }
            Err(error) => { self.error = Some(error); }
        }
        self.bytes = Some(bytes);
        true
    }
}

fn worker(service: Service, cancel: Cancel, latest: Arc<Mutex<Option<Arc<MonitorUpdate>>>>) {
    let mut registrations = Registrations::default();
    let mut backoff = Backoff::default();
    let mut deadline = Instant::now();
    loop {
        if cancel.is_cancelled() { break; }
        let changed = registrations.reload(config::read(&service.config_path));
        if changed || Instant::now() >= deadline {
            let snapshot = git::scan(&registrations.repos, registrations.error.clone(), &cancel);
            if cancel.is_cancelled() { break; }
            let seconds = backoff.observe(&snapshot);
            *latest.lock().unwrap_or_else(|error| error.into_inner()) = Some(Arc::new(MonitorUpdate {
                snapshot: Arc::new(snapshot), interval_seconds: seconds,
            }));
            deadline = Instant::now() + Duration::from_secs(seconds);
        }
        cancel.wait(deadline.saturating_duration_since(Instant::now()).min(Duration::from_secs(1)));
    }
}

use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use tokio::sync::{mpsc, watch};

use crate::{
    adb::{Adb, Device, Service, ServiceKind},
    pairing::Session,
};

#[derive(Clone, Default)]
pub struct Snapshot {
    pub devices: Vec<Device>,
    pub services: Vec<Service>,
}

pub enum EventKind {
    Snapshot(Snapshot),
    Status(String),
    Log(String),
}

pub struct Event {
    pub session: String,
    pub kind: EventKind,
}

pub enum Action {
    Connect(SocketAddr),
}

struct Worker {
    adb: Adb,
    session: Session,
    tx: mpsc::UnboundedSender<Event>,
    attempts: u8,
    next_attempt: Instant,
    paired_ip: Option<std::net::IpAddr>,
    paired_guid: Option<String>,
    connected: bool,
    last_error: String,
}

impl Worker {
    fn emit(&self, kind: EventKind) {
        let _ = self.tx.send(Event {
            session: self.session.name.clone(),
            kind,
        });
    }

    fn status(&self, text: impl Into<String>) {
        self.emit(EventKind::Status(text.into()));
    }
    fn log(&self, text: impl Into<String>) {
        self.emit(EventKind::Log(text.into()));
    }

    // The status line shows the reason; the activity log keeps one entry per distinct error.
    fn error(&mut self, error: String) {
        self.status(error.clone());
        if self.last_error != error {
            self.log(error.clone());
            self.last_error = error;
        }
    }

    async fn refresh(&mut self) {
        let (services, devices) = tokio::join!(self.adb.services(), self.adb.devices());
        // Connected USB and Wi-Fi devices remain visible even if mDNS is unavailable.
        let devices = match devices {
            Ok(devices) => devices,
            Err(err) => {
                self.error(format!("adb devices failed: {}", reason(&err)));
                Vec::new()
            }
        };
        let services = match services {
            Ok(services) => services,
            Err(err) => {
                self.emit(EventKind::Snapshot(Snapshot {
                    devices,
                    services: Vec::new(),
                }));
                self.error(format!(
                    "Discovery failed: {}. Use easyadb pair IP:PORT instead.",
                    reason(&err)
                ));
                return;
            }
        };
        self.emit(EventKind::Snapshot(Snapshot {
            services: services.clone(),
            devices: devices.clone(),
        }));

        if let Some(ip) = self.paired_ip {
            let nearby: Vec<_> = services
                .iter()
                .filter(|s| s.kind == ServiceKind::Connect && s.address.ip() == ip)
                .collect();
            let connected = devices.iter().any(|d| {
                d.state == "device"
                    && (d
                        .serial
                        .parse::<SocketAddr>()
                        .is_ok_and(|addr| addr.ip() == ip)
                        || nearby
                            .iter()
                            .any(|s| device_service_matches(&d.serial, &s.name))
                        || self
                            .paired_guid
                            .as_ref()
                            .is_some_and(|guid| device_service_matches(&d.serial, guid)))
            });
            if connected {
                self.status("Connected");
                if !self.connected {
                    self.log("Device is online in ADB.");
                }
                self.connected = true;
                return;
            }
            self.connected = false;
            self.status("Paired · waiting for device connection");
            if Instant::now() >= self.next_attempt {
                if let Some(service) = nearby.first() {
                    self.next_attempt = Instant::now() + Duration::from_secs(10);
                    match self.adb.connect(service.address).await {
                        Ok(_) => self.log("Connection requested; checking ADB device state…"),
                        Err(err) => self.error(format!("Reconnect failed: {}", reason(&err))),
                    }
                }
            }
            return;
        }

        if self.session.remaining().is_zero() {
            self.status("QR expired · press r for a new code");
            return;
        }
        if self.attempts >= 3 {
            self.status(format!("{} · press r and scan again", self.last_error));
            return;
        }
        if Instant::now() < self.next_attempt {
            return;
        }
        // Exact instance matching prevents pairing unrelated nearby devices.
        if let Some(service) = services
            .iter()
            .find(|s| s.kind == ServiceKind::Pairing && s.name == self.session.name)
        {
            self.status("Phone found · pairing…");
            self.log(format!("QR scanned. Pairing with {}", service.address));
            self.attempts += 1;
            self.next_attempt = Instant::now() + Duration::from_secs(5);
            match self.adb.pair(service.address, &self.session.password).await {
                Ok(output) => {
                    self.paired_ip = Some(service.address.ip());
                    self.paired_guid = output
                        .split("[guid=")
                        .nth(1)
                        .and_then(|s| s.split(']').next())
                        .map(str::to_owned);
                    self.last_error.clear();
                    self.status("Paired · finding connection port…");
                    self.log("Pairing succeeded. Looking for this phone's ADB connection service.");
                }
                Err(err) => self.error(format!(
                    "Pairing attempt {}/3 failed: {}",
                    self.attempts,
                    reason(&err)
                )),
            }
        } else {
            self.status("Ready to scan");
        }
    }
}

// ADB errors can span several lines; the last non-empty one usually names the cause.
fn reason(err: &anyhow::Error) -> String {
    let text = err.to_string();
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("unknown error")
        .trim_end_matches('.')
        .to_owned()
}

fn device_service_matches(serial: &str, name: &str) -> bool {
    serial == name
        || serial
            .strip_prefix(name)
            .is_some_and(|suffix| suffix.starts_with("._adb-tls-connect._tcp"))
}

pub async fn run(
    adb: Adb,
    mut sessions: watch::Receiver<Session>,
    mut actions: mpsc::Receiver<Action>,
    tx: mpsc::UnboundedSender<Event>,
) {
    let make_worker = |session| Worker {
        adb: adb.clone(),
        session,
        tx: tx.clone(),
        attempts: 0,
        next_attempt: Instant::now(),
        paired_ip: None,
        paired_guid: None,
        connected: false,
        last_error: String::new(),
    };
    let initial = sessions.borrow_and_update().clone();
    let mut worker = make_worker(initial);
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            changed = sessions.changed() => {
                if changed.is_err() { return; }
                worker = make_worker(sessions.borrow_and_update().clone());
            }
            action = actions.recv() => {
                let Some(Action::Connect(address)) = action else { return; };
                worker.status("Connecting selected device…");
                match worker.adb.connect(address).await {
                    Ok(message) => worker.log(message),
                    Err(err) => worker.error(format!("Connect failed: {}", reason(&err))),
                }
            }
            _ = interval.tick() => {
                // Rotation or shutdown cancels an in-flight operation and kills its child process.
                tokio::select! {
                    _ = worker.refresh() => {},
                    changed = sessions.changed() => {
                        if changed.is_err() { return; }
                        worker = make_worker(sessions.borrow_and_update().clone());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_identity_does_not_match_a_similar_device_name() {
        assert!(device_service_matches(
            "adb-phone._adb-tls-connect._tcp",
            "adb-phone"
        ));
        assert!(device_service_matches("adb-phone", "adb-phone"));
        assert!(!device_service_matches(
            "adb-phone-other._adb-tls-connect._tcp",
            "adb-phone"
        ));
        assert!(!device_service_matches("other", "adb-phone"));
    }

    #[cfg(unix)]
    fn fixture() -> (tempfile::TempDir, Worker) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("adb");
        std::fs::write(&executable, include_str!("../tests/fixtures/fake-adb.sh")).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let session = Session::new();
        std::fs::write(dir.path().join("session"), &session.name).unwrap();
        std::fs::write(dir.path().join("secret"), &session.password).unwrap();
        let (tx, _) = mpsc::unbounded_channel();
        (
            dir,
            Worker {
                adb: Adb { path: executable },
                session,
                tx,
                attempts: 0,
                next_attempt: Instant::now(),
                paired_ip: None,
                paired_guid: None,
                connected: false,
                last_error: String::new(),
            },
        )
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn scan_to_pair_to_connect_uses_distinct_ports_and_verifies_online_state() {
        let (dir, mut worker) = fixture();
        worker.refresh().await;
        assert!(
            !dir.path().join("paired").exists(),
            "Unrelated pairing services must be ignored"
        );
        std::fs::write(dir.path().join("scanned"), "").unwrap();
        worker.refresh().await;
        assert!(dir.path().join("paired").exists());
        assert!(
            !worker.connected,
            "Pairing is not proof of an online device"
        );
        worker.next_attempt = Instant::now();
        worker.refresh().await;
        assert!(dir.path().join("connected").exists());
        assert!(
            !worker.connected,
            "Connect output is not proof of an online device"
        );
        worker.refresh().await;
        assert!(worker.connected);
        let commands = std::fs::read_to_string(dir.path().join("commands")).unwrap();
        assert!(!commands.contains(&worker.session.password));
        assert_eq!(
            commands
                .lines()
                .filter(|line| line.starts_with("pair "))
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn expired_session_never_pairs_even_when_discovered() {
        let (dir, mut worker) = fixture();
        std::fs::write(dir.path().join("scanned"), "").unwrap();
        worker.session.created = Instant::now() - crate::pairing::SESSION_TTL;
        worker.refresh().await;
        assert!(!dir.path().join("paired").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_pair_with_zero_exit_status_is_retried_at_most_three_times() {
        let (dir, mut worker) = fixture();
        std::fs::write(dir.path().join("scanned"), "").unwrap();
        std::fs::write(dir.path().join("fail-pair"), "").unwrap();
        for _ in 0..5 {
            worker.next_attempt = Instant::now();
            worker.refresh().await;
        }
        assert_eq!(worker.attempts, 3);
        assert_eq!(
            worker.last_error,
            "Pairing attempt 3/3 failed: Failed to pair"
        );
        assert!(worker.paired_ip.is_none());
        assert!(!dir.path().join("connected").exists());
    }
}

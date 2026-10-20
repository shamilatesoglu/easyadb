use std::time::{Duration, Instant};

use qrcode::{types::Color, QrCode};
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};

pub const SESSION_TTL: Duration = Duration::from_secs(180);

// Deliberately no Debug implementation: the session contains a pairing secret.
#[derive(Clone)]
pub struct Session {
    pub name: String,
    pub password: String,
    pub created: Instant,
}

impl Session {
    pub fn new() -> Self {
        let token = |length| {
            OsRng
                .sample_iter(&Alphanumeric)
                .take(length)
                .map(char::from)
                .collect::<String>()
        };
        Self {
            name: format!("studio-{}", token(10)),
            password: token(16),
            created: Instant::now(),
        }
    }

    pub fn payload(&self) -> String {
        format!("WIFI:T:ADB;S:{};P:{};;", self.name, self.password)
    }

    pub fn remaining(&self) -> Duration {
        SESSION_TTL.saturating_sub(self.created.elapsed())
    }

    // Four light modules on every side are essential for reliable scanning.
    // Half blocks preserve square QR modules with normal terminal fonts.
    pub fn qr_lines(&self) -> Vec<String> {
        let qr = QrCode::new(self.payload()).expect("fixed length QR payload fits");
        let size = qr.width() + 8;
        let dark = |x: usize, y: usize| {
            x >= 4 && y >= 4 && x < size - 4 && y < size - 4 && qr[(x - 4, y - 4)] == Color::Dark
        };
        (0..size)
            .step_by(2)
            .map(|y| {
                (0..size)
                    .map(|x| match (dark(x, y), dark(x, y + 1)) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        (false, false) => ' ',
                    })
                    .collect()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_are_unique_and_use_androids_wire_format() {
        let a = Session::new();
        let b = Session::new();
        assert_ne!(a.name, b.name);
        assert_ne!(a.password, b.password);
        assert!(a.payload().starts_with("WIFI:T:ADB;S:studio-"));
        assert!(a.payload().ends_with(";;"));
        assert!(a.password.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn qr_has_a_complete_quiet_zone_and_no_clipped_bottom_row() {
        let session = Session::new();
        let qr = session.qr_lines();
        assert!(qr.iter().take(2).all(|row| row.trim().is_empty()));
        assert!(qr.iter().rev().take(2).all(|row| row.trim().is_empty()));
        for row in &qr {
            assert!(row.starts_with("    ") && row.ends_with("    "));
        }
        let width = QrCode::new(session.payload()).unwrap().width() + 8;
        assert_eq!(qr.len(), width.div_ceil(2));
        assert!(qr.iter().all(|row| row.chars().count() == width));
    }

    #[test]
    fn expired_session_has_no_remaining_time() {
        let mut session = Session::new();
        session.created = Instant::now() - SESSION_TTL;
        assert!(session.remaining().is_zero());
    }
}

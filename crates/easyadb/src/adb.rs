use std::{net::SocketAddr, path::PathBuf, process::Stdio, time::Duration};

use anyhow::{bail, Context, Result};
use tokio::{io::AsyncWriteExt, process::Command, time::timeout};

#[derive(Clone)]
pub struct Adb {
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceKind {
    Pairing,
    Connect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    pub kind: ServiceKind,
    pub address: SocketAddr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub serial: String,
    pub state: String,
    pub model: String,
}

pub fn parse_services(output: &str) -> Vec<Service> {
    output
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() != 3 {
                return None;
            }
            let kind = match fields[1].trim_end_matches('.') {
                "_adb-tls-pairing._tcp" => ServiceKind::Pairing,
                "_adb-tls-connect._tcp" => ServiceKind::Connect,
                _ => return None,
            };
            let address: SocketAddr = fields[2].parse().ok()?;
            if address.port() == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
                return None;
            }
            let name = fields[0].split("._adb-tls-").next()?.to_owned();
            Some(Service {
                name,
                kind,
                address,
            })
        })
        .collect()
}

pub fn parse_devices(output: &str) -> Vec<Device> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?;
            let state = fields.next()?;
            if ![
                "device",
                "offline",
                "unauthorized",
                "no",
                "recovery",
                "sideload",
                "bootloader",
            ]
            .contains(&state)
            {
                return None;
            }
            let model = fields
                .find_map(|s| s.strip_prefix("model:"))
                .unwrap_or(serial)
                .replace('_', " ");
            Some(Device {
                serial: serial.to_owned(),
                state: state.to_owned(),
                model,
            })
        })
        .collect()
}

impl Adb {
    pub async fn run(&self, args: &[&str], input: Option<&str>) -> Result<String> {
        self.run_with_timeout(args, input, Duration::from_secs(15))
            .await
    }

    async fn run_with_timeout(
        &self,
        args: &[&str],
        input: Option<&str>,
        limit: Duration,
    ) -> Result<String> {
        let mut command = Command::new(&self.path);
        command
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().with_context(|| {
            format!(
                "Could not launch {}. Install Android SDK Platform-Tools or use --adb /path/to/adb",
                self.path.display()
            )
        })?;
        let output = timeout(limit, async {
            if let Some(input) = input {
                let mut stdin = child.stdin.take().context("ADB stdin unavailable")?;
                stdin.write_all(input.as_bytes()).await?;
                stdin.write_all(b"\n").await?;
                drop(stdin);
            }
            Ok::<_, anyhow::Error>(child.wait_with_output().await?)
        })
        .await
        .context("ADB timed out. Check Wi-Fi, firewall, and wireless debugging")??;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        // Never display a secret even if a vendor adb happens to echo stdin.
        let text = if let Some(secret) = input {
            text.replace(secret, "[redacted]")
        } else {
            text
        };
        let text: String = text
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect();
        if !output.status.success() {
            bail!("{}", text.trim());
        }
        Ok(text.trim().to_owned())
    }

    pub async fn services(&self) -> Result<Vec<Service>> {
        Ok(parse_services(
            &self.run(&["mdns", "services"], None).await?,
        ))
    }

    pub async fn devices(&self) -> Result<Vec<Device>> {
        Ok(parse_devices(&self.run(&["devices", "-l"], None).await?))
    }

    pub async fn pair(&self, endpoint: SocketAddr, password: &str) -> Result<String> {
        // Password goes through stdin, never the process argument list.
        let output = self
            .run(&["pair", &endpoint.to_string()], Some(password))
            .await?;
        if !output.contains("Successfully paired") {
            bail!("Pairing failed: {output}");
        }
        Ok(output)
    }

    pub async fn connect(&self, endpoint: SocketAddr) -> Result<String> {
        let output = self.run(&["connect", &endpoint.to_string()], None).await?;
        if !(output.starts_with("connected to ") || output.starts_with("already connected to ")) {
            bail!("Connection failed: {output}");
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_tls_services_and_valid_addresses() {
        let records = parse_services(
            "List of discovered mdns services\n\
            studio-Abc _adb-tls-pairing._tcp 192.168.1.2:37123\n\
            adb-pixel._adb-tls-connect._tcp.local. _adb-tls-connect._tcp. [fe80::1%4]:4567\n\
            legacy _adb._tcp 192.168.1.2:5555\n\
            bad _adb-tls-connect._tcp 0.0.0.0:123\n\
            bad _adb-tls-connect._tcp 192.168.1.2:0\n\
            invalid _adb-tls-connect._tcp not-an-address",
        );
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].name, "studio-Abc");
        assert_eq!(records[1].name, "adb-pixel");
        assert_eq!(records[1].address.port(), 4567);
    }

    #[test]
    fn parses_devices_without_treating_daemon_output_as_devices() {
        let records = parse_devices(
            "* daemon started successfully\nList of devices attached\n\
            192.168.1.2:42123 device product:foo model:Pixel_9 device:foo transport_id:1\n\
            usb123 unauthorized\n\
            old offline",
        );
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].model, "Pixel 9");
        assert_eq!(records[1].state, "unauthorized");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reports_timeout_and_nonzero_exit() {
        let shell = Adb {
            path: "/bin/sh".into(),
        };
        assert!(shell
            .run_with_timeout(&["-c", "sleep 2"], None, Duration::from_millis(30))
            .await
            .unwrap_err()
            .to_string()
            .contains("timed out"));
        assert!(shell
            .run(&["-c", "echo broken >&2; exit 1"], None)
            .await
            .unwrap_err()
            .to_string()
            .contains("broken"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn pairing_input_is_redacted() {
        let shell = Adb {
            path: "/bin/sh".into(),
        };
        let result = shell
            .run(
                &["-c", "read secret; printf '%s' \"$secret\""],
                Some("123456"),
            )
            .await
            .unwrap();
        assert_eq!(result, "[redacted]");
    }
}

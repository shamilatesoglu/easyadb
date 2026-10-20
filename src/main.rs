mod adb;
mod pairing;
mod ui;
mod worker;

use std::{io::IsTerminal, net::SocketAddr, path::PathBuf};

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use adb::Adb;

#[derive(Parser)]
#[command(
    version,
    about = "Pair Android devices for wireless ADB by scanning a QR code"
)]
struct Cli {
    /// Android SDK Platform-Tools adb executable
    #[arg(long, global = true, env = "EASYADB_ADB", default_value = "adb")]
    adb: PathBuf,
    /// Preview the interface without running ADB
    #[arg(long)]
    demo: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Check ADB, multicast discovery, and connected devices
    Doctor,
    /// List devices known to ADB
    Devices,
    /// Pair using the six-digit code in Android Settings (hidden prompt)
    Pair {
        /// IP:pairing-port from "Pair device with pairing code"
        endpoint: SocketAddr,
        /// Optional IP:connection-port from the main Wireless debugging screen
        #[arg(long)]
        connect: Option<SocketAddr>,
    },
    /// Connect an already-paired device using its connection port
    Connect { endpoint: SocketAddr },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let adb = Adb { path: cli.adb };
    match cli.command {
        Some(Commands::Doctor) => {
            println!("easyadb · diagnostics\n");
            let mut failures = 0;
            for (name, args) in [
                ("Platform-Tools", vec!["version"]),
                ("mDNS backend", vec!["mdns", "check"]),
                ("Nearby services", vec!["mdns", "services"]),
                ("Devices", vec!["devices", "-l"]),
            ] {
                match adb.run(&args, None).await {
                    Ok(output) => println!("{name}\n{output}\n"),
                    Err(err) => {
                        failures += 1;
                        println!("{name}\nERROR: {err}\n");
                    }
                }
            }
            println!("Phone and computer must share a Wi-Fi network. Enable Wireless debugging on Android 11+.\nIf discovery is empty, open the QR scanner or use: easyadb pair IP:PAIRING_PORT");
            if failures > 0 {
                bail!("{failures} diagnostic check(s) failed");
            }
        }
        Some(Commands::Devices) => println!("{}", adb.run(&["devices", "-l"], None).await?),
        Some(Commands::Pair { endpoint, connect }) => {
            let code = rpassword::prompt_password("Six-digit pairing code: ")?;
            let code = code.trim();
            if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
                bail!("Enter the six-digit code from Android's 'Pair device with pairing code' dialog");
            }
            println!("{}", adb.pair(endpoint, code).await?);
            if let Some(endpoint) = connect {
                println!("{}", adb.connect(endpoint).await?);
            } else {
                println!(
                    "ADB may connect automatically. If needed: easyadb connect IP:CONNECTION_PORT"
                );
            }
        }
        Some(Commands::Connect { endpoint }) => println!("{}", adb.connect(endpoint).await?),
        None => {
            if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
                bail!(
                    "The TUI needs an interactive terminal. Try easyadb doctor or easyadb devices."
                );
            }
            if !cli.demo {
                adb.run(&["version"], None).await?;
            }
            ui::run(adb, cli.demo).await?;
        }
    }
    Ok(())
}

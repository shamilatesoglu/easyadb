use std::{collections::VecDeque, time::Duration};

use anyhow::Result;
use crossterm::event::{self, Event as TerminalEvent, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Clear, Paragraph, Wrap},
    Frame,
};
use tokio::sync::{mpsc, watch};

use crate::{
    adb::{Adb, Device, ServiceKind},
    pairing::Session,
    worker::{self, Action, EventKind, Snapshot},
};

const BG: Color = Color::Rgb(14, 19, 28);
const PANEL: Color = Color::Rgb(20, 28, 39);
const INK: Color = Color::Rgb(229, 237, 242);
const MUTED: Color = Color::Rgb(145, 160, 176);
const ACCENT: Color = Color::Rgb(144, 235, 185);
const BORDER: Color = Color::Rgb(48, 65, 78);

struct App {
    session: Session,
    qr: Vec<String>,
    snapshot: Snapshot,
    status: String,
    logs: VecDeque<String>,
    selected: usize,
    help: bool,
    demo: bool,
}

impl App {
    fn new(demo: bool) -> Self {
        let session = Session::new();
        Self {
            qr: session.qr_lines(),
            session,
            snapshot: Snapshot::default(),
            status: "Ready to scan".into(),
            logs: VecDeque::from([
                "Session created. Open Android's wireless debugging QR scanner.".into(),
            ]),
            selected: 0,
            help: false,
            demo,
        }
    }

    fn log(&mut self, message: String) {
        self.logs
            .push_front(message.replace(['\n', '\r', '\t'], " "));
        self.logs.truncate(50);
    }

    fn nearby(&self) -> Vec<&crate::adb::Service> {
        self.snapshot
            .services
            .iter()
            .filter(|s| s.kind == ServiceKind::Connect)
            .collect()
    }
}

pub async fn run(adb: Adb, demo: bool) -> Result<()> {
    let mut app = App::new(demo);
    let (sessions, session_rx) = watch::channel(app.session.clone());
    let (actions, action_rx) = mpsc::channel(4);
    let (events_tx, mut events) = mpsc::unbounded_channel();
    let task = if demo {
        app.snapshot.devices.push(Device {
            serial: "192.168.1.42:39817".into(),
            state: "device".into(),
            model: "Pixel 9 · demo".into(),
        });
        app.log("Preview mode. No ADB commands are running.".into());
        None
    } else {
        Some(tokio::spawn(worker::run(
            adb, session_rx, action_rx, events_tx,
        )))
    };

    let mut terminal = ratatui::try_init()?;
    let result = (|| -> Result<()> {
        loop {
            while let Ok(event) = events.try_recv() {
                if event.session != app.session.name {
                    continue;
                }
                match event.kind {
                    EventKind::Snapshot(snapshot) => app.snapshot = snapshot,
                    EventKind::Status(status) => app.status = status,
                    EventKind::Log(message) => app.log(message),
                }
            }
            terminal.draw(|f| draw(f, &app))?;
            if !event::poll(Duration::from_millis(100))? {
                continue;
            }
            if let TerminalEvent::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    break;
                }
                if app.help {
                    if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                        app.help = false;
                    }
                    continue;
                }
                match key.code {
                    KeyCode::Char('?') => app.help = true,
                    KeyCode::Char('r') => {
                        app.session = Session::new();
                        app.qr = app.session.qr_lines();
                        app.status = "Ready to scan".into();
                        let _ = sessions.send(app.session.clone());
                        app.log("Fresh QR created. Scan this code to pair another device.".into());
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let count = app.nearby().len();
                        if count > 0 {
                            app.selected = (app.selected + 1) % count;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.selected = app.selected.saturating_sub(1)
                    }
                    KeyCode::Enter if !demo => {
                        let nearby = app.nearby();
                        if let Some(service) =
                            nearby.get(app.selected.min(nearby.len().saturating_sub(1)))
                        {
                            let address = service.address;
                            if actions.try_send(Action::Connect(address)).is_err() {
                                app.log(
                                    "A connection request is already queued. Please wait.".into(),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })();
    ratatui::restore();
    if let Some(task) = task {
        task.abort();
        let _ = task.await;
    }
    result
}

fn block(title: &str) -> Block<'_> {
    Block::bordered()
        .title(format!("  {title}  "))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(PANEL).fg(INK))
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(Block::new().style(Style::default().bg(BG).fg(INK)), area);
    let qr_width = app.qr[0].chars().count() as u16;
    let qr_height = app.qr.len() as u16;
    let min_width = qr_width + 44;
    let min_height = qr_height + 17;
    if area.width < min_width || area.height < min_height {
        frame.render_widget(Paragraph::new(format!(
            "easyadb\n\nA little more room to scan.\nResize to at least {min_width} × {min_height} cells.\nCurrent: {} × {}\n\nReduce your terminal font size, or use:\neasyadb pair IP:PAIRING_PORT\n\nq quit", area.width, area.height
        )).style(Style::default().fg(INK)).wrap(Wrap { trim: false }), area.inner(Margin::new(2, 1)));
        return;
    }
    let outer = area.inner(Margin::new(2, 1));
    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(qr_height + 4),
        Constraint::Length(6),
        Constraint::Length(2),
    ])
    .split(outer);
    let badge = if app.demo {
        "   PREVIEW"
    } else {
        "   WIRELESS ADB"
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    "◈ easyadb",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(badge, Style::default().fg(MUTED)),
            ]),
            Line::from(Span::styled(
                "Less cable. More flow.",
                Style::default().fg(MUTED),
            )),
        ]),
        sections[0],
    );
    let body = Layout::horizontal([Constraint::Length(qr_width + 6), Constraint::Min(32)])
        .spacing(2)
        .split(sections[1]);

    frame.render_widget(block("01 / SCAN TO PAIR"), body[0]);
    let expired = app.session.remaining().is_zero();
    let qr_area = Rect::new(body[0].x + 3, body[0].y + 2, qr_width, qr_height);
    if expired {
        frame.render_widget(
            Paragraph::new("This code has expired.\n\nPress r for a fresh QR.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(ACCENT)),
            qr_area,
        );
    } else {
        let lines: Vec<Line> = app.qr.iter().map(|s| Line::from(s.as_str())).collect();
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().fg(Color::Black).bg(Color::White)),
            qr_area,
        );
    }
    let remaining = app.session.remaining().as_secs();
    frame.render_widget(
        Paragraph::new(if expired {
            "EXPIRED".into()
        } else {
            format!(
                "FRESH FOR {:02}:{:02}  ·  r regenerate",
                remaining / 60,
                remaining % 60
            )
        })
        .alignment(Alignment::Center)
        .style(Style::default().fg(MUTED)),
        Rect::new(body[0].x + 1, body[0].bottom() - 2, body[0].width - 2, 1),
    );

    let side = Layout::vertical([Constraint::Length(12), Constraint::Min(6)])
        .spacing(1)
        .split(body[1]);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                &app.status,
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("1  Same Wi-Fi, phone + computer."),
            Line::from("2  Open Developer options."),
            Line::from("3  Turn on Wireless debugging."),
            Line::from("4  Pair device with QR code."),
            Line::from("5  Scan the code on the left."),
            Line::from(""),
            Line::from(Span::styled(
                "Android 11+ · use the Settings scanner",
                Style::default().fg(MUTED),
            )),
        ])
        .block(block("02 / CONNECT"))
        .wrap(Wrap { trim: false }),
        side[0],
    );

    let mut device_lines = vec![];
    for device in &app.snapshot.devices {
        let online = device.state == "device";
        device_lines.push(Line::from(vec![
            Span::styled(
                if online { "● " } else { "○ " },
                Style::default().fg(if online { ACCENT } else { MUTED }),
            ),
            Span::raw(format!(
                "{} · {}",
                device.model,
                if online { "online" } else { &device.state }
            )),
        ]));
        device_lines.push(Line::from(Span::styled(
            format!("  {}", device.serial),
            Style::default().fg(MUTED),
        )));
    }
    if app.snapshot.devices.is_empty() {
        device_lines.push(Line::from(Span::styled(
            "No connected devices yet.",
            Style::default().fg(MUTED),
        )));
    }
    let nearby = app.nearby();
    if !nearby.is_empty() {
        device_lines.push(Line::from(""));
        device_lines.push(Line::from(Span::styled(
            "NEARBY · Enter to connect if paired",
            Style::default().fg(MUTED),
        )));
        for (index, service) in nearby.iter().enumerate() {
            let selected = index == app.selected.min(nearby.len().saturating_sub(1));
            device_lines.push(Line::from(Span::styled(
                format!("{} {}", if selected { "›" } else { " " }, service.address),
                Style::default().fg(if selected { ACCENT } else { INK }),
            )));
        }
    }
    let available = side[1].height.saturating_sub(2) as usize;
    if device_lines.len() > available && available > 0 {
        device_lines.truncate(available);
        device_lines[available - 1] = Line::from(Span::styled(
            "… more devices: easyadb devices",
            Style::default().fg(MUTED),
        ));
    }
    frame.render_widget(
        Paragraph::new(device_lines).block(block("03 / DEVICES")),
        side[1],
    );

    let logs: Vec<Line> = app
        .logs
        .iter()
        .take(4)
        .map(|message| Line::from(format!("  {message}")))
        .collect();
    frame.render_widget(
        Paragraph::new(logs)
            .style(Style::default().fg(MUTED))
            .block(block("ACTIVITY")),
        sections[2],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" r ", Style::default().fg(ACCENT)),
            Span::raw("new QR   "),
            Span::styled(" ↑↓ ", Style::default().fg(ACCENT)),
            Span::raw("select   "),
            Span::styled(" enter ", Style::default().fg(ACCENT)),
            Span::raw("connect   "),
            Span::styled(" ? ", Style::default().fg(ACCENT)),
            Span::raw("help   "),
            Span::styled(" q ", Style::default().fg(ACCENT)),
            Span::raw("quit"),
        ]))
        .style(Style::default().fg(MUTED)),
        sections[3],
    );

    if app.help {
        let popup = Rect::new(
            area.x + (area.width - 74) / 2,
            area.y + (area.height - 23) / 2,
            74,
            23,
        );
        frame.render_widget(Clear, popup);
        frame.render_widget(Paragraph::new(Text::from(
            "\n  SCAN WITH ANDROID SETTINGS\n\n  Developer options → Wireless debugging → Pair device with QR code.\n  Your normal camera app cannot start ADB pairing.\n\n  NO PHONE APPEARING?\n\n  Use the same Wi-Fi. Guest networks, VPNs, and firewalls can block\n  multicast discovery. Try: easyadb doctor\n\n  PAIR WITHOUT DISCOVERY\n\n  In Settings choose Pair device with pairing code, then run:\n  easyadb pair IP:PAIRING_PORT --connect IP:CONNECTION_PORT\n  The two ports are different. The code is entered at a hidden prompt.\n\n  Pairing secrets stay in memory and expire from this TUI after 3 min.\n  Android keeps the computer trusted until you forget it in Settings.\n\n  Esc / ? close"
        )).block(block("A LITTLE HELP")).style(Style::default().fg(INK).bg(PANEL)), popup);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn render(width: u16, height: u16, app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn dashboard_renders_without_exposing_secret_as_text() {
        let app = App::new(false);
        let screen = render(120, 42, &app);
        assert!(screen.contains("SCAN TO PAIR"));
        assert!(screen.contains("Ready to scan"));
        assert!(!screen.contains(&app.session.password));
    }

    #[test]
    fn small_terminal_shows_resize_guidance_instead_of_clipped_qr() {
        let screen = render(60, 20, &App::new(false));
        assert!(screen.contains("Resize to at least"));
        assert!(!screen.contains('█'));
    }

    #[test]
    fn expired_codes_are_not_rendered() {
        let mut app = App::new(false);
        app.session.created = std::time::Instant::now() - crate::pairing::SESSION_TTL;
        assert!(render(120, 42, &app).contains("This code has expired"));
    }
}

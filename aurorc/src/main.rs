#![allow(dead_code)]

mod app;
mod ipc_client;
mod ui;

use shared::{DaemonRequest, DaemonResponse};
use std::error::Error;
use tokio::sync::mpsc;

use app::App;
use ipc_client::IpcClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 1. Initialize terminal raw mode
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // 2. Initialize App State
    let mut app = App::new();

    // 3. Connect to the daemon UNIX domain socket
    let mut client = IpcClient::new();
    let xdg_runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".to_string());
    let socket_path = std::path::PathBuf::from(xdg_runtime_dir).join("aurord.sock");
    let mut is_connected = false;

    #[allow(unused_assignments)]
    let (mut response_tx, mut response_rx) = mpsc::unbounded_channel::<DaemonResponse>();
    let mut reconnect_timer = tokio::time::interval(std::time::Duration::from_secs(1));

    // 4. Spawn a dedicated thread to poll keyboard and mouse events and send to UI loop
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<crossterm::event::Event>();
    std::thread::spawn(move || loop {
        if crossterm::event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(ev) = crossterm::event::read() {
                if event_tx.send(ev).is_err() {
                    break;
                }
            }
        }
    });

    // 5. Main TUI event loop
    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        tokio::select! {
            // Reconnection timer: fires every 1s
            _ = reconnect_timer.tick() => {
                if !is_connected {
                    // Reset channels to start a clean session
                    let (tx, rx) = mpsc::unbounded_channel();
                    response_tx = tx;
                    response_rx = rx;

                    match client.connect(&socket_path, response_tx).await {
                        Ok(_) => {
                            is_connected = true;
                            app.clear_error();
                            // Retrieve initial status and logs from daemon
                            let _ = client.send_request(&DaemonRequest::GetStatus).await;
                            let _ = client.send_request(&DaemonRequest::StreamLogs).await;
                        }
                        Err(_) => {
                            is_connected = false;
                            client.disconnect();
                            app.set_error("Daemon offline. Is aurord running? (retrying...)".to_string());
                        }
                    }
                }
            }

            // Handle incoming message notifications from the daemon
            resp = response_rx.recv(), if is_connected => {
                match resp {
                    Some(DaemonResponse::Status(list)) => {
                        app.update_packages(list);
                    }
                    Some(DaemonResponse::LogLine(line)) => {
                        app.add_log_line(line);
                    }
                    Some(DaemonResponse::UpdateComplete(_pkg_name, _success)) => {
                        // Refresh packages status after an update completes
                        let _ = client.send_request(&DaemonRequest::GetStatus).await;
                    }
                    Some(DaemonResponse::Metadata { uptime_secs, countdown_secs, daemon_state }) => {
                        app.update_metadata(uptime_secs, countdown_secs, daemon_state);
                    }
                    None => {
                        // All senders dropped, meaning the background reader task died
                        if is_connected {
                            is_connected = false;
                            client.disconnect();
                            app.set_error("Daemon disconnected. Reconnecting...".to_string());
                        }
                    }
                }
            }

            // Handle keyboard user interactions
            Some(ev) = event_rx.recv() => {
                if let crossterm::event::Event::Key(key) = ev {
                    // Check KeyEventKind to prevent double trigger on key release
                    if key.kind == crossterm::event::KeyEventKind::Press {
                        match key.code {
                            crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Esc => {
                                break;
                            }
                            crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                                app.next_package();
                            }
                            crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                                app.previous_package();
                            }
                            crossterm::event::KeyCode::Char('u') => {
                                if is_connected {
                                    if let Some(pkg) = app.selected_package() {
                                        let _ = client.send_request(&DaemonRequest::TriggerUpdate(pkg.name.clone())).await;
                                    }
                                }
                            }
                            crossterm::event::KeyCode::Char('a') => {
                                if is_connected {
                                    let _ = client.send_request(&DaemonRequest::TriggerUpdate("all".to_string())).await;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // 6. Cleanup terminal and restore raw mode
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

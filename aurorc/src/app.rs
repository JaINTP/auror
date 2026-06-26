use shared::PackageStatus;

pub struct App {
    pub packages: Vec<PackageStatus>,
    pub logs: Vec<String>,
    pub selected_index: usize,
    pub uptime_secs: u64,
    pub countdown_secs: u64,
    pub daemon_state: String,
    pub error_message: Option<String>,
    pub log_scroll_offset: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            packages: Vec::new(),
            logs: Vec::new(),
            selected_index: 0,
            uptime_secs: 0,
            countdown_secs: 0,
            daemon_state: "Unknown".to_string(),
            error_message: None,
            log_scroll_offset: 0,
        }
    }

    /// Selects the next package in the list, wrapping around.
    pub fn next_package(&mut self) {
        if self.packages.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.packages.len();
    }

    /// Selects the previous package in the list, wrapping around.
    pub fn previous_package(&mut self) {
        if self.packages.is_empty() {
            return;
        }
        if self.selected_index == 0 {
            self.selected_index = self.packages.len() - 1;
        } else {
            self.selected_index -= 1;
        }
    }

    /// Returns the currently selected package status.
    pub fn selected_package(&self) -> Option<&PackageStatus> {
        self.packages.get(self.selected_index)
    }

    /// Updates the package status list, preserving the selection if possible.
    pub fn update_packages(&mut self, list: Vec<PackageStatus>) {
        let current_selected_name = self.selected_package().map(|p| p.name.clone());
        self.packages = list;

        if self.packages.is_empty() {
            self.selected_index = 0;
            return;
        }

        if let Some(name) = current_selected_name {
            if let Some(pos) = self.packages.iter().position(|p| p.name == name) {
                self.selected_index = pos;
            } else {
                self.selected_index = self.selected_index.min(self.packages.len() - 1);
            }
        } else {
            self.selected_index = 0;
        }
    }

    /// Appends a new compilation log line.
    pub fn add_log_line(&mut self, line: String) {
        self.logs.push(line);
        if self.logs.len() > 1000 {
            self.logs.remove(0);
        }
        // Auto-scroll to bottom
        self.log_scroll_offset = 0;
    }

    /// Updates daemon uptime, countdown, and active state.
    pub fn update_metadata(&mut self, uptime: u64, countdown: u64, state: String) {
        self.uptime_secs = uptime;
        self.countdown_secs = countdown;
        self.daemon_state = state;
    }

    /// Sets an error message to display when IPC goes offline.
    pub fn set_error(&mut self, err: String) {
        self.error_message = Some(err);
    }

    /// Clears any current error messages.
    pub fn clear_error(&mut self) {
        self.error_message = None;
    }
}

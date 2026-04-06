//! Interactive TUI for conflict resolution
//!
//! Displays conflicts with side-by-side diff and allows manual resolution.

use crate::sync::{Conflict, ConflictResolution};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::io::{self, stdout};

/// Result of conflict resolution session
#[derive(Debug)]
pub struct ConflictSession {
    pub resolved: Vec<(usize, ConflictResolution)>,
    pub skipped: Vec<usize>,
    pub cancelled: bool,
}

/// Conflict resolution TUI application state
pub struct ConflictApp {
    conflicts: Vec<Conflict>,
    current: usize,
    list_state: ListState,
    resolutions: Vec<Option<ConflictResolution>>,
    show_help: bool,
}

impl ConflictApp {
    /// Create a new conflict resolution app
    pub fn new(conflicts: Vec<Conflict>) -> Self {
        let len = conflicts.len();
        let mut list_state = ListState::default();
        if len > 0 {
            list_state.select(Some(0));
        }
        
        Self {
            conflicts,
            current: 0,
            list_state,
            resolutions: vec![None; len],
            show_help: false,
        }
    }
    
    /// Run the TUI and return resolution results
    pub fn run(mut self) -> io::Result<ConflictSession> {
        // Setup terminal
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
        
        let result = self.main_loop(&mut terminal);
        
        // Restore terminal
        disable_raw_mode()?;
        stdout().execute(LeaveAlternateScreen)?;
        
        result
    }
    
    fn main_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<ConflictSession> {
        loop {
            terminal.draw(|f| self.draw(f))?;
            
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        return Ok(self.build_session(true));
                    }
                    KeyCode::Char('?') | KeyCode::F(1) => {
                        self.show_help = !self.show_help;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.prev_conflict();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.next_conflict();
                    }
                    KeyCode::Char('l') | KeyCode::Left => {
                        self.resolve_current(ConflictResolution::KeepLocal);
                    }
                    KeyCode::Char('r') | KeyCode::Right => {
                        self.resolve_current(ConflictResolution::KeepRemote);
                    }
                    KeyCode::Char('b') => {
                        self.resolve_current(ConflictResolution::KeepBoth);
                    }
                    KeyCode::Char('s') => {
                        // Skip (clear resolution)
                        if self.current < self.resolutions.len() {
                            self.resolutions[self.current] = None;
                        }
                    }
                    KeyCode::Enter => {
                        if self.all_resolved() {
                            return Ok(self.build_session(false));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    
    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        
        if self.show_help {
            self.draw_help(frame, area);
            return;
        }
        
        // Main layout: list on left, details on right
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(area);
        
        self.draw_conflict_list(frame, main_chunks[0]);
        self.draw_conflict_details(frame, main_chunks[1]);
    }
    
    fn draw_help(&self, frame: &mut Frame, area: Rect) {
        let help_text = vec![
            "Conflict Resolution Help",
            "",
            "Navigation:",
            "  j/↓       Next conflict",
            "  k/↑       Previous conflict",
            "",
            "Resolution:",
            "  l/←       Keep LOCAL version",
            "  r/→       Keep REMOTE version",
            "  b         Keep BOTH (create conflict copy)",
            "  s         Skip (leave unresolved)",
            "",
            "Actions:",
            "  Enter     Apply all resolutions and exit",
            "  q/Esc     Cancel and exit",
            "  ?/F1      Toggle this help",
            "",
            "Press any key to close help...",
        ];
        
        let paragraph = Paragraph::new(help_text.join("\n"))
            .block(Block::default().title(" Help ").borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        
        frame.render_widget(paragraph, area);
    }
    
    fn draw_conflict_list(&mut self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self.conflicts.iter().enumerate().map(|(i, c)| {
            let status = match self.resolutions.get(i).and_then(|r| *r) {
                Some(ConflictResolution::KeepLocal) => "[L]",
                Some(ConflictResolution::KeepRemote) => "[R]",
                Some(ConflictResolution::KeepBoth) => "[B]",
                None => "[ ]",
            };
            
            let path = c.path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| c.path.to_string_lossy().to_string());
            
            ListItem::new(format!("{} {}", status, path))
        }).collect();
        
        let resolved = self.resolutions.iter().filter(|r| r.is_some()).count();
        let title = format!(" Conflicts ({}/{}) ", resolved, self.conflicts.len());
        
        let list = List::new(items)
            .block(Block::default().title(title).borders(Borders::ALL))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");
        
        frame.render_stateful_widget(list, area, &mut self.list_state);
    }
    
    fn draw_conflict_details(&self, frame: &mut Frame, area: Rect) {
        if self.conflicts.is_empty() {
            let msg = Paragraph::new("No conflicts to resolve")
                .block(Block::default().title(" Details ").borders(Borders::ALL));
            frame.render_widget(msg, area);
            return;
        }
        
        let conflict = &self.conflicts[self.current];
        
        // Split into path, local info, remote info, and actions
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Path
                Constraint::Min(5),     // Content comparison
                Constraint::Length(3),  // Actions hint
            ])
            .split(area);
        
        // Path header
        let path_block = Paragraph::new(format!("Path: {}", conflict.path.display()))
            .block(Block::default().title(" File ").borders(Borders::ALL));
        frame.render_widget(path_block, chunks[0]);
        
        // Side-by-side comparison
        let compare_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);
        
        // Local info
        let local_info = format!(
            "Size: {} bytes\nModified: {}\nBlocks: {}",
            conflict.local.size(),
            format_time(conflict.local.mtime()),
            conflict.local.blocks().len()
        );
        let local_block = Paragraph::new(local_info)
            .block(Block::default()
                .title(" LOCAL ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)));
        frame.render_widget(local_block, compare_chunks[0]);
        
        // Remote info
        let remote_info = format!(
            "Size: {} bytes\nModified: {}\nBlocks: {}",
            conflict.remote.size(),
            format_time(conflict.remote.mtime()),
            conflict.remote.blocks().len()
        );
        let remote_block = Paragraph::new(remote_info)
            .block(Block::default()
                .title(" REMOTE ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)));
        frame.render_widget(remote_block, compare_chunks[1]);
        
        // Actions hint
        let hint = if self.all_resolved() {
            "Press Enter to apply | l=local r=remote b=both s=skip | ?=help"
        } else {
            "l=keep local | r=keep remote | b=keep both | s=skip | ?=help"
        };
        let actions = Paragraph::new(hint)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        frame.render_widget(actions, chunks[2]);
    }
    
    fn next_conflict(&mut self) {
        if self.conflicts.is_empty() {
            return;
        }
        self.current = (self.current + 1) % self.conflicts.len();
        self.list_state.select(Some(self.current));
    }
    
    fn prev_conflict(&mut self) {
        if self.conflicts.is_empty() {
            return;
        }
        self.current = if self.current == 0 {
            self.conflicts.len() - 1
        } else {
            self.current - 1
        };
        self.list_state.select(Some(self.current));
    }
    
    fn resolve_current(&mut self, resolution: ConflictResolution) {
        if self.current < self.resolutions.len() {
            self.resolutions[self.current] = Some(resolution);
        }
    }
    
    fn all_resolved(&self) -> bool {
        !self.resolutions.is_empty() && self.resolutions.iter().all(|r| r.is_some())
    }
    
    fn build_session(&self, cancelled: bool) -> ConflictSession {
        let mut resolved = Vec::new();
        let mut skipped = Vec::new();
        
        for (i, res) in self.resolutions.iter().enumerate() {
            match res {
                Some(r) => resolved.push((i, *r)),
                None => skipped.push(i),
            }
        }
        
        ConflictSession {
            resolved,
            skipped,
            cancelled,
        }
    }
}

/// Format timestamp for display
fn format_time(mtime: u64) -> String {
    use chrono::{Local, TimeZone};
    if mtime == 0 {
        return "Unknown".to_string();
    }
    Local.timestamp_opt(mtime as i64, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| format!("{}", mtime))
}

/// Run the conflict resolution TUI
pub fn resolve_conflicts(conflicts: Vec<Conflict>) -> io::Result<ConflictSession> {
    ConflictApp::new(conflicts).run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::FileEntry;
    use crate::storage::compute_hash;
    
    fn make_entry(path: &str, data: &[u8], mtime: u64) -> FileEntry {
        let blocks = vec![compute_hash(data)];
        FileEntry::new(path, data.len() as u64, mtime, 0o644, blocks)
    }
    
    fn make_conflict(path: &str) -> Conflict {
        let local = make_entry(path, b"local content", 1000);
        let remote = make_entry(path, b"remote content", 2000);
        Conflict::new(path, local, remote)
    }
    
    #[test]
    fn test_conflict_app_new() {
        let conflicts = vec![
            make_conflict("a.txt"),
            make_conflict("b.txt"),
        ];
        
        let app = ConflictApp::new(conflicts);
        assert_eq!(app.conflicts.len(), 2);
        assert_eq!(app.current, 0);
        assert_eq!(app.resolutions.len(), 2);
    }
    
    #[test]
    fn test_conflict_app_navigation() {
        let conflicts = vec![
            make_conflict("a.txt"),
            make_conflict("b.txt"),
            make_conflict("c.txt"),
        ];
        
        let mut app = ConflictApp::new(conflicts);
        
        assert_eq!(app.current, 0);
        
        app.next_conflict();
        assert_eq!(app.current, 1);
        
        app.next_conflict();
        assert_eq!(app.current, 2);
        
        app.next_conflict();
        assert_eq!(app.current, 0); // Wraps around
        
        app.prev_conflict();
        assert_eq!(app.current, 2); // Wraps around
    }
    
    #[test]
    fn test_conflict_app_resolve() {
        let conflicts = vec![
            make_conflict("a.txt"),
            make_conflict("b.txt"),
        ];
        
        let mut app = ConflictApp::new(conflicts);
        
        app.resolve_current(ConflictResolution::KeepLocal);
        assert_eq!(app.resolutions[0], Some(ConflictResolution::KeepLocal));
        
        app.next_conflict();
        app.resolve_current(ConflictResolution::KeepRemote);
        assert_eq!(app.resolutions[1], Some(ConflictResolution::KeepRemote));
        
        assert!(app.all_resolved());
    }
    
    #[test]
    fn test_conflict_app_build_session() {
        let conflicts = vec![
            make_conflict("a.txt"),
            make_conflict("b.txt"),
            make_conflict("c.txt"),
        ];
        
        let mut app = ConflictApp::new(conflicts);
        
        // Resolve first and third, skip second
        app.resolve_current(ConflictResolution::KeepLocal);
        app.current = 2;
        app.resolve_current(ConflictResolution::KeepBoth);
        
        let session = app.build_session(false);
        
        assert!(!session.cancelled);
        assert_eq!(session.resolved.len(), 2);
        assert_eq!(session.skipped.len(), 1);
        assert_eq!(session.skipped[0], 1);
    }
    
    #[test]
    fn test_format_time() {
        assert_eq!(format_time(0), "Unknown");
        
        // Valid timestamp should produce a formatted date
        let formatted = format_time(1609459200); // 2021-01-01 00:00:00 UTC
        assert!(formatted.contains("2021") || formatted.contains("2020")); // Timezone dependent
    }
    
    #[test]
    fn test_conflict_session_cancelled() {
        let conflicts = vec![make_conflict("a.txt")];
        let app = ConflictApp::new(conflicts);
        
        let session = app.build_session(true);
        assert!(session.cancelled);
    }
}

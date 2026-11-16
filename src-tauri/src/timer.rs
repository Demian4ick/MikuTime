//! TimerService + SessionManager. All times are milliseconds; UI never mutates time.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Status {
    Idle,
    Focus,
    Paused,
    Break,
    AwaitingDecision,
    Completed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Phase {
    Focus,
    ShortBreak,
    LongBreak,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub focus_minutes: u32,
    pub short_break_minutes: u32,
    pub long_break_minutes: u32,
    pub sessions_before_long: u32,
    pub auto_start: bool,
    pub volume: f64,
    pub text_speed: u32,
    pub quiet_mode: bool,
    pub talkativeness: String,
    pub daily_goal_minutes: u32,
    pub always_on_top: bool,
    pub companion_position: Option<(i32, i32)>,
    pub background: Option<String>,
    pub sprites: BTreeMap<String, String>,
    pub voices: BTreeMap<String, String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            sessions_before_long: 4,
            auto_start: false,
            volume: 0.7,
            text_speed: 60,
            quiet_mode: false,
            talkativeness: "Balanced".into(),
            daily_goal_minutes: 120,
            always_on_top: true,
            companion_position: None,
            background: None,
            sprites: BTreeMap::new(),
            voices: BTreeMap::new(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=480).contains(&self.focus_minutes)
            || !(1..=180).contains(&self.short_break_minutes)
            || !(1..=180).contains(&self.long_break_minutes)
            || !(1..=20).contains(&self.sessions_before_long)
            || !self.volume.is_finite()
            || !(0.0..=1.0).contains(&self.volume)
            || self.daily_goal_minutes > 1440
            || self.text_speed > 200
            || !["Quiet", "Balanced", "Chatty", "Disabled"].contains(&self.talkativeness.as_str())
        {
            return Err("Настройки вне допустимых границ".into());
        }
        Ok(())
    }
}

/// Only editable scalar settings; resource paths and positions cannot be patched by the UI.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub focus_minutes: Option<u32>,
    pub short_break_minutes: Option<u32>,
    pub long_break_minutes: Option<u32>,
    pub sessions_before_long: Option<u32>,
    pub auto_start: Option<bool>,
    pub volume: Option<f64>,
    pub text_speed: Option<u32>,
    pub quiet_mode: Option<bool>,
    pub talkativeness: Option<String>,
    pub daily_goal_minutes: Option<u32>,
    pub always_on_top: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    pub color: String,
    pub archived: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
}
impl Segment {
    pub fn duration(&self) -> i64 {
        (self.end_ms - self.start_ms).max(0)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub category_id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub segments: Vec<Segment>,
    pub extensions: u32,
    pub completed: bool,
}
impl Session {
    pub fn work_ms(&self) -> i64 {
        self.segments.iter().map(Segment::duration).sum()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timer {
    pub status: Status,
    pub phase: Phase,
    pub remaining_ms: i64,
    pub deadline_ms: Option<i64>,
    pub segment_started_at: Option<i64>,
    pub session: Option<Session>,
    pub cycle: u32,
}
impl Default for Timer {
    fn default() -> Self {
        Self {
            status: Status::Idle,
            phase: Phase::Focus,
            remaining_ms: 25 * 60_000,
            deadline_ms: None,
            segment_started_at: None,
            session: None,
            cycle: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Database {
    pub schema_version: u32,
    pub saved_at: i64,
    pub settings: Settings,
    pub timer: Timer,
    pub categories: Vec<Category>,
    pub selected_category: String,
    pub sessions: Vec<Session>,
    pub breaks: Vec<Segment>,
}
impl Default for Database {
    fn default() -> Self {
        let names = [
            ("unity", "Юнити", "#57b69c"),
            ("blender", "Блендер", "#dfab72"),
            ("programming", "Программирование", "#55aebe"),
            ("reading", "Чтение", "#bd95bd"),
            ("other", "Другое", "#8ca9b4"),
        ];
        Self {
            schema_version: 1,
            saved_at: 0,
            settings: Settings::default(),
            timer: Timer::default(),
            categories: names
                .iter()
                .map(|(id, name, color)| Category {
                    id: (*id).into(),
                    name: (*name).into(),
                    color: (*color).into(),
                    archived: false,
                })
                .collect(),
            selected_category: "programming".into(),
            sessions: vec![],
            breaks: vec![],
        }
    }
}

impl Database {
    pub fn update_settings(&mut self, patch: SettingsPatch) -> Result<(), String> {
        let mut settings = self.settings.clone();
        macro_rules! apply {
            ($($field:ident),+ $(,)?) => { $(if let Some(value) = patch.$field { settings.$field = value; })+ };
        }
        apply!(
            focus_minutes,
            short_break_minutes,
            long_break_minutes,
            sessions_before_long,
            auto_start,
            volume,
            text_speed,
            quiet_mode,
            talkativeness,
            daily_goal_minutes,
            always_on_top
        );
        settings.validate()?;
        let minutes = settings.focus_minutes;
        self.settings = settings;
        self.set_focus_minutes(minutes)
    }
    pub fn localize_builtin_categories(&mut self) {
        for (id, old, translated) in [
            ("unity", "Unity", "Юнити"),
            ("blender", "Blender", "Блендер"),
            ("programming", "Programming", "Программирование"),
            ("reading", "Reading", "Чтение"),
            ("other", "Other", "Другое"),
        ] {
            if let Some(category) = self
                .categories
                .iter_mut()
                .find(|c| c.id == id && c.name == old)
            {
                category.name = translated.into();
            }
        }
    }
    /// Editing the next focus duration must never reset an existing session or break.
    pub fn set_focus_minutes(&mut self, minutes: u32) -> Result<(), String> {
        if !(1..=480).contains(&minutes) {
            return Err("Время работы должно быть от 1 до 480 минут".into());
        }
        self.settings.focus_minutes = minutes;
        if self.timer.phase == Phase::Focus
            && matches!(self.timer.status, Status::Idle | Status::Completed)
        {
            self.timer.remaining_ms = minutes as i64 * 60_000;
        }
        Ok(())
    }
    /// Arrow clicks are relative and only edit the large, not-yet-started work timer.
    pub fn adjust_focus_minutes(&mut self, delta: i32) -> Result<(), String> {
        if delta != -1 && delta != 1 {
            return Err("Стрелки изменяют время на одну минуту".into());
        }
        if self.timer.phase != Phase::Focus
            || !matches!(self.timer.status, Status::Idle | Status::Completed)
        {
            return Err("Стрелки доступны до запуска работы".into());
        }
        let minutes = (i64::from(self.settings.focus_minutes) + i64::from(delta)).clamp(1, 480);
        self.set_focus_minutes(minutes as u32)
    }
    pub fn remaining(&self, now: i64) -> i64 {
        self.timer
            .deadline_ms
            .map(|end| (end - now).max(0))
            .unwrap_or(self.timer.remaining_ms)
    }
    pub fn running(&self) -> bool {
        matches!(self.timer.status, Status::Focus | Status::Break)
    }

    fn close_segment(&mut self, end: i64) {
        if let Some(start) = self.timer.segment_started_at.take() {
            if end > start {
                let segment = Segment {
                    start_ms: start,
                    end_ms: end,
                };
                if self.timer.phase == Phase::Focus {
                    if let Some(s) = &mut self.timer.session {
                        s.segments.push(segment);
                    }
                } else {
                    self.breaks.push(segment);
                }
            }
        }
    }
    fn run_phase(&mut self, now: i64) {
        self.timer.deadline_ms = Some(now + self.timer.remaining_ms);
        self.timer.segment_started_at = Some(now);
        self.timer.status = if self.timer.phase == Phase::Focus {
            Status::Focus
        } else {
            Status::Break
        };
    }
    fn start_focus(&mut self, now: i64) {
        self.timer.phase = Phase::Focus;
        self.timer.remaining_ms = self.settings.focus_minutes as i64 * 60_000;
        self.timer.session = Some(Session {
            id: format!("{}-{}", now, self.sessions.len()),
            category_id: self.selected_category.clone(),
            started_at: now,
            ended_at: None,
            segments: vec![],
            extensions: 0,
            completed: false,
        });
        self.run_phase(now);
    }
    fn save_session(&mut self, now: i64, completed: bool) {
        if let Some(mut s) = self.timer.session.take() {
            s.ended_at = Some(now);
            s.completed = completed;
            if s.work_ms() > 0 {
                self.sessions.push(s);
            }
        }
    }
    fn begin_break(&mut self, now: i64) {
        self.timer.cycle += 1;
        let long = self.timer.cycle % self.settings.sessions_before_long == 0;
        self.timer.phase = if long {
            Phase::LongBreak
        } else {
            Phase::ShortBreak
        };
        self.timer.remaining_ms = if long {
            self.settings.long_break_minutes
        } else {
            self.settings.short_break_minutes
        } as i64
            * 60_000;
        // Choosing rest starts it immediately. auto_start governs the next work only.
        self.run_phase(now);
    }
    /// Tick is deadline-based, even when the OS misses ticks or suspends rendering.
    pub fn tick(&mut self, now: i64) -> Option<&'static str> {
        if !self.running() {
            return None;
        }
        let deadline = self.timer.deadline_ms?;
        if now < deadline {
            return None;
        }
        self.close_segment(deadline);
        self.timer.remaining_ms = 0;
        self.timer.deadline_ms = None;
        if self.timer.phase == Phase::Focus {
            self.timer.status = Status::AwaitingDecision;
            Some("focus_complete")
        } else {
            self.timer.status = Status::Completed;
            self.timer.phase = Phase::Focus;
            self.timer.remaining_ms = self.settings.focus_minutes as i64 * 60_000;
            if self.settings.auto_start {
                self.start_focus(now);
            }
            Some("break_complete")
        }
    }
    pub fn action(&mut self, action: &str, now: i64) -> Result<Option<&'static str>, String> {
        // A command arriving after the deadline must not account waiting time as focus.
        let tick_event = self.tick(now);
        let event = match action {
            "start" if matches!(self.timer.status, Status::Idle | Status::Completed) => {
                self.start_focus(now);
                Some("start")
            }
            "pause" if self.timer.status == Status::Focus => {
                self.timer.remaining_ms = self.remaining(now);
                self.close_segment(now);
                self.timer.deadline_ms = None;
                self.timer.status = Status::Paused;
                Some("pause")
            }
            "resume" if self.timer.status == Status::Paused && self.timer.phase == Phase::Focus => {
                self.run_phase(now);
                Some("resume")
            }
            "extend" if self.timer.status == Status::AwaitingDecision => {
                let session = self.timer.session.as_mut().ok_or("Нет активной сессии")?;
                session.extensions = session
                    .extensions
                    .checked_add(1)
                    .ok_or("Слишком много продлений")?;
                let first = session.extensions == 1;
                self.timer.remaining_ms = 300_000;
                self.run_phase(now);
                Some(if first {
                    "first_extension"
                } else {
                    "extension"
                })
            }
            "finish" if self.timer.phase == Phase::Focus && self.timer.session.is_some() => {
                self.close_segment(now);
                self.timer.deadline_ms = None;
                self.save_session(now, true);
                self.begin_break(now);
                Some("break_start")
            }
            "skip" if self.timer.phase != Phase::Focus => {
                self.close_segment(now);
                self.timer.deadline_ms = None;
                self.timer.phase = Phase::Focus;
                self.timer.status = Status::Completed;
                self.timer.remaining_ms = self.settings.focus_minutes as i64 * 60_000;
                if self.settings.auto_start {
                    self.start_focus(now);
                }
                Some("break_skipped")
            }
            "reset" => {
                self.close_segment(now);
                self.save_session(now, false);
                self.timer = Timer {
                    cycle: self.timer.cycle,
                    remaining_ms: self.settings.focus_minutes as i64 * 60_000,
                    ..Timer::default()
                };
                Some("reset")
            }
            _ => return Err(format!("Действие {action} недоступно в текущем состоянии")),
        };
        Ok(event.or(tick_event))
    }
    /// Conservative crash/restart recovery. Never count time while app was closed.
    pub fn recover(&mut self, now: i64) -> bool {
        // Migrate old prepared/paused breaks: no work-style Resume is needed.
        if self.timer.status == Status::Paused && self.timer.phase != Phase::Focus {
            self.run_phase(now);
            return true;
        }
        if !self.running() {
            return false;
        }
        let checkpoint = self
            .saved_at
            .max(self.timer.segment_started_at.unwrap_or(self.saved_at));
        self.tick(checkpoint);
        if self.running() {
            self.timer.remaining_ms = self.remaining(checkpoint);
            self.close_segment(checkpoint);
            self.timer.deadline_ms = None;
            self.timer.status = Status::Paused;
            if self.timer.phase != Phase::Focus {
                self.run_phase(now);
            }
        }
        true
    }
    pub fn checkpoint(&self, now: i64) -> Self {
        let mut data = self.clone();
        data.saved_at = now;
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Database {
        let mut d = Database::default();
        d.settings.focus_minutes = 1;
        d
    }
    #[test]
    fn pauses_are_not_work() {
        let mut d = db();
        d.action("start", 1000).unwrap();
        d.action("pause", 11000).unwrap();
        d.action("resume", 51000).unwrap();
        d.tick(101000);
        assert_eq!(d.timer.status, Status::AwaitingDecision);
        assert_eq!(d.timer.session.as_ref().unwrap().work_ms(), 60000);
    }
    #[test]
    fn unlimited_extensions_one_session_and_no_waiting() {
        let mut d = db();
        d.action("start", 1000).unwrap();
        d.tick(61000);
        let id = d.timer.session.as_ref().unwrap().id.clone();
        for i in 0..25 {
            let start = 100000 + i * 400000;
            assert_eq!(
                d.action("extend", start).unwrap(),
                Some(if i == 0 {
                    "first_extension"
                } else {
                    "extension"
                })
            );
            d.tick(start + 300000);
        }
        d.action("finish", 11000000).unwrap();
        assert_eq!(d.sessions.len(), 1);
        assert_eq!(d.sessions[0].id, id);
        assert_eq!(d.sessions[0].work_ms(), 60000 + 25 * 300000);
        assert_eq!(d.sessions[0].extensions, 25);
    }
    #[test]
    fn delayed_tick_caps_work_at_deadline() {
        let mut d = db();
        d.action("start", 1000).unwrap();
        d.tick(999999);
        assert_eq!(d.timer.session.unwrap().work_ms(), 60000);
    }
    #[test]
    fn restart_pauses_at_checkpoint() {
        let mut d = db();
        d.action("start", 1000).unwrap();
        d.saved_at = 21000;
        assert!(d.recover(999999));
        assert_eq!(d.timer.status, Status::Paused);
        assert_eq!(d.remaining(999999), 40000);
        assert_eq!(d.timer.session.unwrap().work_ms(), 20000);
    }
    #[test]
    fn reset_preserves_actual_work_as_partial_record() {
        let mut d = db();
        d.action("start", 1000).unwrap();
        d.action("reset", 11000).unwrap();
        assert_eq!(d.sessions[0].work_ms(), 10000);
        assert!(!d.sessions[0].completed);
    }
    #[test]
    fn long_break_cycle_and_break_accounting() {
        let mut d = db();
        d.settings.sessions_before_long = 2;
        d.settings.auto_start = true;
        d.action("start", 0).unwrap();
        d.action("finish", 10000).unwrap();
        assert_eq!(d.timer.phase, Phase::ShortBreak);
        d.action("skip", 30000).unwrap();
        assert_eq!(d.breaks[0].duration(), 20000);
        d.action("finish", 40000).unwrap();
        assert_eq!(d.timer.phase, Phase::LongBreak);
    }
    #[test]
    fn command_after_deadline_does_not_credit_decision_time() {
        let mut d = db();
        d.action("start", 0).unwrap();
        d.action("finish", 999999).unwrap();
        assert_eq!(d.sessions[0].work_ms(), 60000);
    }
    #[test]
    fn rest_starts_immediately_without_auto_start_and_cannot_be_paused() {
        let mut d = db();
        d.action("start", 0).unwrap();
        assert_eq!(d.action("finish", 10000).unwrap(), Some("break_start"));
        assert_eq!(d.timer.status, Status::Break);
        assert_eq!(d.timer.deadline_ms, Some(310000));
        assert!(d.action("pause", 15000).is_err());
        assert!(d.action("resume", 15000).is_err());
        d.action("skip", 20000).unwrap();
        assert_eq!(d.breaks[0].duration(), 10000);
        assert_eq!(d.sessions[0].work_ms(), 10000);
        assert_eq!(d.timer.status, Status::Completed);
    }
    #[test]
    fn completed_rest_waits_for_next_work_unless_auto_start_is_enabled() {
        let mut d = db();
        d.settings.short_break_minutes = 1;
        d.action("start", 0).unwrap();
        d.action("finish", 10000).unwrap();
        assert_eq!(d.tick(70000), Some("break_complete"));
        assert_eq!(d.timer.status, Status::Completed);
        assert!(d.timer.session.is_none());
        assert_eq!(d.sessions[0].work_ms(), 10000);
        assert_eq!(d.breaks[0].duration(), 60000);
    }
    #[test]
    fn old_paused_break_is_restored_without_work_resume() {
        let mut d = db();
        d.timer.phase = Phase::ShortBreak;
        d.timer.status = Status::Paused;
        d.timer.remaining_ms = 42000;
        assert!(d.recover(100000));
        assert_eq!(d.timer.status, Status::Break);
        assert_eq!(d.timer.deadline_ms, Some(142000));
    }
    #[test]
    fn restarted_break_excludes_offline_time_and_continues_automatically() {
        let mut d = db();
        d.action("start", 0).unwrap();
        d.action("finish", 10000).unwrap();
        d.saved_at = 20000;
        assert!(d.recover(100000));
        assert_eq!(d.timer.status, Status::Break);
        assert_eq!(d.breaks[0].duration(), 10000);
        assert_eq!(d.timer.deadline_ms, Some(390000));
        assert_eq!(d.sessions[0].work_ms(), 10000);
    }
    #[test]
    fn quick_duration_updates_idle_and_completed_only() {
        let mut d = db();
        d.set_focus_minutes(40).unwrap();
        assert_eq!(d.timer.remaining_ms, 40 * 60000);
        d.action("start", 0).unwrap();
        d.set_focus_minutes(10).unwrap();
        assert_eq!(d.timer.deadline_ms, Some(40 * 60000));
        d.action("pause", 10000).unwrap();
        let remaining = d.timer.remaining_ms;
        d.set_focus_minutes(15).unwrap();
        assert_eq!(d.timer.remaining_ms, remaining);
        d.action("finish", 10000).unwrap();
        let rest_end = d.timer.deadline_ms;
        d.set_focus_minutes(30).unwrap();
        assert_eq!(d.timer.deadline_ms, rest_end);
        d.action("skip", 20000).unwrap();
        d.set_focus_minutes(35).unwrap();
        assert_eq!(d.timer.remaining_ms, 35 * 60000);
        assert!(d.set_focus_minutes(0).is_err());
        assert!(d.set_focus_minutes(481).is_err());
        assert_eq!(d.settings.focus_minutes, 35);
    }
    #[test]
    fn default_category_translation_preserves_user_names_and_ids() {
        let mut d = db();
        d.categories[2].name = "Programming".into();
        d.categories[3].name = "My books".into();
        d.localize_builtin_categories();
        assert_eq!(d.categories[2].name, "Программирование");
        assert_eq!(d.categories[2].id, "programming");
        assert_eq!(d.categories[3].name, "My books");
    }
    #[test]
    fn bad_actions_do_not_start_timer() {
        let mut d = db();
        assert!(d.action("extend", 0).is_err());
        assert_eq!(d.timer.status, Status::Idle);
    }
    #[test]
    fn pause_at_deadline_still_transitions_to_decision() {
        let mut d = db();
        d.action("start", 0).unwrap();
        assert!(d.action("pause", 60_001).is_err());
        assert_eq!(d.timer.status, Status::AwaitingDecision);
        assert_eq!(d.timer.session.unwrap().work_ms(), 60_000);
    }
    #[test]
    fn break_tick_caps_rest_and_auto_starts_only_at_wakeup() {
        let mut d = db();
        d.settings.auto_start = true;
        d.settings.short_break_minutes = 1;
        d.action("start", 0).unwrap();
        d.action("finish", 10_000).unwrap();
        d.tick(999_999);
        assert_eq!(d.breaks[0].duration(), 60_000);
        assert_eq!(d.timer.status, Status::Focus);
        assert_eq!(d.timer.segment_started_at, Some(999_999));
    }
    #[test]
    fn duration_arrows_step_large_idle_timer_and_clamp_bounds() {
        let mut d = Database::default();
        d.set_focus_minutes(10).unwrap();
        d.adjust_focus_minutes(1).unwrap();
        assert_eq!(d.settings.focus_minutes, 11);
        assert_eq!(d.timer.remaining_ms, 11 * 60_000);
        d.adjust_focus_minutes(-1).unwrap();
        assert_eq!(d.timer.remaining_ms, 10 * 60_000);
        for _ in 0..3 {
            d.adjust_focus_minutes(1).unwrap();
        }
        assert_eq!(d.timer.remaining_ms, 13 * 60_000);
        d.set_focus_minutes(1).unwrap();
        d.adjust_focus_minutes(-1).unwrap();
        assert_eq!(d.timer.remaining_ms, 60_000);
        d.set_focus_minutes(480).unwrap();
        d.timer.status = Status::Completed;
        d.adjust_focus_minutes(1).unwrap();
        assert_eq!(d.timer.remaining_ms, 480 * 60_000);
        assert!(d.adjust_focus_minutes(0).is_err());
        assert!(d.adjust_focus_minutes(20).is_err());
        assert_eq!(d.settings.focus_minutes, 480);
    }
    #[test]
    fn duration_arrows_do_not_modify_work_pause_or_rest() {
        let mut d = Database::default();
        d.action("start", 1000).unwrap();
        let before = serde_json::to_value(&d).unwrap();
        assert!(d.adjust_focus_minutes(1).is_err());
        assert_eq!(serde_json::to_value(&d).unwrap(), before);
        d.action("pause", 2000).unwrap();
        let before = serde_json::to_value(&d).unwrap();
        assert!(d.adjust_focus_minutes(-1).is_err());
        assert_eq!(serde_json::to_value(&d).unwrap(), before);
        d.action("finish", 3000).unwrap();
        let before = serde_json::to_value(&d).unwrap();
        assert!(d.adjust_focus_minutes(1).is_err());
        assert_eq!(serde_json::to_value(&d).unwrap(), before);
    }
    #[test]
    fn validate_settings() {
        let mut s = Settings::default();
        s.focus_minutes = 0;
        assert!(s.validate().is_err());
    }
    #[test]
    fn text_speed_defaults_for_legacy_settings_and_validates_boundaries() {
        let mut settings: Settings = serde_json::from_str(r#"{"focusMinutes":40}"#).unwrap();
        assert_eq!(settings.text_speed, 60);
        assert_eq!(settings.focus_minutes, 40);
        for speed in [0, 1, 200] {
            settings.text_speed = speed;
            assert!(settings.validate().is_ok());
        }
        settings.text_speed = 201;
        assert!(settings.validate().is_err());
    }
    #[test]
    fn settings_patches_preserve_other_fields_resources_and_active_timer() {
        let mut d = Database::default();
        d.settings.background = Some("original.png".into());
        d.action("start", 1000).unwrap();
        let timer = serde_json::to_value(&d.timer).unwrap();
        d.update_settings(SettingsPatch {
            focus_minutes: Some(42),
            ..Default::default()
        })
        .unwrap();
        d.update_settings(SettingsPatch {
            text_speed: Some(0),
            volume: Some(0.2),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(d.settings.focus_minutes, 42);
        assert_eq!(d.settings.text_speed, 0);
        assert_eq!(d.settings.volume, 0.2);
        assert_eq!(d.settings.background.as_deref(), Some("original.png"));
        assert_eq!(serde_json::to_value(&d.timer).unwrap(), timer);
        assert!(serde_json::from_str::<SettingsPatch>(r#"{"background":"bad.png"}"#).is_err());
    }
    #[test]
    fn invalid_patch_is_atomic_and_idle_duration_updates_immediately() {
        let mut d = Database::default();
        let original = serde_json::to_value(&d.settings).unwrap();
        assert!(d
            .update_settings(SettingsPatch {
                focus_minutes: Some(99),
                text_speed: Some(201),
                ..Default::default()
            })
            .is_err());
        assert_eq!(serde_json::to_value(&d.settings).unwrap(), original);
        d.update_settings(SettingsPatch {
            focus_minutes: Some(42),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(d.timer.remaining_ms, 42 * 60_000);
    }
}

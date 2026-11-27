//! Single-service Codex quota scheduler used by production.

use crate::models::{AppUsageData, UsageData};
use crate::poller::PollError;
use crate::quota_refresh::{self, History, ResetRefresh};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

pub struct Service {
    pub data: Option<UsageData>,
    pub history: History,
    pub error: Option<PollError>,
    pub interval_ms: u32,
    in_flight: Option<u64>,
    next_ticket: u64,
    forced: bool,
    retries: u32,
    due: Instant,
    retry_not_before: Option<Instant>,
    auth_paused: bool,
    reset: ResetRefresh,
}

#[derive(Clone)]
pub struct Job {
    pub id: usize,
    generation: u64,
    ticket: u64,
    forced: bool,
    auth_paused: bool,
}

pub struct Attempt {
    result: Option<Result<UsageData, PollError>>,
}

impl Attempt {
    pub fn from_result(result: Result<UsageData, PollError>) -> Self {
        Self {
            result: Some(result),
        }
    }

    pub fn cancelled() -> Self {
        Self { result: None }
    }
}

pub struct Completion {
    pub accepted: bool,
    pub notify_auth: bool,
    pub successful: bool,
}

pub struct Monitor {
    pub service: Service,
}

fn is_auth(error: PollError) -> bool {
    matches!(error, PollError::AuthRequired)
}

fn next_reset_ms(data: &UsageData, wall: SystemTime) -> Option<u32> {
    [data.session.resets_at, data.weekly.resets_at]
        .into_iter()
        .flatten()
        .filter_map(|time| time.duration_since(wall).ok())
        .filter(|delay| !delay.is_zero())
        .min()
        .map(|delay| delay.as_millis().clamp(1000, u32::MAX as u128) as u32)
}

pub fn execute(job: &Job, poll: impl FnOnce() -> Result<UsageData, PollError>) -> Attempt {
    if job.auth_paused && !job.forced {
        return Attempt::cancelled();
    }
    Attempt::from_result(poll())
}

pub fn launch(
    jobs: Vec<Job>,
    poll: impl Fn(&Job) -> Attempt + Send + Sync + 'static,
    complete: impl Fn(&Job, Attempt) + Send + Sync + 'static,
) {
    let poll = Arc::new(poll);
    let complete = Arc::new(complete);
    for job in jobs {
        let worker_job = job.clone();
        let worker_poll = poll.clone();
        let worker_complete = complete.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("quota-codex".to_string())
            .spawn(move || worker_complete(&worker_job, worker_poll(&worker_job)))
        {
            crate::diagnose::log_error("unable to start Codex quota query", error);
            complete(&job, Attempt::from_result(Err(PollError::RequestFailed)));
        }
    }
}

impl Monitor {
    pub fn new(now: Instant) -> Self {
        Self {
            service: Service {
                data: None,
                history: History::default(),
                error: None,
                interval_ms: 900_000,
                in_flight: None,
                next_ticket: 0,
                forced: false,
                retries: 0,
                due: now,
                retry_not_before: None,
                auth_paused: false,
                reset: ResetRefresh::default(),
            },
        }
    }

    pub fn job_is_current(&self, job: &Job) -> bool {
        self.service.in_flight == Some(job.ticket) && self.service.generation() == job.generation
    }

    pub fn force(&mut self, now: Instant, include_auth: bool) {
        self.service.due = self
            .service
            .retry_not_before
            .map_or(now, |deadline| deadline.max(now));
        self.service.forced |= include_auth || !self.service.auth_paused;
    }

    pub fn wake(&mut self, now: Instant, base: u32) -> (Vec<Job>, u32) {
        let jobs = self.plan(now);
        (jobs, self.delay_ms(now, base))
    }

    fn plan(&mut self, now: Instant) -> Vec<Job> {
        let s = &mut self.service;
        if s.in_flight.is_some() || s.due > now {
            return Vec::new();
        }
        s.next_ticket = s.next_ticket.wrapping_add(1);
        s.in_flight = Some(s.next_ticket);
        vec![Job {
            id: 1,
            generation: 0,
            ticket: s.next_ticket,
            forced: std::mem::take(&mut s.forced),
            auth_paused: s.auth_paused,
        }]
    }

    pub fn finish(
        &mut self,
        job: &Job,
        attempt: Attempt,
        now: Instant,
        wall: SystemTime,
        base: u32,
        adaptive: bool,
    ) -> Completion {
        let s = &mut self.service;
        if s.in_flight != Some(job.ticket) {
            return Completion {
                accepted: false,
                successful: false,
                notify_auth: false,
            };
        }
        s.in_flight = None;
        let mut notify_auth = false;
        let mut successful = false;
        match attempt.result {
            Some(Ok(data)) => {
                successful = true;
                s.retries = 0;
                s.error = None;
                s.auth_paused = false;
                s.retry_not_before = None;
                let reset = [data.session.resets_at, data.weekly.resets_at]
                    .into_iter()
                    .flatten()
                    .filter(|t| *t <= wall)
                    .max();
                s.interval_ms = quota_refresh::interval_ms(base, adaptive, Some(&data));
                if let Some(ms) = s.reset.interval_ms(reset, now) {
                    s.interval_ms = s.interval_ms.min(ms);
                }
                if let Some(ms) = next_reset_ms(&data, wall) {
                    s.interval_ms = s.interval_ms.min(ms);
                }
                s.history.record_success(wall, s.interval_ms);
                s.data = Some(data);
            }
            Some(Err(error)) => {
                notify_auth = is_auth(error) && (s.error != Some(error) || job.forced);
                s.error = Some(error);
                s.retries = s.retries.saturating_add(1);
                s.auth_paused = is_auth(error);
                s.interval_ms = if s.auth_paused {
                    base
                } else {
                    quota_refresh::retry_interval_ms(base, s.retries)
                };
            }
            None => s.interval_ms = base,
        }
        s.due = if s.forced {
            now
        } else {
            now + Duration::from_millis(s.interval_ms as u64)
        };
        Completion {
            accepted: true,
            successful,
            notify_auth,
        }
    }

    pub fn reconfigure(&mut self, base: u32, adaptive: bool, now: Instant, wall: SystemTime) {
        let s = &mut self.service;
        if s.error.is_none() {
            s.interval_ms = quota_refresh::interval_ms(base, adaptive, s.data.as_ref());
            if let Some(ms) = s.data.as_ref().and_then(|data| next_reset_ms(data, wall)) {
                s.interval_ms = s.interval_ms.min(ms);
            }
            s.history.expected_interval_ms = s.interval_ms;
            if !s.forced {
                s.due = now + Duration::from_millis(s.interval_ms as u64);
            }
        }
    }

    pub fn delay_ms(&self, now: Instant, base: u32) -> u32 {
        if self.service.in_flight.is_none() {
            self.service
                .due
                .saturating_duration_since(now)
                .as_millis()
                .clamp(1000, u32::MAX as u128) as u32
        } else {
            base
        }
    }

    pub fn cached(&self) -> AppUsageData {
        AppUsageData {
            codex: self.service.data.clone(),
        }
    }
}

impl Service {
    pub fn description(&self, now: SystemTime, _monotonic: Instant, chinese: bool) -> String {
        self.history
            .description(now, chinese, self.error.is_none(), self.interval_ms)
    }

    pub fn stale(&self, now: SystemTime) -> bool {
        self.error.is_some() || self.history.is_stale(now)
    }
    fn generation(&self) -> u64 {
        0
    }
}

#![forbid(unsafe_code)]

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppConfig {
    pub mode: AppMode,
    pub qa_exit: Option<QaExit>,
    pub force_warp: bool,
    pub simulate_device_loss_once: bool,
    pub simulate_lifecycle_events: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppMode {
    Bootstrap,
    Showcase,
    WindowSmoke,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QaExit {
    millis: u64,
}

impl QaExit {
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self { millis }
    }

    #[must_use]
    pub const fn millis(self) -> u64 {
        self.millis
    }
}

#[must_use]
pub const fn crate_identity() -> &'static str {
    "shell-app"
}

#[must_use]
pub fn parse_args<const N: usize>(args: [&str; N]) -> AppConfig {
    let mut mode = AppMode::Bootstrap;
    let mut qa_exit = None;
    let mut force_warp = false;
    let mut simulate_device_loss_once = false;
    let mut simulate_lifecycle_events = false;
    let mut index = 1;

    while index < args.len() {
        match args[index] {
            "--showcase" => mode = AppMode::Showcase,
            "--window-smoke" => mode = AppMode::WindowSmoke,
            "--force-warp" => force_warp = true,
            "--simulate-device-loss-once" => simulate_device_loss_once = true,
            "--simulate-lifecycle-events" => simulate_lifecycle_events = true,
            "--qa-exit-ms" => {
                if let Some(raw) = args
                    .get(index + 1)
                    .and_then(|value| value.parse::<u64>().ok())
                {
                    qa_exit = Some(QaExit::from_millis(raw));
                    index += 1;
                }
            }
            _ => {}
        }
        index += 1;
    }

    AppConfig {
        mode,
        qa_exit,
        force_warp,
        simulate_device_loss_once,
        simulate_lifecycle_events,
    }
}

#[must_use]
pub fn parse_env_args() -> AppConfig {
    let args = std::env::args().collect::<Vec<_>>();
    parse_arg_slice(&args)
}

#[must_use]
pub fn parse_arg_slice(args: &[String]) -> AppConfig {
    let mut mode = AppMode::Bootstrap;
    let mut qa_exit = None;
    let mut force_warp = false;
    let mut simulate_device_loss_once = false;
    let mut simulate_lifecycle_events = false;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--showcase" => mode = AppMode::Showcase,
            "--window-smoke" => mode = AppMode::WindowSmoke,
            "--force-warp" => force_warp = true,
            "--simulate-device-loss-once" => simulate_device_loss_once = true,
            "--simulate-lifecycle-events" => simulate_lifecycle_events = true,
            "--qa-exit-ms" => {
                if let Some(raw) = args
                    .get(index + 1)
                    .and_then(|value| value.parse::<u64>().ok())
                {
                    qa_exit = Some(QaExit::from_millis(raw));
                    index += 1;
                }
            }
            _ => {}
        }
        index += 1;
    }

    AppConfig {
        mode,
        qa_exit,
        force_warp,
        simulate_device_loss_once,
        simulate_lifecycle_events,
    }
}

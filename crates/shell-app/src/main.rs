#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = shell_app::parse_env_args();
    match config.mode {
        shell_app::AppMode::Bootstrap => {
            println!("shell-app: bootstrap complete; exiting cleanly");
            Ok(())
        }
        shell_app::AppMode::Showcase => {
            let session_id = shell_platform_windows::current_session_id()?;
            let lock_path = shell_app::instance_lock_path(
                &shell_platform_windows::local_app_data_path()?,
                session_id,
            );
            let instance = match shell_app::acquire_instance_lock(&lock_path)? {
                shell_app::InstanceOwnership::Primary(lock) => lock,
                shell_app::InstanceOwnership::Existing => {
                    match shell_platform_windows::activate_existing_instance() {
                        Ok(()) => return Ok(()),
                        Err(activation_error) => {
                            match shell_app::acquire_instance_lock(&lock_path)? {
                                shell_app::InstanceOwnership::Primary(lock) => lock,
                                shell_app::InstanceOwnership::Existing => {
                                    return Err(activation_error.into());
                                }
                            }
                        }
                    }
                }
            };
            let _instance = instance;
            shell_platform_windows::run_showcase(showcase_run_config(&config)?)?;
            Ok(())
        }
        shell_app::AppMode::WindowSmoke => {
            shell_platform_windows::run_showcase(showcase_run_config(&config)?)?;
            Ok(())
        }
    }
}

fn showcase_run_config(
    config: &shell_app::AppConfig,
) -> Result<shell_platform_windows::ShowcaseRunConfig, std::num::TryFromIntError> {
    let qa_exit_ms = config
        .qa_exit
        .map(|value| value.millis().clamp(100, 60_000))
        .map(u32::try_from)
        .transpose()?;
    Ok(shell_platform_windows::ShowcaseRunConfig {
        force_warp: config.force_warp,
        qa_exit_ms,
        simulate_device_loss_once: config.simulate_device_loss_once,
        simulate_lifecycle_events: config.simulate_lifecycle_events,
        safe_mode: config.safe_mode,
        high_contrast: config.high_contrast,
        reduced_motion: config.reduced_motion || config.safe_mode,
        liquid_glass: config.liquid_glass && !config.safe_mode && !config.high_contrast,
        watchdog_heartbeat: config.watchdog_child,
    })
}

#[cfg(test)]
mod tests {
    use shell_app::parse_args;

    use super::showcase_run_config;

    #[test]
    fn release_windows_binary_declares_the_gui_subsystem() {
        assert!(include_str!("main.rs").contains(
            "#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = \"windows\")]"
        ));
    }

    #[test]
    fn safe_mode_keeps_hardware_first_and_reduces_motion() {
        let config = parse_args(["shell-app.exe", "--safe-mode"]);

        let run = showcase_run_config(&config).expect("safe mode config should be valid");

        assert!(run.safe_mode);
        assert!(!run.force_warp);
        assert!(run.reduced_motion);
    }

    #[test]
    fn explicit_warp_can_be_combined_with_safe_mode() {
        let config = parse_args(["shell-app.exe", "--safe-mode", "--force-warp"]);

        let run = showcase_run_config(&config).expect("combined config should be valid");

        assert!(run.safe_mode);
        assert!(run.force_warp);
        assert!(run.reduced_motion);
    }

    #[test]
    fn safe_and_high_contrast_modes_suppress_liquid_glass() {
        let safe = showcase_run_config(&parse_args([
            "shell-app.exe",
            "--liquid-glass",
            "--safe-mode",
        ]))
        .expect("safe config should be valid");
        let contrast = showcase_run_config(&parse_args([
            "shell-app.exe",
            "--liquid-glass",
            "--high-contrast",
        ]))
        .expect("contrast config should be valid");

        assert!(!safe.liquid_glass);
        assert!(!contrast.liquid_glass);
    }
}

#![forbid(unsafe_code)]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = shell_app::parse_env_args();
    match config.mode {
        shell_app::AppMode::Bootstrap => {
            println!("shell-app: bootstrap complete; exiting cleanly");
            Ok(())
        }
        shell_app::AppMode::Showcase | shell_app::AppMode::WindowSmoke => {
            let qa_exit = config
                .qa_exit
                .map(|value| value.millis().clamp(100, 60_000))
                .map(u32::try_from)
                .transpose()?;
            shell_platform_windows::run_showcase(
                config.force_warp,
                qa_exit,
                config.simulate_device_loss_once,
                config.simulate_lifecycle_events,
            )?;
            Ok(())
        }
    }
}

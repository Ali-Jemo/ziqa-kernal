#![forbid(clippy::unwrap_used)]
#![forbid(clippy::expect_used)]

use crate::core::Orbital;
#[cfg(not(feature = "ziqa-bga-direct"))]
use log::warn;
use log::{debug, error, info};
use redox_log::{OutputBuilder, RedoxLogger};
use std::{process::Command, rc::Rc};

use config::Config;
use scheme::OrbitalScheme;

mod compositor;
mod config;
mod core;
#[cfg(feature = "ziqa-bga-direct")]
mod native_shell;
#[cfg(feature = "ziqa-bga-direct")]
mod prof; // PROF-TEMP: remove to revert instrumentation
mod scheme;
mod widget;
mod window;
mod window_order;
#[cfg(feature = "ziqa-bga-direct")]
mod ziqa_input;

/// Run Orbital's main event loop.
fn orbital() -> Result<(), String> {
    // Ignore possible errors while enabling logging
    let _ = RedoxLogger::new()
        .with_output(
            OutputBuilder::stdout()
                .with_filter(log::LevelFilter::Warn)
                .with_ansi_escape_codes()
                .build(),
        )
        .with_process_name("orbital".into())
        .enable();

    #[cfg(feature = "ziqa-bga-direct")]
    let mut login_cmd = Command::new("ziqa-native-shell");

    #[cfg(not(feature = "ziqa-bga-direct"))]
    let mut login_cmd = {
        let mut args = std::env::args().skip(1);
        let vt = std::env::var("VT").expect("`VT` environment variable not set");
        unsafe {
            std::env::remove_var("VT");
        }
        let login_cmd = args.next().ok_or("no login manager argument")?;

        match Command::new("inputd").arg("-A").arg(&vt).status() {
            Ok(status) => {
                if !status.success() {
                    warn!("inputd -A '{}' exited with status: {:?}", vt, status);
                }
            }
            Err(err) => {
                warn!("inputd -A '{}' failed to run with error: {}", vt, err);
            }
        }

        let mut command = Command::new(login_cmd);
        command.args(args);
        command
    };

    let (orbital, displays) =
        Orbital::open_display().map_err(|e| format!("could not open display, caused by: {}", e))?;

    debug!(
        "found display {}x{}",
        displays.displays[0].screen_rect().width(),
        displays.displays[0].screen_rect().height()
    );
    #[cfg(feature = "ziqa-bga-direct")]
    let config = Rc::new(Config::default());
    #[cfg(not(feature = "ziqa-bga-direct"))]
    let config = Rc::new(Config::from_path("/ui/orbital.toml"));
    let scheme = OrbitalScheme::new(displays, config)?;

    orbital
        .run(scheme, &mut login_cmd)
        .map_err(|e| format!("error in main loop, caused by {}", e))
}

/// Start orbital. This will start orbital main event loop.
///
/// Startup messages and errors are logged to RedoxLogger with filter set to DEBUG
fn main() {
    redox_log::RedoxLogger::init_timezone();
    match orbital() {
        Ok(()) => {
            info!("ran to completion successfully, exiting with status=0");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("orbital: {e}");
            error!("error during daemon execution, exiting with status=1: {e}");
            std::process::exit(1);
        }
    }
}

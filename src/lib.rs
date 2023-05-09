pub mod discordstreamer;

use gst::glib;
use tokio::runtime;
use once_cell::sync::Lazy;

pub static RUNTIME: Lazy<runtime::Runtime> = Lazy::new(|| {
    runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
});

fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    discordstreamer::register(plugin)?;
    Ok(())
}

gst::plugin_define!(
    discordstreamer,
    env!("CARGO_PKG_DESCRIPTION"),
    plugin_init,
    concat!(env!("CARGO_PKG_VERSION"), "-", env!("COMMIT_ID")),
    "MIT/X11",
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_REPOSITORY"),
    env!("BUILD_REL_DATE")
);
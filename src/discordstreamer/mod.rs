mod imp;

use gst::glib;
use gst::prelude::*;
use gst::glib::StaticType;

glib::wrapper! {
    pub struct DiscordStreamer(ObjectSubclass<imp::DiscordStreamer>) @extends gst::Element, gst::Object;
}

impl Default for DiscordStreamer {
    fn default() -> Self {
        glib::Object::new()
    }
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    DiscordStreamer::static_type().mark_as_plugin_api(gst::PluginAPIFlags::empty());
    gst::Element::register(
        Some(plugin),
        "discordstreamer",
        gst::Rank::None,
        DiscordStreamer::static_type(),
    )
}
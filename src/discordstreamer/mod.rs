mod imp;

use gst::glib;
use gst::glib::StaticType;

glib::wrapper! {
    pub struct DiscordStreamer(ObjectSubclass<imp::DiscordStreamer>) @extends gst::Element, gst::Object;
}

impl Default for DiscordStreamer {
    fn default() -> Self {
        glib::Object::new::<Self>()
    }
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "discordstreamer",
        gst::Rank::None,
        DiscordStreamer::static_type(),
    )
}
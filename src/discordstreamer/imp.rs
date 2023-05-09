use std::ptr::NonNull;
use std::sync::Mutex;
use gst::glib;
use gst::glib::subclass::TypeData;
use gst::glib::Type;
use gst::subclass::prelude::*;
use once_cell::sync::Lazy;
use tokio::runtime;

pub static RUNTIME: Lazy<runtime::Runtime> = Lazy::new(|| {
    runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
});

pub static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "discordstreamer",
        gst::DebugColorFlags::empty(),
        Some(env!("CARGO_PKG_DESCRIPTION")),
    )
});

#[derive(Default)]
struct State {
}

#[derive(Default)]
pub struct DiscordStreamer {
    state: Mutex<State>
}

#[glib::object_subclass]
impl ObjectSubclass for DiscordStreamer {
    const NAME: &'static str = "DiscordStreamer";
    type Type = super::DiscordStreamer;
    type ParentType = gst::Element;

    fn with_class(klass: &Self::Class) -> Self {
        Self {
            state: Mutex::new(Default::default())
        }
    }

}

impl ObjectImpl for DiscordStreamer {}

impl GstObjectImpl for DiscordStreamer {}

impl ElementImpl for DiscordStreamer {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
            gst::subclass::ElementMetadata::new(
                "DiscordStreamer",
                "Sink/Video/Audio",
                env!("CARGO_PKG_DESCRIPTION"),
                "Lorenzo Rizzotti <dev@dreaming.codes>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }
}
